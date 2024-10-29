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
        ignore_auth_fail: bool,
    ) -> Result<Response<Body>, anyhow::Error> {
        self.request(
            uri,
            method,
            |method, c| async { c.method(method).body(Body::from("")).map_err(|e| e.into()) },
            retries,
            ignore_auth_fail
        )
        .await
    }

    pub async fn request<Fut, F, B>(
        &self,
        uri: &Uri,
        context: B,
        complete_request: F,
        retries: usize,
        // Sometimes when we fetch the root of a repository, we might fail with an auth error
        // not then the rest of the flow/targeted paths are fine. 
        ignore_auth_fail: bool,
    ) -> Result<Response<Body>, anyhow::Error>
    where
        F: Fn(B, http::request::Builder) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<http::request::Request<Body>>>,
        B: Send + 'static + Sync + Clone,
    {
        let mut uri = uri.clone();
        let mut attempt = 0;
        let error = loop {
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
                    if attempt >= retries {
                        break err;
                    }
                    attempt += 1;

                    // Unwrap safe because we set the line right before this.
                    match err {
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
                        RequestFailType::HyperError(_) => break err, // terminal.
                        RequestFailType::AnyhowError(_) => break err, // terminal.
                        RequestFailType::AuthFailure(resp, auth_fail) => {
                            if ignore_auth_fail {
                                return Ok(resp);
                            }
                            let auth_info = authentication_flow::authenticate_request(
                                &auth_fail,
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
        };
        Err(error).with_context(|| {
            format!(
                "Exhausted attempts, or ran into terminal error issuing http requests to URI: {:?}",
                uri
            )
        })
    }
}
