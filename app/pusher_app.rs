use anyhow::bail;
use anyhow::Context;

use clap::Parser;

use rules_minidock_tools::container_specs::ConfigDelta;
use rules_minidock_tools::container_specs::Manifest;
use rules_minidock_tools::container_specs::SpecificationType;
use rules_minidock_tools::hash::sha256_value::Sha256Value;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(name = "pusher app")]
struct Opt {
    #[clap(long, parse(from_os_str))]
    pusher_config: PathBuf,

    #[clap(long, parse(from_os_str))]
    cache_path: PathBuf,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
pub struct PusherConfig {
    pub merger_data: String,
    pub registry: String,
    registry_type: String,
    pub repository: String,
    pub container_tags: Option<Vec<String>>,
    pub container_tag_file: Option<String>,
}

impl PusherConfig {
    pub fn registry_type(&self) -> Result<SpecificationType, anyhow::Error> {
        match self.registry_type.to_lowercase().as_str() {
            "oci" => Ok(SpecificationType::Oci),
            "docker" => Ok(SpecificationType::Docker),
            other => bail!("Unknown registry type {}", other),
        }
    }
}

const BYTES_IN_MB: u64 = 1024 * 1024;
const BYTES_IN_GB: u64 = BYTES_IN_MB * 1024;

fn size_to_string(size: u64) -> String {
    let gb = size / BYTES_IN_GB;
    let mb = size / BYTES_IN_MB;
    if gb > 0 {
        let gb_flt = (gb as f64) + ((mb % 1024) as f64) / 1024_f64;
        format!("{} GB", gb_flt)
    } else {
        format!("{} MB", mb)
    }
}

fn load_tags(pusher_config: &PusherConfig) -> Result<Vec<String>, anyhow::Error> {
    let mut res = Vec::default();
    if let Some(tags) = &pusher_config.container_tags {
        for t in tags.iter() {
            res.push(t.clone());
        }
    }
    if let Some(f) = &pusher_config.container_tag_file {
        for t in std::fs::read_to_string(f)?
            .split_ascii_whitespace()
            .flat_map(|e| e.split(','))
            .filter(|e| !e.is_empty())
        {
            res.push(t.to_string());
        }
    }
    res.sort();
    res.dedup();
    Ok(res)
}
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();

    if !opt.pusher_config.exists() {
        bail!(
            "Path for config passed in does not exist: {:#?}",
            opt.pusher_config
        );
    }

    let pusher_config_content = std::fs::read_to_string(&opt.pusher_config)?;
    let pusher_config: PusherConfig = serde_json::from_str(pusher_config_content.as_str())
        .with_context(|| {
            format!(
                "Attempting to pusher config from file: {},content:\n{}",
                &opt.pusher_config.to_string_lossy(),
                pusher_config_content
            )
        })?;

    let merger_data_path = PathBuf::from(&pusher_config.merger_data);

    let config_path = merger_data_path.join("config.json");
    let _config = ConfigDelta::parse_file(&config_path)?;
    let manifest_path = merger_data_path.join("manifest.json");
    let manifest_bytes = std::fs::read(&manifest_path)?;
    let manifest = Manifest::parse(&manifest_bytes)?;

    let tags = load_tags(&pusher_config)?;
    if tags.is_empty() {
        bail!("No tags specified, unable to know where to push a manifest. Try 'latest' ? ")
    }
    let upload_metadata_path = merger_data_path.join("upload_metadata.json");
    let upload_metadata = rules_minidock_tools::UploadMetadata::parse_file(&upload_metadata_path)?;

    let destination_registry = rules_minidock_tools::registry::from_maybe_domain_and_name(
        &pusher_config.registry,
        &pusher_config.repository,
    )
    .await?;

