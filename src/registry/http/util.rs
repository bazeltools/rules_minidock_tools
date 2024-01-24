use crate::registry::ContentAndContentType;
use anyhow::{bail, Error};
use http::Uri;
use http::{Response, StatusCode};
use hyper::body::HttpBody as _;

use hyper::Body;

use tokio::io::AsyncWriteExt;

use super::HttpCli;

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
    let mut r = client
        .request(
            &uri,
            (),
            |_, c| async {
                c.method(http::Method::GET)
                    .header(
                        "Accept",
                        "application/vnd.docker.distribution.manifest.v2+json",
                    )
                    .body(Body::from(""))
                    .map_err(|e| e.into())
            },
            3,
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
