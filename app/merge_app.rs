use clap::Parser;
use rules_minidock_tools::{hash::sha256_value::Sha256Value, LayerConfig, UploadMetadata};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(name = "merge app")]
struct Opt {
    #[clap(long, parse(from_os_str))]
    merger_config_path: PathBuf,

    #[clap(long, parse(from_os_str))]
    relative_search_path: Option<PathBuf>,

    #[clap(long, parse(from_os_str))]
    directory_output: PathBuf,

    #[clap(long, parse(from_os_str))]
    directory_output_short_path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();

    let pusher_config =
        rules_minidock_tools::merge_config::MergeConfig::parse_file(&opt.merger_config_path)?;

    let relative_search_path = opt.relative_search_path.clone();

    let (merge_config, mut manifest, layers) =
        rules_minidock_tools::merge_outputs::merge(&pusher_config, &relative_search_path).await?;

    let config_path = opt.directory_output.join("config.json");
    merge_config.write_file(&config_path)?;

    let (config_sha, config_len) = Sha256Value::from_path(&config_path).await?;

    manifest.update_config(config_sha, config_len);

    let manifest_path = opt.directory_output.join("manifest.json");
    manifest.write_file(&manifest_path)?;

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
    let upload_metadata_path = opt.directory_output.join("upload_metadata.json");

    let file = File::create(&upload_metadata_path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &upload_metadata)?;

    Ok(())
}