    let mut same_registry = false;
    let source_registry = if let Some(source_remote_metadata) =
        upload_metadata.remote_metadata.as_ref()
    {
        match (
            source_remote_metadata.registry.as_ref(),
            source_remote_metadata.repository.as_ref(),
        ) {
            (None, None) => None,
            (Some(_), None) => {
                eprintln!("Warning, source image has a specified registry but no repository. Presuming neither are present");
                None
            }
            (None, Some(_)) => {
                eprintln!("Warning, source image has a specified repository but no registry. Presuming neither are present");
                None
            }
            (Some(registry), Some(repository)) => {
                if registry == &pusher_config.registry {
                    same_registry = true;
                    if repository == &pusher_config.repository {
                        Some(destination_registry.clone())
                    } else {
                        Some(
                            rules_minidock_tools::registry::from_maybe_domain_and_name(
                                &registry,
                                &repository,
                            )
                            .await?,
                        )
                    }
                } else {
                    Some(
                        rules_minidock_tools::registry::from_maybe_domain_and_name(
                            &registry,
                            &repository,
                        )
                        .await?,
                    )
                }
            }
        }
    } else {
        None
    };

    let mut missing_size = 0;
    let mut missing_digests = Vec::default();
    for layer in manifest.layers.iter() {
        let size = layer.size;
        let digest = &layer.digest;
        let exists = destination_registry.blob_exists(digest).await?;
        if !exists {
            missing_size += size;
            missing_digests.push(layer.clone());
        }
    }

    if missing_size > 0 {
        println!(
            "Missing {} layers, total size: {}",
            missing_digests.len(),
            size_to_string(missing_size)
        );
        if same_registry {
            // for layer in missing_digests.iter() {
            // }
            todo!()
        }

        let mut v = Vec::default();
        for missing in missing_digests.drain(..) {
            if let Some(local_data) = upload_metadata
                .layer_configs
                .iter()
                .find(|e| e.outer_sha256 == missing.digest)
            {
                let local_layer_path: PathBuf = (&local_data.layer_data).into();
                eprintln!("Found {} locally, uploading..", local_data.outer_sha256);
                destination_registry
                    .upload_blob(
                        &local_layer_path,
                        &local_data.outer_sha256,
                        local_data.compressed_length,
                    )
                    .await?
            } else {
                v.push(missing);
            }
        }

        std::mem::swap(&mut missing_digests, &mut v);

        if !missing_digests.is_empty() {
            if missing_digests.len() as u64 != missing_size {
                println!("We uploaded the locally found layers - {} , but we still have remaining {} layers", missing_size - missing_digests.len() as u64, missing_digests.len());
            }

            match source_registry {
        Some(source_registry) => {
            let cache_path = opt.cache_path.join("tmp");
            let tmp_cache_path = opt.cache_path.join("tmp");
            if !tmp_cache_path.exists() {
                std::fs::create_dir_all(&tmp_cache_path)?;
            }
            for missing in missing_digests.iter() {
                let expected_path = cache_path.join(missing.digest.strip_prefix("sha256:").unwrap_or(&missing.digest));
                if !expected_path.exists() {
                    let local_storage = tempfile::NamedTempFile::new_in(&tmp_cache_path)?;
                    eprintln!("Downloading from remote registry: {}, size: {}", &missing.digest, size_to_string(missing.size));
                    source_registry.download_blob(local_storage.path(), &missing.digest, missing.size).await?;
                    std::fs::rename(local_storage.path(), cache_path.join(missing.digest.strip_prefix("sha256:").unwrap_or(&missing.digest)))?;
                }
                destination_registry.upload_blob(&expected_path, &missing.digest, missing.size).await?;
            }
        }
        None =>
            bail!("We still have remaining missing digests that we dont have locally. However we haven't been configured with a source repository, so we have no means to fetch them.")
        }
        }
    }

    println!("All referenced layers present, just metadata uploads remaining");

    let manifest = manifest.set_specification_type(pusher_config.registry_type()?);

    let (config_sha, config_sha_len) = Sha256Value::from_path(&config_path).await?;
    let config_sha_printed = format!("sha256:{}", config_sha);
    let expected_sha = &manifest.config.digest;

    if expected_sha != &config_sha_printed {
        bail!("The config we have on disk at {:?}, does not have the same sha as the manifest expects. Got: {}, expected: {}", &config_path, config_sha_printed, expected_sha)
    }
    if !destination_registry
        .blob_exists(&config_sha_printed)
        .await?
    {
        destination_registry
            .upload_blob(&config_path, &config_sha_printed, config_sha_len.0 as u64)
            .await?;
    }

    // First lets upload the manifest keyed by the digest.

    destination_registry
        .upload_manifest(&pusher_config.repository, &manifest, &tags)
        .await?;

    Ok(())
}
