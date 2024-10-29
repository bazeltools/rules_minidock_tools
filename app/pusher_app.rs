use anyhow::bail;
use anyhow::Context;

use clap::Parser;

use indicatif::MultiProgress;
use indicatif::ProgressBar;
use indicatif::ProgressDrawTarget;
use indicatif::ProgressStyle;
use rules_minidock_tools::container_specs::ConfigDelta;
use rules_minidock_tools::container_specs::Manifest;
use rules_minidock_tools::container_specs::SpecificationType;
use rules_minidock_tools::hash::sha256_value::Sha256Value;

use rules_minidock_tools::registry::ops::ActionsTaken;
use rules_minidock_tools::registry::ops::RequestState;
use rules_minidock_tools::registry::Registry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Parser, Debug)]
#[clap(name = "pusher app")]
struct Opt {
    #[clap(long)]
    pusher_config: PathBuf,

    #[clap(long)]
    cache_path: PathBuf,

    #[clap(long)]
    verbose: bool,

    /// Do not upload the manifests, meaning just blobs that don't effect tags/usage are synchronized.
    #[clap(long)]
    skip_manifest_upload: bool,

    #[clap(long)]
    // List of comma separated helpers. with the registry:helper_path
    //e.g. foo.gcr.io:/path/to/helper,bar.gcr.io:/path/to/helper2
    docker_authorization_helpers: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
pub struct PusherConfig {
    pub manifest_path: String,
    pub config_path: String,
    pub upload_metadata_path: String,

    pub registry_list: Vec<String>,
    registry_type: String,
    pub repository: String,
    pub container_tags: Option<Vec<String>>,
    pub container_tag_file: Option<String>,
    pub stamp_info_file: String,
    pub stamp_to_env: bool,
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

    let docker_authorization_helpers = if let Some(arg) = &opt.docker_authorization_helpers {
        Arc::new(rules_minidock_tools::registry::DockerAuthenticationHelper::from_str(arg)?)
    } else {
        Default::default()
    };

    let pusher_config_content = std::fs::read_to_string(&opt.pusher_config)?;
    let pusher_config: PusherConfig = serde_json::from_str(pusher_config_content.as_str())
        .with_context(|| {
            format!(
                "Attempting to pusher config from file: {},content:\n{}",
                &opt.pusher_config.to_string_lossy(),
                pusher_config_content
            )
        })?;

    let config_path = PathBuf::from(&pusher_config.config_path);

    let manifest = {
        let manifest_path = PathBuf::from(&pusher_config.manifest_path);
        let manifest_bytes = std::fs::read(&manifest_path)?;
        Manifest::parse(&manifest_bytes)?
    };

    let tags = load_tags(&pusher_config)?;
    if tags.is_empty() {
        bail!("No tags specified, unable to know where to push a manifest. Try 'latest' ? ")
    }
    let upload_metadata_path = PathBuf::from(&pusher_config.upload_metadata_path);
    let upload_metadata = rules_minidock_tools::UploadMetadata::parse_file(&upload_metadata_path)?;

    let destination_registries_setup: Vec<
        Result<tokio::task::JoinHandle<Result<Arc<dyn Registry>, anyhow::Error>>, anyhow::Error>,
    > = pusher_config
        .registry_list
        .iter()
        .map(|r| {
            let r = r.clone();
            if r.is_empty() {
                bail!("Passed in an invalid registry, its an empty string.")
            }
            let repository = pusher_config.repository.clone();
            let docker_authorization_helpers = docker_authorization_helpers.clone();
            Ok(tokio::spawn(async move {
                rules_minidock_tools::registry::from_maybe_domain_and_name(
                    &r,
                    &repository,
                    docker_authorization_helpers,
                )
                .await
            }))
        })
        .collect();

