use std::{path::PathBuf, sync::Arc};

use anyhow::Context;
use clap::Parser;

use rules_minidock_tools::container_specs::{ConfigDelta, Manifest};

// cargo run --bin puller-app -- --registry l.gcr.io --repository google/bazel --digest sha256:08434856d8196632b936dd082b8e03bae0b41346299aedf60a0d481ab427a69f

#[derive(Parser, Debug)]
#[clap(name = "puller app")]
struct Opt {
    #[clap(long)]
    registry: String,

    #[clap(long)]
    repository: String,

    #[clap(long)]
    digest: String,

    #[clap(long)]
    architecture: String,

    #[clap(long)]
    // List of comma separated helpers. with the registry:helper_path
    //e.g. foo.gcr.io:/path/to/helper,bar.gcr.io:/path/to/helper2
    docker_authorization_helpers: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();

    let docker_authorization_helpers = if let Some(arg) = &opt.docker_authorization_helpers {
        Arc::new(rules_minidock_tools::registry::DockerAuthenticationHelper::from_str(arg)?)
    } else {
        Default::default()
    };

    let registry = rules_minidock_tools::registry::from_maybe_domain_and_name(
        &opt.registry,
        &opt.repository,
        docker_authorization_helpers,
    )
    .await
    .with_context(|| format!("Failed to connect to registry name: {}", opt.registry))?;
    let manifest_ret = registry.fetch_manifest_as_string(&opt.digest).await?;

    let cfg_path = PathBuf::from("config.json");
    let manifest_path = PathBuf::from("manifest.json");

    let manifest = Manifest::parse_str(&manifest_ret.content)?;
    let config_str = registry
        .fetch_config_as_string(&manifest.config.digest)
        .await?;

    let config = ConfigDelta::parse_str(&config_str.content)?;

    config.write_file(&cfg_path)?;

    manifest.write_file(&manifest_path)?;
    Ok(())
}
