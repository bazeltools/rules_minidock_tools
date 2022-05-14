use anyhow::bail;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(name = "basic")]
struct Opt {
    #[clap(long, parse(from_os_str))]
    pusher_config_path: PathBuf,

    #[clap(long, parse(from_os_str))]
    relative_search_path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();

    let pusher_config = rules_minidock_tools::docker_types::pusher_config::Layers::parse_file(
        &opt.pusher_config_path,
    )?;

    let relative_search_path = opt.relative_search_path.clone();

    let (merge_config, layers) = rules_minidock_tools::merge_outputs::merge(&pusher_config, &relative_search_path).await?;

    println!("merged_config: {:#?}", merge_config);
    println!("layers: {:#?}", layers);
    Ok(())
}
