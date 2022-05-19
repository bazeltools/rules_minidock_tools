use crate::registry::{CopyOperations, RegistryName};
use anyhow::{bail, Error};

use http::StatusCode;

use sha2::Digest;

use super::util::redirect_uri_fetch;

#[async_trait::async_trait]
impl CopyOperations for super::HttpRegistry {
    fn registry_name(&self) -> RegistryName {
        RegistryName(self.name.clone())
    }

    async fn try_copy_from(
        &self,
        source_registry_name: &RegistryName,
        digest: &str,
    ) -> Result<(), Error> {
        let uri = self.repository_uri_from_path(format!(
            "/blobs/uploads/?mount={}from={}",
            digest, source_registry_name
        ))?;

        let r = redirect_uri_fetch(
            &self.http_client,
            |req| req.method(http::Method::POST),
            &uri,
        )
        .await?;

        if r.status() == StatusCode::CREATED {
            Ok(())
        } else {
            bail!("Failed to request {:#?} -- {:#?}", uri, r.status().as_str())
        }
    }
}
