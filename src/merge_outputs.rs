use crate::{docker_types::{self, manifest, config::Config}, hash::sha256_value::{self, Sha256Value}};


use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use anyhow::{Error, bail};

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct OutputLayer {
    pub content: String,
    pub sha256: String
}

#[derive(Deserialize, Serialize, Default, Debug, PartialEq, Eq)]
pub struct LayerUploads {
    pub layers: Vec<OutputLayer>
}


pub async fn merge(
        layers: &docker_types::pusher_config::Layers,
        relative_search_path: & PathBuf
) -> Result<(docker_types::config::Config, LayerUploads), anyhow::Error> {

    let mut cfg = docker_types::config::Config::default();
    let mut layer_uploads = LayerUploads::default();

    for layer in layers.0.iter() {

        if let Some(base_config_path) = &layer.base_config {
            let cfg_path = relative_search_path.join(base_config_path);
            if !cfg_path.exists() {
                bail!("Probably a bad search path or config path, unable to find {:?}", cfg_path)
            }
            docker_types::config::merge_config(&mut cfg, & docker_types::config::Config::parse_file(&cfg_path)?)?;
        }
        if let Some(config) = &layer.config {
            docker_types::config::merge_config(&mut cfg, config)?;
        }
        if let Some(layer) = &layer.current_layer {
            if !layer.is_empty() {
                let pb = relative_search_path.join(layer);
                if !pb.exists() {
                    bail!("Layer path likely incorrect, unable to find {:?}", pb)
                }
                let sha_v = Sha256Value::from_path(&pb).await?;
                let sha_str_fmt = format!("sha256:{}", sha_v);
                cfg.add_layer(&sha_str_fmt);
                layer_uploads.layers.push(OutputLayer {
                    content: layer.clone(),
                    sha256: sha_str_fmt
                });
            }
        }
    }
    Ok((cfg, layer_uploads))
}