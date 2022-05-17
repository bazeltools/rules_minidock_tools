use crate::{
    container_specs::oci_types::{self},
    hash::sha256_value::{DataLen, Sha256Value},
    PathPair,
};

use std::path::PathBuf;

use anyhow::bail;

#[derive(Debug, PartialEq, Eq)]
pub struct OutputLayer {
    pub content: PathPair,
    pub sha256: Sha256Value,
    pub inner_sha_v: Sha256Value,
    pub compressed_size: DataLen,
    pub uncompressed_size: DataLen,
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct LayerUploads {
    pub layers: Vec<OutputLayer>,
}

pub async fn merge(
    merge_config: &super::merge_config::MergeConfig,
    relative_search_path: &Option<PathBuf>,
) -> Result<
    (
        oci_types::config::Config,
        oci_types::manifest::Manifest,
        LayerUploads,
    ),
    anyhow::Error,
> {
    let rel_as_path = |rel: &str| {
        relative_search_path
            .as_ref()
            .map(|p| p.join(rel))
            .unwrap_or_else(|| PathBuf::from(rel))
    };

    let mut cfg = oci_types::config::Config::default();
    let mut manifest = oci_types::manifest::Manifest::default();

    let mut layer_uploads = LayerUploads::default();

    if let Some(remote_info) = &merge_config.remote_metadata {
        if let Some(config) = &remote_info.config {
            let cfg_path = rel_as_path(&config.path);
            if !cfg_path.exists() {
                bail!(
                    "Probably a bad search path or config path, unable to find {:?}",
                    cfg_path
                )
            }
            oci_types::config::merge_config(
                &mut cfg,
                &oci_types::config::Config::parse_file(&cfg_path)?,
            )?;
        }

        if let Some(base_manifest) = &remote_info.manifest {
            let p = rel_as_path(&base_manifest.path);

            if !p.exists() {
                bail!(
                    "Probably a bad search path or config path, unable to find {:?}",
                    p
                )
            }
            oci_types::manifest::merge_manifest(
                &mut manifest,
                &oci_types::manifest::Manifest::parse_file(&p)?,
            )?;
        }
    }

    for info in merge_config.infos.iter() {
        if let Some(config) = &info.config {
            oci_types::config::merge_config(&mut cfg, config)?;
        }
        if let Some(layer) = &info.data {
            let pb = rel_as_path(&layer.path);
            if !pb.exists() {
                bail!("Layer path likely incorrect, unable to find {:?}", pb)
            }
            let (compressed_sha_v, compressed_size) = Sha256Value::from_path(&pb).await?;
            let (inner_sha_v, uncompressed_size) = Sha256Value::from_path_uncompressed(&pb).await?;
            let sha_str_fmt = format!("sha256:{}", inner_sha_v);
            cfg.add_layer(&sha_str_fmt);

            manifest.add_layer(compressed_sha_v, compressed_size);
            layer_uploads.layers.push(OutputLayer {
                content: layer.clone(),
                sha256: compressed_sha_v,
                compressed_size,
                inner_sha_v,
                uncompressed_size,
            });
        }
    }
    Ok((cfg, manifest, layer_uploads))
}
