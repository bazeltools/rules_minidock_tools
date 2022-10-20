use clap::Parser;
use rules_minidock_tools::{hash::sha256_value::Sha256Value, LayerConfig, UploadMetadata};
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(name = "merge app")]
struct Opt {
    #[clap(long)]
    merger_config_path: PathBuf,

    #[clap(long)]
    relative_search_path: Option<PathBuf>,

    #[clap(long)]
    config_path: PathBuf,

    #[clap(long)]
    manifest_path: PathBuf,

    #[clap(long)]
    manifest_sha256_path: PathBuf,

    #[clap(long)]
    upload_metadata_path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();

    let pusher_config =
        rules_minidock_tools::merge_config::MergeConfig::parse_file(&opt.merger_config_path)?;

    let relative_search_path = opt.relative_search_path.clone();

    let (merge_config, mut manifest, layers) =
        rules_minidock_tools::merge_outputs::merge(&pusher_config, &relative_search_path).await?;

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
