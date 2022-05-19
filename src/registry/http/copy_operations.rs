


use std::path::Path;
use std::sync::Arc;

use crate::hash::sha256_value::Sha256Value;
use crate::registry::{BlobStore, CopyOperations, RegistryName};
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
impl CopyOperations for super::HttpRegistry {
    fn registry_name(&self) -> RegistryName {
        RegistryName(self.name.clone())
    }

    async fn try_copy_from(&self, source_registry: &RegistryName, digest: &str) -> Result<bool, Error> {
        let uri = self.repository_uri_from_path(format!("/uploads/?mount={}from={}", digest, source_registry))?;

        let r = redirect_uri_fetch(
            &self.http_client,
            |req| req.method(http::Method::POST),
            &uri,
        )
        .await?;

        if r.status() == StatusCode::NOT_FOUND {
            Ok(false)
        } else if r.status() == StatusCode::CREATED {
            Ok(true)
        } else {
            bail!("Failed to request {:#?} -- {:#?}", uri, r.status().as_str())
        }
    }

}
