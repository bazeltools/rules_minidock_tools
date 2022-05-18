use std::path::PathBuf;

use anyhow::bail;
use clap::Parser;

use rules_minidock_tools::container_specs::docker_types::manifest::ManifestReference;
use rules_minidock_tools::hash::sha256_value::Sha256Value;
use rules_minidock_tools::registry::Registry;
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
}

enum ContentFlavor {
    Docker,
    Oci,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();

    let registry = rules_minidock_tools::registry::from_maybe_domain_and_name(&opt.registry, &opt.repository).await?;
    let manifest_ret = registry.fetch_manifest_as_string(&opt.digest).await?;

    let content_flavor = match manifest_ret.content_type.as_ref().map(|e| e.as_str()) {
        Some("application/vnd.docker.distribution.manifest.v2+json") => ContentFlavor::Docker,
        _ => bail!("Unknown content type, response was: {:#?}", &manifest_ret),
    };

    let cfg_path = PathBuf::from("config.json");
    let manifest_path = PathBuf::from("manifest.json");
    match content_flavor {
        ContentFlavor::Docker => {
            let mut docker_manifest =
                rules_minidock_tools::container_specs::docker_types::manifest::Manifest::parse_str(
                    &manifest_ret.content,
                )?;
            let config_str = registry
                .fetch_config_as_string(&docker_manifest.config.digest)
                .await?;
            let docker_config =
                rules_minidock_tools::container_specs::docker_types::config::Config::parse_str(
                    &config_str.content,
                )?;

            let oci_config: rules_minidock_tools::container_specs::oci_types::config::Config =
                docker_config.try_into()?;

            oci_config.write_file(&cfg_path)?;

            let (sha_v, data_len) = Sha256Value::from_path(&cfg_path).await?;

            docker_manifest.config = ManifestReference {
                media_type: String::from("application/vnd.oci.image.config.v1+json"),
                size: data_len.0 as u64,
                digest: format!("sha256:{}", sha_v),
            };

            let oci_manifest: rules_minidock_tools::container_specs::oci_types::manifest::Manifest =
                docker_manifest.try_into()?;

            oci_manifest.write_file(&manifest_path)?;
        }
        ContentFlavor::Oci => todo!(),
    }
    // println!("Hello, world! -- {:#?}", r.body());
    Ok(())
}
