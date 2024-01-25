use crate::registry::{CopyOperations, RegistryName};
use anyhow::{bail, Error};

use http::StatusCode;

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
            "/blobs/uploads/?mount={}&from={}",
            digest, source_registry_name
        ))?;

        let r = self
            .http_client
            .request(
                &uri,
                (),
                |_, c| async {
                    c.method(http::Method::POST)
                        .body(hyper::Body::from(""))
                        .map_err(|e| e.into())
                },
                3,
            )
            .await?;

        if r.status() == StatusCode::CREATED {
            Ok(())
        } else {
            bail!("Failed to request {:#?} -- {:#?}", uri, r.status().as_str())
        }
    }
}
