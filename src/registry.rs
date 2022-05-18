use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::container_specs::oci_types::Manifest;
use crate::hash::sha256_value::Sha256Value;
use anyhow::{bail, Context, Error};
use http::Uri;
use http::{Response, StatusCode};
use hyper::body::HttpBody as _;
use hyper::{Body, Client};
use hyper_rustls::ConfigBuilderExt;
use indicatif::ProgressBar;
use sha2::Digest;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;

#[derive(Debug, Clone)]
pub struct ContentAndContentType {
    pub content_type: Option<String>,
    pub content: String,
}

pub struct Registry {
    registry_uri: Uri,
    name: String,
    http_client: Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
}

impl Registry {
    pub async fn from_maybe_domain_and_name(
        registry_base: &String,
        name: &String,
    ) -> Result<Registry, Error> {
        let mut uri_parts = registry_base.parse::<Uri>()?.into_parts();
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
        let reg = Registry {
            registry_uri: registry_uri.clone(),
            name: name.to_string(),
            http_client,
        };

        let req_uri = reg.v2_from_path("/")?;
        let req_future = reg.redirect_uri_fetch(|req| req.method(http::Method::HEAD), &req_uri);

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
            let body = reg.dump_body_to_string(&mut resp).await.unwrap_or_default();
            bail!("Failed to request base url of registry. Registry configuration likely broken: {:#?}, status code: {:?}, body:\n{:?}", registry_uri, resp.status(), body);
        }

