mod authentication_flow;
mod private_impl;

use std::sync::Arc;

use anyhow::Context;

use http::Response;
use http::Uri;

use hyper::{Body, Client};
use tokio::sync::Mutex;

use crate::registry::DockerAuthenticationHelper;

use self::authentication_flow::AuthResponse;
use self::private_impl::{run_single_request, RequestFailType};

// https://raw.githubusercontent.com/google/go-containerregistry/main/images/credhelper-basic.svg
pub struct HttpCli {
    pub inner_client: Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
    pub auth_info: Arc<Mutex<Option<AuthResponse>>>,
    pub docker_authorization_helpers: Arc<Vec<DockerAuthenticationHelper>>,
}

impl HttpCli {
    pub async fn request_simple(
        &self,
        uri: &Uri,
        method: http::Method,
        retries: usize,
    ) -> Result<Response<Body>, anyhow::Error> {
        self.request(
            uri,
            method,
            |method, c| async { c.method(method).body(Body::from("")).map_err(|e| e.into()) },
            retries,
        )
        .await
    }

    pub async fn request<Fut, F, B>(
        &self,
        uri: &Uri,
        context: B,
        complete_request: F,
        retries: usize,
    ) -> Result<Response<Body>, anyhow::Error>
    where
        F: Fn(B, http::request::Builder) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<http::request::Request<Body>>>,
        B: Send + 'static + Sync + Clone,
    {
        let mut uri = uri.clone();
        let mut attempt = 0;
        let mut last_error: Option<RequestFailType> = None;
        while attempt < retries + 1 {
            attempt += 1;
            match run_single_request(
                self.auth_info.clone(),
                &uri,
                context.clone(),
                &complete_request,
                &self.inner_client,
            )
            .await
            {
                Ok(o) => return Ok(o),
                Err(err) => {
                    last_error = Some(err);
                    // Unwrap safe because we set the line right before this.
                    match &last_error.as_ref().unwrap() {
                        RequestFailType::Redirection(new_url) => {
                            let new_uri = new_url.parse::<Uri>().with_context(|| {
                                format!("Failed to parse new url {:?}", new_url)
                            })?;
                            // Sometimes we can receive new URI's that don't contain hosts
                            // we need to supply this information from the last URI we used in that case
                            if new_uri.host().is_some() {
                                uri = new_uri;
                            } else {
                                let mut parts = uri.into_parts();
                                parts.path_and_query = new_uri.path_and_query().cloned();
                                uri = Uri::from_parts(parts).with_context(|| {
                                    format!(
                                        "Constructed an invalid uri from parts, new uri: {:?}",
                                        new_uri
                                    )
                                })?;
                            }
                            continue;
                        }
                        RequestFailType::ConnectError(_) => continue,
                        RequestFailType::HyperError(_) => break, // terminal.
                        RequestFailType::AnyhowError(_) => break, // terminal.
                        RequestFailType::AuthFailure(auth_fail) => {
                            let auth_info = authentication_flow::authenticate_request(
                                auth_fail,
                                &self.inner_client,
                                self.docker_authorization_helpers.clone(),
                            )
                            .await?;
                            let mut ai = self.auth_info.lock().await;
                            *ai = Some(auth_info);
                            drop(ai);
                            attempt -= 1;
                            continue;
                        }
                    }
                }
            }
        }
        match last_error {
            None => anyhow::bail!("We failed in trying to issue http requests, but we have no last error. Unexpected state. Attempting to query: {:?}", uri),
            Some(ex) =>
                Err(ex).with_context(|| format!("Exhausted attempts, or ran into terminal error issuing http requests to URI: {:?}", uri))
        }
    }
}
