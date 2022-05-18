use crate::registry::ContentAndContentType;
use anyhow::{bail, Error};
use http::Uri;
use http::{Response, StatusCode};
use hyper::body::HttpBody as _;

use hyper::Body;

use sha2::Digest;
use tokio::io::AsyncWriteExt;

use super::HttpCli;

#[async_recursion::async_recursion]
pub(super) async fn redirect_uri_fetch<F>(
    client: &HttpCli,
    configure_request_builder: F,
    uri: &Uri,
) -> Result<Response<Body>, Error>
where
    F: Fn(http::request::Builder) -> http::request::Builder + Send + Sync,
{
    let req_builder = http::request::Builder::default().uri(uri);
    let req_builder = configure_request_builder(req_builder);

    let request = req_builder.body(Body::from(""))?;

    let r: Response<Body> = client.request(request).await?;

    let status = r.status();
    if status.is_redirection() {
        if let Some(location_header) = r.headers().get(http::header::LOCATION) {
            let location_str = location_header.to_str()?;
            return redirect_uri_fetch(
                client,
                configure_request_builder,
                &location_str.parse::<Uri>()?,
            )
            .await;
        }
    }

    Ok(r)
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