    let mut destination_registries = vec![];
    for r in destination_registries_setup {
        destination_registries.push(r?.await??);
    }

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
                rules_minidock_tools::registry::from_maybe_domain_and_name(
                    &registry,
                    &repository,
                    docker_authorization_helpers,
                )
                .await?,
            ),
        }
    } else {
        None
    };

    let mut manifest = manifest.set_specification_type(pusher_config.registry_type()?);

    let mut config = ConfigDelta::parse_file(&config_path)?;
    if pusher_config.stamp_to_env {
        if let Ok(content) = std::fs::read_to_string(&pusher_config.stamp_info_file) {
            let mut execution_config = std::mem::take(&mut config.config).unwrap_or_default();
            let mut env = std::mem::take(&mut execution_config.env).unwrap_or_default();

            for ln in content.lines() {
                if let Some((key, v)) = ln.split_once(" ") {
                    env.push(format!(
                        "{}={}",
                        key.strip_prefix("STABLE_").unwrap_or(key),
                        v
                    ));
                }
            }
            execution_config.env = Some(env);
            config.config = Some(execution_config);
        }
    }
    let tmp_config = tempfile::NamedTempFile::new()?;
    config.write_file(tmp_config.path())?;
    let config_path: PathBuf = tmp_config.path().to_path_buf();
    let (config_sha, config_len) = Sha256Value::from_path(&config_path).await?;
    manifest.update_config(config_sha, config_len);
    let config_sha_printed = manifest.config.digest.clone();

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

    let mp = Arc::new(MultiProgress::with_draw_target(
        ProgressDrawTarget::stderr_with_hz(12),
    ));

    mp.set_alignment(indicatif::MultiProgressAlignment::Top);

    let concurrent_io_operations: &'static Semaphore = Box::leak(Box::new(Semaphore::new(32)));

    let mut tokio_data = Vec::default();

    for destination_registry in destination_registries.iter().cloned() {
        let request_state = Arc::new(RequestState {
            local_digests: local_digests.clone(),
            destination_registry: Arc::clone(&destination_registry),
            source_registry: source_registry.clone(),
            cache_path: cache_path.clone(),
        });

        for layer in manifest.layers.iter() {
            let layer = layer.clone();
            let request_state = Arc::clone(&request_state);
            let mp = mp.clone();

            tokio_data.push(tokio::spawn(async move {
                rules_minidock_tools::registry::ops::ensure_present(
                    &layer,
                    request_state,
                    mp,
                    concurrent_io_operations,
                )
                .await
            }))
        }

        let config_sha_printed = config_sha_printed.clone();
        let config_path = config_path.clone();
        tokio_data.push(tokio::spawn(async move {
            match destination_registry.blob_exists(&config_sha_printed).await {
                Ok(true) => Ok(ActionsTaken::default()),
                Err(e) => Err(e),
                Ok(false) => destination_registry
                    .upload_blob(&config_path, &config_sha_printed, config_len.0 as u64, None)
                    .await
                    .map(|_| ActionsTaken::default()),
            }
        }));
    }

    let mut actions_taken = ActionsTaken::default();
    for join_result in tokio_data {
        actions_taken.merge(&join_result.await??);
    }

    println!(
        "\n\nAll referred to layers have been ensured present, actions taken:{}\n",
        actions_taken
    );
    if opt.skip_manifest_upload {
        mp.clear()?;
        mp.set_draw_target(ProgressDrawTarget::hidden());
        drop(mp);

        println!("Asked to skip manifest uploads, exiting.");
        return Ok(());
    }

    println!("Manifest uploads commencing");

    let mut tokio_data = Vec::default();

    // First lets upload the manifest keyed by the digest.
    let manifest = Arc::new(manifest);
    for destination_registry in destination_registries.iter() {
        for t in tags.iter() {
            let message_style = ProgressStyle::with_template("{msg}").unwrap();
            let message_pb = ProgressBar::new(1);
            message_pb.set_style(message_style.clone());
            let pb = mp.add(message_pb);

            pb.set_message(format!(
                "Uploading tag {} to {}",
                t,
                destination_registry.registry_name()
            ));

            let t = t.clone();
            let destination_registry = destination_registry.clone();
            let manifest = Arc::clone(&manifest);
            tokio_data.push(tokio::spawn(async move {
                let r = destination_registry.upload_manifest(&manifest, &t).await;
                if r.is_ok() {
                    pb.set_message(format!("{}", console::style("✔").green()));
                } else {
                    pb.set_message(format!("{}", console::style("x").red()));
                }
                r.map(|s| (s, t))
            }));
        }
    }

    let mut uploaded_locations = Vec::default();
    for join_result in tokio_data {
        let uploaded_location = join_result.await??;
        uploaded_locations.push(uploaded_location);
    }

    mp.clear()?;
    mp.set_draw_target(ProgressDrawTarget::hidden());
    drop(mp);

    for (uploaded_location, tag) in uploaded_locations {
        if let Some(loc) = uploaded_location {
            eprintln!("Uploaded {} to: {}", tag, loc);
        } else {
            eprintln!(
                "Skipped an upload, manifest already present for tag {}",
                tag
            );
        }
    }
    Ok(())
}
