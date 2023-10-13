use serde_json;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Error;
use clap::Parser;
use hash::sha256_value::Sha256Value;
use merge_config::MergeConfig;
use merge_config::RemoteMetadata;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
pub struct PathPair {
    pub short_path: String,
    pub path: String,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
pub struct LayerConfig {
    pub layer_data: String,
    pub compressed_length: u64,
    pub outer_sha256: String,
    pub inner_sha256: String,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
pub struct UploadMetadata {
    pub layer_configs: Vec<LayerConfig>,
    pub remote_metadata: Option<RemoteMetadata>,
}

impl UploadMetadata {
    pub fn parse_file(f: impl AsRef<Path>) -> Result<UploadMetadata, Error> {
        use std::fs::File;
        use std::io::BufReader;

        // Open the file in read-only mode with buffer.
        let file = File::open(f.as_ref())?;
        let reader = BufReader::new(file);

        let u: UploadMetadata = serde_json::from_reader(reader)?;

        Ok(u)
    }
}

#[derive(Parser, Debug)]
#[clap(name = "merge app")]
pub struct Opt {
    #[clap(long)]
    pub merger_config_path: PathBuf,

    #[clap(long)]
    pub external_config_path: Option<PathBuf>,

    #[clap(long)]
    pub relative_search_path: Option<PathBuf>,

    #[clap(long)]
    pub config_path: PathBuf,

    #[clap(long)]
    pub manifest_path: PathBuf,

    #[clap(long)]
    pub manifest_sha256_path: PathBuf,

    #[clap(long)]
    pub upload_metadata_path: PathBuf,
}

pub async fn merge_main(opt: Opt) -> Result<(), anyhow::Error> {
    let pusher_config = MergeConfig::parse_file(&opt.merger_config_path)?;

    let external_execution_config = match &opt.external_config_path {
        Some(path) => {
            let json_str = fs::read_to_string(&path)?;
            Some(serde_json::from_str(&json_str)?)
        }
        None => None,
    };

    let relative_search_path = opt.relative_search_path.clone();

    let (merge_config, mut manifest, layers) = merge_outputs::merge(
        &pusher_config,
        &relative_search_path,
        &external_execution_config,
    )
    .await?;

    let config_path = opt.config_path;
    merge_config.write_file(&config_path)?;

    let (config_sha, config_len) = Sha256Value::from_path(&config_path).await?;

    manifest.update_config(config_sha, config_len);

    let manifest_path = opt.manifest_path;
    manifest.write_file(&manifest_path)?;

    let (manifest_sha256, _) = Sha256Value::from_path(&manifest_path).await?;

    let file = File::create(&opt.manifest_sha256_path)?;
    let mut writer = BufWriter::new(file);
    write!(writer, "{}", manifest_sha256)?;
    drop(writer);

    let mut layer_configs = Vec::default();

    for output_layer in layers.layers.iter() {
        layer_configs.push(LayerConfig {
            layer_data: output_layer.content.short_path.clone(),
            outer_sha256: format!("sha256:{}", output_layer.sha256),
            inner_sha256: format!("sha256:{}", output_layer.inner_sha_v),
            compressed_length: output_layer.compressed_size.0 as u64,
        });
    }

    let upload_metadata = UploadMetadata {
        layer_configs,
        remote_metadata: pusher_config.remote_metadata.clone(),
    };

    use std::fs::File;
    use std::io::BufWriter;
    let upload_metadata_path = opt.upload_metadata_path;

    let file = File::create(&upload_metadata_path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &upload_metadata)?;

    Ok(())
}

pub mod container_specs;
pub mod hash;
pub mod merge_config;
pub mod merge_outputs;

pub mod registry;
