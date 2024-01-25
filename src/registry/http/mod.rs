mod blob;
mod copy_operations;
mod http_cli;
mod util;
use bytes::Bytes;
use http_cli::HttpCli;
use std::time::Duration;

use crate::container_specs::manifest::Manifest;
use crate::registry::http::util::dump_body_to_string;

use anyhow::{bail, Context, Error};
use http::Uri;
use http::{Response, StatusCode};

use hyper::{Body, Client};
use hyper_rustls::ConfigBuilderExt;

use tokio::time::timeout;

use self::util::request_path_in_repository_as_string;

use super::ContentAndContentType;

pub struct HttpRegistry {
    registry_uri: Uri,
    name: String,
    http_client: HttpCli,
}

#[async_trait::async_trait]
impl super::RegistryCore for HttpRegistry {
    async fn fetch_manifest_as_string(&self, digest: &str) -> Result<ContentAndContentType, Error> {
        let uri = self.repository_uri_from_path(format!("/manifests/{}", digest))?;
        Ok(request_path_in_repository_as_string(&self.http_client, &uri).await?)
    }

    async fn fetch_config_as_string(&self, digest: &str) -> Result<ContentAndContentType, Error> {
        let uri = self.repository_uri_from_path(format!("/blobs/{}", digest))?;
        Ok(request_path_in_repository_as_string(&self.http_client, &uri).await?)
    }

    async fn upload_manifest(
        &self,
        manifest: &Manifest,
        tag: &str,
    ) -> Result<Option<String>, Error> {
        let manifest_bytes = Bytes::from(manifest.to_bytes()?);

        if let Ok(content_and_type) = self.fetch_manifest_as_string(tag).await {
            if manifest_bytes == content_and_type.content.as_bytes() {
                return Ok(None);
            }
        }

        let post_target_uri = self.repository_uri_from_path(format!("/manifests/{}", tag))?;

        let response = self
            .http_client
            .request(
                &post_target_uri,
                (),
                |_, builder| async {
                    builder
                        .method(http::Method::PUT)
                        .header("Content-Type", manifest.media_type())
                        .body(Body::from(manifest_bytes.clone()))
                        .map_err(|e| e.into())
                },
                0,
            )
            .await;

        eprintln!("Got response: {:?}", response);

        let mut r: Response<Body> = response?;
        eprintln!("Got response code: {:?}", r.status());

        if r.status() != StatusCode::CREATED {
            bail!("Expected to get status code CREATED, but got {:#?}, hitting url: {:#?},\nUploading {:#?}\nResponse:{}", r.status(), post_target_uri, manifest, dump_body_to_string(&mut r).await? )
        }

        if let Some(location_header) = r.headers().get(http::header::LOCATION) {
            let location_str = location_header.to_str()?;
            Ok(Some(location_str.to_string()))
        } else {
            bail!("We got a positive response code: {:#?}, however we are missing the location header as is required in the spec", r.status())
        }
    }
}

impl HttpRegistry {
    pub(crate) async fn from_maybe_domain_and_name<S: AsRef<str> + Send, S2: AsRef<str> + Send>(
        registry_base: S,
        name: S2,
    ) -> Result<HttpRegistry, Error> {
        let mut uri_parts = registry_base.as_ref().parse::<Uri>()?.into_parts();
        // default to using https
        if uri_parts.scheme.is_none() {
            uri_parts.scheme = Some("https".parse()?);
        }
        uri_parts.path_and_query = Some("/".try_into()?);

        let registry_uri = Uri::from_parts(uri_parts)?;

        let tls = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_native_roots()
            .with_no_client_auth();

        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(tls)
            .https_or_http()
            .enable_http1()
            .build();

        let http_client: Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>> =
            Client::builder().build::<_, hyper::Body>(https);

        eprintln!("Connecting to registry {:?}", registry_uri);
        let reg = HttpRegistry {
            registry_uri: registry_uri.clone(),
            name: name.as_ref().to_string(),
            http_client: HttpCli {
                inner_client: http_client,
                credentials: Default::default(),
                auth_info: Default::default(),
            },
        };

        let req_uri = reg.v2_from_path("/")?;

        let req_future = reg
            .http_client
            .request_simple(&req_uri, http::Method::HEAD, 3);

        let mut resp = match timeout(Duration::from_millis(4000), req_future).await {
            Err(_) => bail!(
                "Timed out connecting to registry {:?}, after waiting 4 seconds.",
                registry_uri
            ),
            Ok(e) => e.with_context(|| {
                format!(
                    "When trying to query base url of registry: {:?} ; query uri: {:?}",
                    registry_uri, req_uri
                )
            })?,
        };

        if resp
            .headers()
            .get("docker-distribution-api-version")
            .is_none()
        {
            let body = dump_body_to_string(&mut resp).await.unwrap_or_default();
            bail!("Failed to request base url of registry. Registry configuration likely broken: {:#?}, status code: {:?}, body:\n{:?}", registry_uri, resp.status(), body);
        }

        eprintln!("Connected to registry {:?}", registry_uri);

        Ok(reg)
    }

    fn v2_from_path<S: AsRef<str>>(&self, path: S) -> Result<Uri, Error> {
        let mut uri_builder = self.registry_uri.clone().into_parts();
        let path_ext = path.as_ref();
        if !path_ext.is_empty() && !path_ext.starts_with('/') {
            bail!("Invalid path reference, should start in a /")
        }
        uri_builder.path_and_query = Some(format!("/v2{}", path.as_ref()).try_into()?);

        let query_uri = Uri::from_parts(uri_builder)?;
        Ok(query_uri)
    }

    fn repository_uri_from_path<S: AsRef<str>>(&self, path: S) -> Result<Uri, Error> {
        let path_ext = path.as_ref();
        if !path_ext.starts_with('/') {
            bail!("Invalid path reference, should start in a /")
        }
        self.v2_from_path(format!("/{}{}", self.name, path_ext))
    }
}
