use std::time::Duration;

use crate::registry::ContentAndContentType;
use anyhow::{bail, Context, Error};
use http::Uri;
use http::{Response, StatusCode};
use hyper::body::HttpBody as _;

use hyper::Body;

use tokio::io::AsyncWriteExt;

use super::HttpCli;

#[async_recursion::async_recursion]
async fn inner_redirect_uri_fetch<F>(
    client: &HttpCli,
    configure_request_builder: F,
    uri: &Uri,
    retries: usize,
) -> Result<Response<Body>, Error>
where
    F: Fn(http::request::Builder) -> http::request::Builder + Send + Sync,
{
    let req_builder = http::request::Builder::default().uri(uri);
    let req_builder = configure_request_builder(req_builder);

    let request = req_builder.body(Body::from(""))?;

    let r: Response<Body> = match client.request(request).await {
        Err(e) => {
            if e.is_connect() && retries < 10 {
                tokio::time::sleep(Duration::from_millis((retries * 2) as u64)).await;
                return inner_redirect_uri_fetch(
                    client,
                    configure_request_builder,
                    uri,
                    retries + 1,
                )
                .await;
            } else {
                return Err(e.into());
            }
        }
        Ok(r) => r,
    };

    let status = r.status();
    if status.is_redirection() {
        if let Some(location_header) = r.headers().get(http::header::LOCATION) {
            let location_str = location_header.to_str()?;
            return inner_redirect_uri_fetch(
                client,
                configure_request_builder,
                &location_str.parse::<Uri>()?,
                retries,
            )
            .await
            .with_context(|| {
                format!(
                    "Failure when attempting to query, url we were redirected to {}",
                    location_str
                )
            });
        }
    }

    Ok(r)
}

pub(super) async fn redirect_uri_fetch<F>(
    client: &HttpCli,
    configure_request_builder: F,
    uri: &Uri,
) -> Result<Response<Body>, Error>
where
    F: Fn(http::request::Builder) -> http::request::Builder + Send + Sync,
{
    inner_redirect_uri_fetch(client, configure_request_builder, uri, 0).await
}

pub(super) async fn dump_body_to_string(response: &mut Response<Body>) -> Result<String, Error> {
    let mut buffer = Vec::default();
    while let Some(chunk) = response.body_mut().data().await {
        buffer.write_all(&chunk?).await?;
    }
    let metadata = std::str::from_utf8(&buffer)?;
    Ok(metadata.to_string())
}

pub(super) async fn request_path_in_repository_as_string(
    client: &HttpCli,
    uri: &Uri,
) -> Result<ContentAndContentType, Error> {
    let mut r = redirect_uri_fetch(
        client,
        |req| {
            req.header(
                "Accept",
                "application/vnd.docker.distribution.manifest.v2+json",
            )
        },
        uri,
    )
    .await?;
    let metadata = dump_body_to_string(&mut r).await?;

    let status = r.status();
    if status != StatusCode::OK {
        bail!(
            "Request to {:#?} failed, code: {:?}; body content:\n{:#?}",
            uri,
            status,
            metadata
        )
    }

    let content_string = metadata;

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
