use std::path::Path;
use std::sync::Arc;

use crate::hash::sha256_value::Sha256Value;
use crate::registry::ops::BYTES_IN_MB;
use crate::registry::BlobStore;
use anyhow::{bail, Context, Error};
use http::StatusCode;
use http::Uri;

use indicatif::ProgressBar;
use sha2::Digest;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;

use super::util::dump_body_to_string;

#[async_trait::async_trait]
impl BlobStore for super::HttpRegistry {
    async fn blob_exists(&self, digest: &str) -> Result<bool, Error> {
        let uri = self.repository_uri_from_path(format!("/blobs/{}", digest))?;

        let mut r = self
            .http_client
            .request_simple(&uri, http::Method::HEAD, 3)
            .await
            .context("testing if blob exists")?;

        if r.status() == StatusCode::NOT_FOUND {
            Ok(false)
        } else if r.status() == StatusCode::OK {
            Ok(true)
        } else {
            bail!(
                "Failed call for blob exists {:#?} -- {:#?}, body: {:#?}",
                uri,
                r.status().as_str(),
                dump_body_to_string(&mut r).await?
            )
        }
    }

    async fn download_blob(
        &self,
        target_file: &Path,
        digest: &str,
        length: u64,
        progress_bar: Option<ProgressBar>,
    ) -> Result<(), Error> {
        let target_file = target_file.to_path_buf();

        let uri = self.repository_uri_from_path(format!("/blobs/{}", digest))?;
        let mut response = self
            .http_client
            .request_simple(&uri, http::Method::GET, 3)
            .await
            .context("Requesting blob real path")?;

        if response.status() != StatusCode::OK {
            bail!(
                "Attempted to download blob at uri {:#?}, but got status code {:#?}, body:{:#?}",
                uri,
                response.status(),
                dump_body_to_string(&mut response).await?
            )
        }

        let mut tokio_output = tokio::fs::File::create(&target_file)
            .await
            .with_context(|| {
                format!(
                    "Failed to open file to write to {:?} for download",
                    target_file
                )
            })?;
        let body = response.body_mut();
        let mut total_bytes = 0;
        let mut hasher = sha2::Sha256::new();

        while let Some(chunk) = body.next().await {
            let data = chunk?;
            total_bytes += data.len();

            if let Some(progress_bar) = &progress_bar {
                progress_bar.set_position(total_bytes as u64 / BYTES_IN_MB);
            }

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

        if digest != sha_str {
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

    async fn upload_blob(
        &self,
        local_path: &Path,
        digest: &str,
        length: u64,
        progress_bar: Option<ProgressBar>,
    ) -> Result<(), Error> {
        let post_target_uri = self.repository_uri_from_path("/blobs/uploads/")?;
        // We expect our POST request to get a location header of where to perform the real upload to.

        let mut r = self
            .http_client
            .request_simple(&post_target_uri, http::Method::POST, 3)
            .await
            .with_context(|| {
                format!(
                    "Trying figure out http location for real upload target, posting to {}",
                    post_target_uri
                )
            })?;

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

        // Sometimes we can receive new URI's that don't contain hosts
        // we need to supply this information from the last URI we used in that case
        let location_uri = if location_uri.host().is_some() {
            location_uri
        } else {
            let mut parts = post_target_uri.into_parts();
            parts.path_and_query = location_uri.path_and_query().cloned();
            Uri::from_parts(parts).with_context(|| {
                format!(
                    "Constructed an invalid uri from parts, new uri: {:?}",
                    location_uri
                )
            })?
        };

        let total_uploaded_bytes = Arc::new(Mutex::new(0));

        struct Context {
            progress_bar: Option<ProgressBar>,
            length: u64,
            local_path: std::path::PathBuf,
        }
        let mut r = self
            .http_client
            .request(
                &location_uri,
                Arc::new(Context {
                    progress_bar,
                    length,
                    local_path: local_path.to_path_buf(),
                }),
                |context, builder| async move {
                    let f = tokio::fs::File::open(context.local_path.clone()).await?;

                    let stream = futures::stream::unfold(
                        (context.progress_bar.clone(), ReaderStream::new(f), 0),
                        |(progress_bar_cp, mut reader_stream, read_bytes)| async move {
                            let nxt_chunk = reader_stream.next().await?;

                            match nxt_chunk {
                                Ok(chunk) => {
                                    let read_bytes: usize = read_bytes + chunk.len();
                                    if let Some(progress_bar) = &progress_bar_cp {
                                        progress_bar.set_position(read_bytes as u64 / BYTES_IN_MB);
                                    }
                                    Some((Ok(chunk), (progress_bar_cp, reader_stream, read_bytes)))
                                }
                                Err(ex) => {
                                    let e: Box<dyn std::error::Error + Send + Sync> = Box::new(ex);
                                    Some((Err(e), (progress_bar_cp, reader_stream, read_bytes)))
                                }
                            }
                        },
                    );

                    let body = hyper::Body::wrap_stream::<
                        _,
                        bytes::Bytes,
                        Box<dyn std::error::Error + Send + Sync>,
                    >(stream);

                    builder
                        .method(http::Method::PUT)
                        .header("Content-Length", context.length)
                        .header("Content-Type", "application/octet-stream")
                        .body(body)
                        .map_err(|e| e.into())
                },
                3,
            )
            .await
            .context("Performing upload bytes operation")?;

        let total_uploaded_bytes: usize = {
            let m = total_uploaded_bytes.lock().await;
            *m
        };
        if r.status() != StatusCode::CREATED && r.status() != StatusCode::OK {
            bail!("Blob Upload: Expected to get status code OK, but got {:#?},\nUploading {:?}\nUploading to: {:#?}\nBody:\n{:#?}\nUploaded: {} bytes\nExpected length: {}", r.status(), local_path, &location_uri, dump_body_to_string(&mut r).await?, total_uploaded_bytes, length)
        }

        if let Some(location_header) = r.headers().get(http::header::LOCATION) {
            tracing::debug!(
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
