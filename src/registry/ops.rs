use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{bail, Error};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::container_specs::blob_reference::BlobReference;

use super::Registry;
use console::style;

pub const BYTES_IN_MB: u64 = 1024 * 1024;
pub const BYTES_IN_GB: u64 = BYTES_IN_MB * 1024;

pub fn size_to_string(size: u64) -> String {
    let gb = size / BYTES_IN_GB;
    let mb = size / BYTES_IN_MB;
    if gb > 0 {
        let gb_flt = (gb as f64) + ((mb % 1024) as f64) / 1024_f64;
        format!("{} GB", gb_flt)
    } else {
        format!("{} MB", mb)
    }
}

#[derive(Default)]
pub struct ActionsTaken {
    already_present: usize,
    already_present_size: u64,

    copied_from_source_repository: usize,
    copied_from_source_repository_size: u64,

    uploaded_from_local: usize,
    uploaded_from_local_size: u64,

    downloaded_from_source_repository: usize,
    downloaded_from_source_repository_size: u64,

    uploaded_data_from_source_repository: usize,
    uploaded_data_from_source_repository_size: u64,
}
impl std::fmt::Display for ActionsTaken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lines = vec![
            format!(
                "Already present on remote:                 {} entries, {}",
                self.already_present,
                size_to_string(self.already_present_size)
            ),
            format!(
                "Copied remotely from source repository:    {} entries, {}",
                self.copied_from_source_repository,
                size_to_string(self.copied_from_source_repository_size)
            ),
            format!(
                "Uploaded from local output state:          {} entries, {}",
                self.uploaded_from_local,
                size_to_string(self.uploaded_from_local_size)
            ),
            format!(
                "Downloaded from source repository:         {} entries, {}",
                self.downloaded_from_source_repository,
                size_to_string(self.downloaded_from_source_repository_size)
            ),
            format!(
                "Uploaded data originally from source repo: {} entries, {}",
                self.uploaded_data_from_source_repository,
                size_to_string(self.uploaded_data_from_source_repository_size)
            ),
        ];
        write!(f, "{}", lines.join("\n"))
    }
}

impl ActionsTaken {
    pub fn merge(&mut self, other: &ActionsTaken) {
        self.already_present += other.already_present;
        self.already_present_size += other.already_present_size;
        self.copied_from_source_repository += other.copied_from_source_repository;
        self.copied_from_source_repository_size += other.copied_from_source_repository_size;

        self.uploaded_from_local += other.uploaded_from_local;
        self.uploaded_from_local_size += other.uploaded_from_local_size;

        self.downloaded_from_source_repository += other.downloaded_from_source_repository;
        self.downloaded_from_source_repository_size += other.downloaded_from_source_repository_size;

        self.uploaded_data_from_source_repository += other.uploaded_data_from_source_repository;
        self.uploaded_data_from_source_repository_size +=
            other.uploaded_data_from_source_repository_size;
    }

    pub fn already_present(blob: &BlobReference) -> ActionsTaken {
        ActionsTaken {
            already_present: 1,
            already_present_size: blob.size,
            ..Default::default()
        }
    }

    pub fn copied_from_source_repository(blob: &BlobReference) -> ActionsTaken {
        ActionsTaken {
            copied_from_source_repository: 1,
            copied_from_source_repository_size: blob.size,
            ..Default::default()
        }
    }

    pub fn uploaded_from_local(blob: &BlobReference) -> ActionsTaken {
        ActionsTaken {
            uploaded_from_local: 1,
            uploaded_from_local_size: blob.size,
            ..Default::default()
        }
    }

    pub fn uploaded_data_from_source_repository(
        blob: &BlobReference,
        downloaded: bool,
    ) -> ActionsTaken {
        let (downloaded_from_source_repository, downloaded_from_source_repository_size) =
            if downloaded { (1, blob.size) } else { (0, 0) };
        ActionsTaken {
            uploaded_data_from_source_repository: 1,
            uploaded_data_from_source_repository_size: blob.size,
            downloaded_from_source_repository,
            downloaded_from_source_repository_size,
            ..Default::default()
        }
    }
}

pub struct RequestState {
    pub local_digests: HashMap<String, PathBuf>,
    pub destination_registry: Arc<dyn Registry>,
    pub source_registry: Option<Arc<dyn Registry>>,
    pub cache_path: PathBuf,
}

impl RequestState {
    pub(super) async fn destination_present(&self, blob: &BlobReference) -> Result<bool, Error> {
        self.destination_registry.blob_exists(&blob.digest).await
    }