        Ok(reg)
    }

    #[async_recursion::async_recursion]
    async fn redirect_uri_fetch<F>(
        &self,
        configure_request_builder: F,
        uri: &Uri,
    ) -> Result<Response<Body>, Error>
    where
        F: Fn(http::request::Builder) -> http::request::Builder + Send + Sync,
    {
        let req_builder = http::request::Builder::default().uri(uri);
        let req_builder = configure_request_builder(req_builder);

        let request = req_builder.body(Body::from(""))?;

        let r: Response<Body> = self.http_client.request(request).await?;

        let status = r.status();
        if status.is_redirection() {
            if let Some(location_header) = r.headers().get(http::header::LOCATION) {
                let location_str = location_header.to_str()?;
                return self
                    .redirect_uri_fetch(configure_request_builder, &location_str.parse::<Uri>()?)
                    .await;
            }
        }

        Ok(r)
    }

    async fn dump_body_to_string(&self, response: &mut Response<Body>) -> Result<String, Error> {
        let mut buffer = Vec::default();
        while let Some(chunk) = response.body_mut().data().await {
            buffer.write_all(&chunk?).await?;
        }
        let metadata = std::str::from_utf8(&buffer)?;
        Ok(metadata.to_string())
    }

    async fn request_path_in_repository_as_string(
        &self,
        uri: &Uri,
    ) -> Result<ContentAndContentType, Error> {
        let mut r = self
            .redirect_uri_fetch(
                |req| {
                    req.header(
                        "Accept",
                        "application/vnd.docker.distribution.manifest.v2+json",
                    )
                },
                uri,
            )
            .await?;
        let metadata = self.dump_body_to_string(&mut r).await?;

        let status = r.status();
        if status != StatusCode::OK {
            bail!(
                "Request to {:#?} failed, code: {:?}; body content:\n{:#?}",
                uri,
                status,
                metadata
            )
        }

        let content_string = metadata.to_string();

        match r.headers().get("content-type") {
            Some(c) => Ok(ContentAndContentType {
                content_type: Some(c.to_str()?.to_string()),
                content: content_string,
            }),
            None => Ok(ContentAndContentType {
                content_type: None,
                content: content_string,
            }),
        }
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

    pub async fn fetch_manifest_as_string<S: AsRef<str>>(
        &self,
        digest: S,
    ) -> Result<ContentAndContentType, Error> {
        let uri = self.repository_uri_from_path(format!("/manifests/{}", digest.as_ref()))?;
        Ok(self.request_path_in_repository_as_string(&uri).await?)
    }

    pub async fn fetch_config_as_string<S: AsRef<str>>(
        &self,
        digest: S,
    ) -> Result<ContentAndContentType, Error> {
        let uri = self.repository_uri_from_path(format!("/blobs/{}", digest.as_ref()))?;
        Ok(self.request_path_in_repository_as_string(&uri).await?)
    }

    pub async fn blob_exists<S: AsRef<str>>(&self, digest: S) -> Result<bool, Error> {
        let uri = self.repository_uri_from_path(format!("/blobs/{}", digest.as_ref()))?;

        let r = self
            .redirect_uri_fetch(|req| req.method(http::Method::HEAD), &uri)
            .await?;

        if r.status() == StatusCode::NOT_FOUND {
            Ok(false)
        } else if r.status() == StatusCode::OK {
            Ok(true)
        } else {
            bail!("Failed to request {:#?} -- {:#?}", uri, r.status().as_str())
        }
    }

    pub async fn download_blob<P: AsRef<Path>, S: AsRef<str>>(
        &self,
        target_file: P,
        digest: S,
        length: u64,
    ) -> Result<(), Error> {
        let uri = self.repository_uri_from_path(format!("/blobs/{}", digest.as_ref()))?;
        let mut response = self
            .redirect_uri_fetch(|req| req.method(http::Method::GET), &uri)
            .await?;

        if response.status() != StatusCode::OK {
            bail!(
                "Attempted to download blob at uri {:#?}, but got status code {:#?}, body:{:#?}",
                uri,
                response.status(),
                self.dump_body_to_string(&mut response).await?
            )
        }

        let mut tokio_output = tokio::fs::File::create(target_file.as_ref()).await.unwrap();
        let body = response.body_mut();
        let mut total_bytes = 0;
        let mut hasher = sha2::Sha256::new();
        while let Some(chunk) = body.next().await {
            let data = chunk?;

            total_bytes += data.len();
            if !data.is_empty() {
                hasher.update(&data[..]);
            }
            tokio_output.write_all(&data[..]).await?;
        }
        tokio_output.flush().await?;
        drop(tokio_output);

        let sha256_value = Sha256Value::new_from_slice(&hasher.finalize())
            .with_context(|| "produced an invalid sha byte slice, shouldn't really happen")?;

        let sha_str = format!("sha256:{}", sha256_value);

        if digest.as_ref() != &sha_str {
            bail!(
                "Download produced the incorrect sha. Expected {} / {} bytes -- Got {} / {} bytes",
                digest.as_ref(),
                length,
                sha_str,
                total_bytes
            )
        }
        Ok(())
    }

    pub async fn upload_manifest(
        &self,
        manifest: &Manifest,
        manifest_bytes: &Vec<u8>,
        tags: &Vec<String>,
    ) -> Result<(), Error> {
        for t in tags.iter() {
            let post_target_uri = self.repository_uri_from_path(format!("/manifest/{}", t))?;
            let req_builder = http::request::Builder::default()
                .method(http::Method::POST)
                .uri(post_target_uri.clone())
                .header("Content-Type", &manifest.media_type);
            let request = req_builder.body(Body::from(manifest_bytes.clone()))?;
            let mut r: Response<Body> = self.http_client.request(request).await?;

            if r.status() != StatusCode::CREATED {
                bail!("Expected to get status code CREATED, but got {:#?},\nUploading {:#?}\nResponse:{}", r.status(), manifest, self.dump_body_to_string(&mut r).await? )
            }

            if let Some(location_header) = r.headers().get(http::header::LOCATION) {
                let location_str = location_header.to_str()?;
                eprintln!("Uploaded manifest to {}, for tag: {} @ {}", self.name, t, location_str);
            } else {
                bail!("We got a positive responsecode: {:#?}, however we are missing the location header as is required in the spec", r.status())
            }
        }
        Ok(())
    }

    pub async fn upload_blob<P: AsRef<Path>, S: AsRef<str>>(
        &self,
        local_path: P,
        digest: S,
        length: u64,
    ) -> Result<(), Error> {
        let post_target_uri = self.repository_uri_from_path("/blobs/uploads/")?;
        // We expect our POST request to get a location header of where to perform the real upload to.
        let req_builder = http::request::Builder::default()
            .method(http::Method::POST)
            .uri(post_target_uri.clone());
        let request = req_builder.body(Body::from(""))?;
        let mut r: Response<Body> = self.http_client.request(request).await?;
        if r.status() != StatusCode::ACCEPTED {
            bail!(
                "Expected to get a ACCEPTED/202 for upload post request to {:?}, but got {:?}",
                post_target_uri,
                r.status()
            )
        }

        let location_uri = if let Some(location_header) = r.headers().get(http::header::LOCATION) {
            let location_str = location_header.to_str()?;
            let chr = if location_str.contains('?') { '&' } else { '?' };
            format!("{}{}digest={}", location_str, chr, digest.as_ref()).parse::<Uri>().with_context(|| format!("Unable to parse location header response when doing post for new upload, location header was {:?}", location_str))?
        } else {
            let body = self.dump_body_to_string(&mut r).await?;
            bail!("Was a redirection response code, but missing Location header, invalid response from server, body:\n{:#?}", body);
        };

        let f = tokio::fs::File::open(local_path.as_ref()).await?;

        let mut file_reader_stream = ReaderStream::new(f);

        let total_uploaded_bytes = Arc::new(Mutex::new(0));
        let stream_byte_ref = Arc::clone(&total_uploaded_bytes);

        let bar = Arc::new(Mutex::new(ProgressBar::new(length)));
        let bar_for_stream = Arc::clone(&bar);

        let stream: async_stream::AsyncStream<
            Result<bytes::Bytes, Box<dyn std::error::Error + Send + Sync>>,
            _,
        > = async_stream::try_stream! {
            while let Some(chunk) = file_reader_stream.next().await {
                let chunk = chunk?;
                let mut cntr = stream_byte_ref.lock().await;
                *cntr += chunk.len();
                let progress_bar = bar_for_stream.lock().await;
                progress_bar.inc(chunk.len() as u64);
                yield chunk
            }
        };

        let body = hyper::Body::wrap_stream(stream);

        let req_builder = http::request::Builder::default()
            .method(http::Method::PUT)
            .uri(location_uri.clone())
            .header("Content-Length", length.to_string())
            .header("Content-Type", "application/octet-stream");

        let request = req_builder.body(body)?;

        let mut r: Response<Body> = self.http_client.request(request).await?;

        bar.lock().await.finish();
        let total_uploaded_bytes: usize = {
            let m = total_uploaded_bytes.lock().await;
            *m
        };
        if r.status() != StatusCode::CREATED && r.status() != StatusCode::OK {
            bail!("Expected to get status code OK, but got {:#?},\nUploading {:?}\nUploading to: {:#?}\nBody:\n{:#?}\nUploaded: {} bytes\nExpected length: {}", r.status(), local_path.as_ref(), &location_uri, self.dump_body_to_string(&mut r).await?, total_uploaded_bytes, length)
        }

        if let Some(location_header) = r.headers().get(http::header::LOCATION) {
            eprintln!(
                "Blob upload complete for digest {}, stored at: {:#?}",
                digest.as_ref(),
                location_header.to_str()?
            );
        } else {
            bail!(
                "Invalid server response, expected to get a location header for successful upload"
            );
        }

        Ok(())
    }
}
