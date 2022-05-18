mod http;

use std::{path::Path, sync::Arc};

use anyhow::Error;

#[derive(Debug, Clone)]
pub struct ContentAndContentType {
    pub content_type: Option<String>,
    pub content: String,
}

#[async_trait::async_trait]
pub trait RegistryCore {
    async fn fetch_manifest_as_string(&self, digest: &str) -> Result<ContentAndContentType, Error>;

    async fn fetch_config_as_string(&self, digest: &str) -> Result<ContentAndContentType, Error>;

    async fn upload_manifest(
        &self,
        manifest: &crate::container_specs::oci_types::Manifest,
        manifest_bytes: &Vec<u8>,
        tags: &Vec<String>,
    ) -> Result<(), Error>;
}

#[async_trait::async_trait]
pub trait BlobStore {
    async fn blob_exists(&self, digest: &str) -> Result<bool, Error>;

    async fn download_blob(
        &self,
        target_file: &Path,
        digest: &str,
        length: u64,
    ) -> Result<(), Error>;

    async fn upload_blob(&self, local_path: &Path, digest: &str, length: u64) -> Result<(), Error>;
}

pub trait Registry: RegistryCore + BlobStore {}

impl<T> Registry for T where T: RegistryCore + BlobStore {}

pub async fn from_maybe_domain_and_name<S: AsRef<str> + Send, S2: AsRef<str> + Send>(
    registry_base: S,
    name: S2,
) -> Result<Arc<dyn Registry>, Error> {
    let inner_reg = http::HttpRegistry::from_maybe_domain_and_name(registry_base, name).await?;
    Ok(Arc::new(inner_reg))
}