    pub(super) async fn with_source_present(
        &self,
        blob: &BlobReference,
    ) -> Result<Option<&Arc<dyn Registry>>, Error> {
        if let Some(source_registry) = &self.source_registry {
            if source_registry.blob_exists(&blob.digest).await? {
                Ok(Some(source_registry))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }
}

pub async fn ensure_present(
    blob: &BlobReference,
    request_state: Arc<RequestState>,
    mp: Arc<MultiProgress>,
) -> Result<ActionsTaken, Error> {
    let prefix_str = if let Some(local_layer_path) = request_state.local_digests.get(&blob.digest) {
        let p = local_layer_path.to_string_lossy();
        if p.len() >= 80 {
            p.split_at(p.len() - 80).1.to_string()
        } else {
            p.to_string()
        }
    } else {
        blob.digest.clone()
    };
    let message_style = ProgressStyle::with_template("{prefix:80} {msg}").unwrap();
    let io_style =
        ProgressStyle::with_template("{prefix:80} {msg:25} {pos}/{len:4}MB {bar:60.green/yellow}")
            .unwrap();

    let message_pb = ProgressBar::new(1);
    message_pb.set_style(message_style.clone());
    let pb = mp.add(message_pb);
    pb.set_prefix(prefix_str);

    pb.set_message("Checking destination presence");

    let destination_registry_name = request_state.destination_registry.registry_name();
    if request_state.destination_present(blob).await? {
        pb.finish_with_message(format!("{}", style("✔").green()));
        return Ok(ActionsTaken::already_present(blob));
    }

    pb.set_message("Checking source repository presence");
    if let Some(source_registry) = request_state.with_source_present(blob).await? {
        let source_registry_name = source_registry.registry_name();
        pb.set_message("Try copy from source repository");
        if let Err(e) = request_state
            .destination_registry
            .try_copy_from(&source_registry_name, &blob.digest)
            .await
        {
            tracing::debug!(
                "Failed to copy a missing digest between remote repos, will continue: digest: {:#?}, from: {}, to: {}; error: {:#?}",
                &blob.digest, &source_registry_name, &destination_registry_name, e
            );
        }
        pb.set_message("Checking destination presence post copy");
        if request_state.destination_present(blob).await? {
            pb.finish_with_message(format!("{}", style("✔").green()));
            return Ok(ActionsTaken::copied_from_source_repository(blob));
        }
        pb.set_message("Not found, copy failed.");
    }

    if let Some(local_layer_path) = request_state.local_digests.get(&blob.digest) {
        tracing::debug!("Found {} locally, uploading..", blob.digest);
        pb.set_message("Uploading");
        pb.set_style(io_style.clone());
        pb.set_length(blob.size / BYTES_IN_MB);
        pb.set_position(0);
        request_state
            .destination_registry
            .upload_blob(local_layer_path, &blob.digest, blob.size, Some(pb.clone()))
            .await?;
        pb.finish_with_message(format!("{}", style("✔").green()));
        return Ok(ActionsTaken::uploaded_from_local(blob));
    }

    pb.set_message("Checking source repository presence");
    pb.set_style(message_style.clone());

    if let Some(source_registry) = request_state.with_source_present(blob).await? {
        let tmp_cache_path = request_state.cache_path.join("tmp");
        let expected_path = request_state
            .cache_path
            .join(blob.digest.strip_prefix("sha256:").unwrap_or(&blob.digest));

        let mut downloaded = false;
        if !expected_path.exists() {
            let local_storage = tempfile::NamedTempFile::new_in(&tmp_cache_path)?;
            tracing::debug!(
                "Downloading from remote registry: {}, size: {}",
                &blob.digest,
                size_to_string(blob.size)
            );

            pb.set_message("Downloading from upstream");
            pb.set_style(io_style.clone());
            pb.set_length(blob.size / BYTES_IN_MB);
            pb.set_position(0);
            source_registry
                .download_blob(
                    local_storage.path(),
                    &blob.digest,
                    blob.size,
                    Some(pb.clone()),
                )
                .await?;
            std::fs::rename(
                local_storage.path(),
                request_state
                    .cache_path
                    .join(blob.digest.strip_prefix("sha256:").unwrap_or(&blob.digest)),
            )?;
            downloaded = true;
        }
        pb.set_message("Uploading cached data");
        pb.set_style(io_style.clone());
        pb.set_length(blob.size / BYTES_IN_MB);
        pb.set_position(0);
        request_state
            .destination_registry
            .upload_blob(&expected_path, &blob.digest, blob.size, Some(pb.clone()))
            .await?;
        pb.finish_with_message(format!("{}", style("✔").green()));
        Ok(ActionsTaken::uploaded_data_from_source_repository(
            blob, downloaded,
        ))
    } else {
        pb.finish_with_message(format!("{} Exhausted digest sources", style("x").red()));
        bail!("We still have remaining missing digests that we dont have locally. However we haven't been configured with a source repository, so we have no means to fetch them.")
    }
}
