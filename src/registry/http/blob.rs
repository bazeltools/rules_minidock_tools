use std::path::Path;
use std::sync::Arc;

use crate::hash::sha256_value::Sha256Value;
use crate::registry::BlobStore;
use anyhow::{bail, Context, Error};
use http::Uri;
use http::{Response, StatusCode};

use hyper::Body;

use indicatif::ProgressBar;
use sha2::Digest;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;

use super::util::{dump_body_to_string, redirect_uri_fetch};

#[async_trait::async_trait]
impl BlobStore for super::HttpRegistry {
    async fn blob_exists(&self, digest: &str) -> Result<bool, Error> {
        let uri = self.repository_uri_from_path(format!("/blobs/{}", digest))?;

        let r = redirect_uri_fetch(
            &self.http_client,
            |req| req.method(http::Method::HEAD),
            &uri,
        )
        .await?;

        if r.status() == StatusCode::NOT_FOUND {
            Ok(false)
        } else if r.status() == StatusCode::OK {
            Ok(true)
        } else {
            bail!("Failed to request {:#?} -- {:#?}", uri, r.status().as_str())
        }
    }

    async fn download_blob(
        &self,
        target_file: &Path,
        digest: &str,
        length: u64,
    ) -> Result<(), Error> {
        let target_file = target_file.to_path_buf();

        let uri = self.repository_uri_from_path(format!("/blobs/{}", digest))?;
        let mut response =
            redirect_uri_fetch(&self.http_client, |req| req.method(http::Method::GET), &uri)
                .await?;

        if response.status() != StatusCode::OK {
            bail!(
                "Attempted to download blob at uri {:#?}, but got status code {:#?}, body:{:#?}",
                uri,
                response.status(),
                dump_body_to_string(&mut response).await?
            )
        }

        let mut tokio_output = tokio::fs::File::create(&target_file).await.unwrap();
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

        if digest != &sha_str {
            bail!(
                "Download produced the incorrect sha. Expected {} / {} bytes -- Got {} / {} bytes",
                digest,
                length,
                sha_str,
                total_bytes
            )
        }
        Ok(())
    }

    async fn upload_blob(&self, local_path: &Path, digest: &str, length: u64) -> Result<(), Error> {
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
            format!("{}{}digest={}", location_str, chr, digest).parse::<Uri>().with_context(|| format!("Unable to parse location header response when doing post for new upload, location header was {:?}", location_str))?
        } else {
            let body = dump_body_to_string(&mut r).await?;
            bail!("Was a redirection response code, but missing Location header, invalid response from server, body:\n{:#?}", body);
        };

        let f = tokio::fs::File::open(local_path).await?;

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
            bail!("Expected to get status code OK, but got {:#?},\nUploading {:?}\nUploading to: {:#?}\nBody:\n{:#?}\nUploaded: {} bytes\nExpected length: {}", r.status(), local_path, &location_uri, dump_body_to_string(&mut r).await?, total_uploaded_bytes, length)
        }

        if let Some(location_header) = r.headers().get(http::header::LOCATION) {
            eprintln!(
                "Blob upload complete for digest {}, stored at: {:#?}",
                digest,
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
