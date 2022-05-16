use crate::{docker_types::{self, manifest, config::Config}, hash::sha256_value::{self, Sha256Value, DataLen}};


use std::path::{Path, PathBuf};

use anyhow::{Error, bail};

#[derive(Debug, PartialEq, Eq)]
pub struct OutputLayer {
    pub content: docker_types::PathPair,
    pub sha256: Sha256Value,
    pub inner_sha_v: Sha256Value,
    pub compressed_size: DataLen,
    pub uncompressed_size: DataLen
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct LayerUploads {
    pub layers: Vec<OutputLayer>
}


pub async fn merge(
        layers: &docker_types::pusher_config::Layers,
        relative_search_path: &Option<PathBuf>
) -> Result<(docker_types::config::Config, docker_types::manifest::Manifest, LayerUploads), anyhow::Error> {

    let rel_as_path = |rel: &str| {
        relative_search_path.as_ref().map(|p| p.join(rel)).unwrap_or_else(|| PathBuf::from(rel))
    };

    let mut cfg = docker_types::config::Config::default();
    let mut manifest = docker_types::manifest::Manifest::default();

    let mut layer_uploads = LayerUploads::default();

    for layer in layers.0.iter() {

        if let Some(base_config) = &layer.base_config {
            let cfg_path = rel_as_path(&base_config.path);
            if !cfg_path.exists() {
                bail!("Probably a bad search path or config path, unable to find {:?}", cfg_path)
            }
            docker_types::config::merge_config(&mut cfg, & docker_types::config::Config::parse_file(&cfg_path)?)?;
        }

        if let Some(base_manifest) = &layer.base_manifest {
            let p = rel_as_path(&base_manifest.path);

            if !p.exists() {
                bail!("Probably a bad search path or config path, unable to find {:?}", p)
            }
            docker_types::manifest::merge_manifest(&mut manifest, & docker_types::manifest::Manifest::parse_file(&p)?)?;
        }

        if let Some(config) = &layer.config {
            docker_types::config::merge_config(&mut cfg, config)?;
        }
        if let Some(layer) = &layer.current_layer {
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
                uncompressed_size
            });
        }
    }
    Ok((cfg, manifest, layer_uploads))
}