use anyhow::bail;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(name = "basic")]
struct Opt {
    #[clap(long, parse(from_os_str))]
    manifest_path: Option<PathBuf>,

    #[clap(long, parse(from_os_str))]
    output_manifest_path: Option<PathBuf>,

    #[clap(long, parse(from_os_str))]
    config_path: Option<PathBuf>,

    #[clap(long, parse(from_os_str))]
    output_config_path: Option<PathBuf>,

    #[clap(long, parse(from_os_str))]
    pusher_config_path: Option<PathBuf>,

    #[clap(long, parse(from_os_str))]
    output_pusher_config_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();

    if let Some(manifest_path) = &opt.manifest_path {
        let manifest =
            rules_minidock_tools::docker_types::manifest::Manifest::parse_file(manifest_path)?;
        println!("{:#?}", manifest);

        if let Some(output_manifest_path) = &opt.output_manifest_path {
            manifest.write_file(output_manifest_path)?;
        }
    } else if let Some(_output_manifest_path) = &opt.output_manifest_path {
        bail!("Tried to write a manifest output, but had no input path specified")
    }

    if let Some(config_path) = &opt.config_path {
        let config = rules_minidock_tools::docker_types::config::Config::parse_file(config_path)?;
        println!("{:#?}", config);

        if let Some(output_config_path) = &opt.output_config_path {
            config.write_file(output_config_path)?;
        }
    } else if let Some(_output_config_path) = &opt.output_config_path {
        bail!("Tried to write a _output_config_path output, but had no input path specified")
    }

    if let Some(pusher_config_path) = &opt.pusher_config_path {
        let pusher_config = rules_minidock_tools::docker_types::pusher_config::Layers::parse_file(
            pusher_config_path,
        )?;
        println!("{:#?}", pusher_config);

        if let Some(output_pusher_config_path) = &opt.output_pusher_config_path {
            pusher_config.write_file(output_pusher_config_path)?;
        }
    } else if let Some(_output_pusher_config_path) = &opt.output_pusher_config_path {
        bail!("Tried to write a _output_pusher_config_path output, but had no input path specified")
    }

    println!("Hello, world!");
    Ok(())
}
