use anyhow::bail;
use anyhow::Context;

use clap::Parser;

use indicatif::MultiProgress;
use indicatif::ProgressBar;
use rules_minidock_tools::container_specs::ConfigDelta;
use rules_minidock_tools::container_specs::Manifest;
use rules_minidock_tools::container_specs::SpecificationType;
use rules_minidock_tools::hash::sha256_value::Sha256Value;

use rules_minidock_tools::registry::ops::ActionsTaken;
use rules_minidock_tools::registry::ops::RequestState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[clap(name = "pusher app")]
struct Opt {
    #[clap(long, parse(from_os_str))]
    pusher_config: PathBuf,

    #[clap(long, parse(from_os_str))]
    cache_path: PathBuf,

    #[clap(long)]
    verbose: bool,
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
            (Some(registry), Some(repository)) => Some(
                rules_minidock_tools::registry::from_maybe_domain_and_name(&registry, &repository)
                    .await?,
            ),
        }
    } else {
        None
    };

    let mut local_digests: HashMap<String, PathBuf> = HashMap::default();
    for local_data in upload_metadata.layer_configs.iter() {
        let local_layer_path: PathBuf = (&local_data.layer_data).into();
        local_digests.insert(local_data.outer_sha256.clone(), local_layer_path);
    }

    let cache_path = opt.cache_path.join("tmp");
    let tmp_cache_path = opt.cache_path.join("tmp");
    if !tmp_cache_path.exists() {
        std::fs::create_dir_all(&tmp_cache_path)?;
    }

    let request_state = Arc::new(RequestState {
        local_digests,
        destination_registry: Arc::clone(&destination_registry),
        source_registry,
        cache_path,
    });

    let mp = Arc::new(MultiProgress::new());

    mp.set_alignment(indicatif::MultiProgressAlignment::Bottom);

    let pb_main = mp.add(ProgressBar::new(manifest.layers.len() as u64));

    let mut tokio_data = Vec::default();
    for layer in manifest.layers.iter() {
        let layer = layer.clone();
        let request_state = Arc::clone(&request_state);
        let pb_main = pb_main.clone();
        let mp = mp.clone();

        tokio_data.push(tokio::spawn(async move {
            pb_main.tick();
            let r = rules_minidock_tools::registry::ops::ensure_present(&layer, request_state, mp)
                .await;
            pb_main.inc(1);
            r
        }))
    }

    let mut actions_taken = ActionsTaken::default();
    for join_result in tokio_data {
        actions_taken.merge(&join_result.await??);
    }
    pb_main.finish();

    println!("All referred to layers have been ensured present, actions taken:\n{}\nMetadata uploads commencing", actions_taken);

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
            .upload_blob(
                &config_path,
                &config_sha_printed,
                config_sha_len.0 as u64,
                None,
            )
            .await?;
    }

    // First lets upload the manifest keyed by the digest.

    destination_registry
        .upload_manifest(&pusher_config.repository, &manifest, &tags)
        .await?;

    Ok(())
}
