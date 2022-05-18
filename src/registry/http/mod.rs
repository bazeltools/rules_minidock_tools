mod blob;
mod util;

use std::time::Duration;

use crate::container_specs::oci_types::Manifest;
use crate::registry::http::util::{dump_body_to_string, redirect_uri_fetch};

use anyhow::{bail, Error};
use http::Uri;
use http::{Response, StatusCode};

use hyper::{Body, Client};
use hyper_rustls::ConfigBuilderExt;

use sha2::Digest;

use tokio::time::timeout;

use self::util::request_path_in_repository_as_string;

use super::ContentAndContentType;

type HttpCli = Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>;

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
        manifest_bytes: &Vec<u8>,
        tags: &Vec<String>,
    ) -> Result<(), Error> {
        for t in tags.iter() {
            let post_target_uri = self.repository_uri_from_path(format!("/manifests/{}", t))?;
            let req_builder = http::request::Builder::default()
                .method(http::Method::PUT)
                .uri(post_target_uri.clone())
                .header("Content-Type", &manifest.media_type);
            let request = req_builder.body(Body::from(manifest_bytes.clone()))?;
            let mut r: Response<Body> = self.http_client.request(request).await?;

            if r.status() != StatusCode::CREATED {
                bail!("Expected to get status code CREATED, but got {:#?}, hitting url: {:#?},\nUploading {:#?}\nResponse:{}", r.status(), post_target_uri, manifest, dump_body_to_string(&mut r).await? )
            }

            if let Some(location_header) = r.headers().get(http::header::LOCATION) {
                let location_str = location_header.to_str()?;
                eprintln!(
                    "Uploaded manifest to repository {} @ {:#?}, for tag: {} @ {}",
                    self.name, post_target_uri, t, location_str
                );
            } else {
                bail!("We got a positive response code: {:#?}, however we are missing the location header as is required in the spec", r.status())
            }
        }
        Ok(())
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
            http_client,
        };

        let req_uri = reg.v2_from_path("/")?;
        let req_future = redirect_uri_fetch(
            &reg.http_client,
            |req| req.method(http::Method::HEAD),
            &req_uri,
        );

        let mut resp = match timeout(Duration::from_millis(4000), req_future).await {
            Err(_) => bail!(
                "Timed out connecting to registry {:?}, after waiting 4 seconds.",
                registry_uri
            ),
            Ok(e) => e?,
        };

        if resp
            .headers()
            .get("docker-distribution-api-version")
            .is_none()
        {
            let body = dump_body_to_string(&mut resp).await.unwrap_or_default();
            bail!("Failed to request base url of registry. Registry configuration likely broken: {:#?}, status code: {:?}, body:\n{:?}", registry_uri, resp.status(), body);
        }

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
