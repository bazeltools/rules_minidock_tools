use crate::{
    container_specs::{self, ConfigDelta, Manifest},
    hash::sha256_value::{DataLen, Sha256Value},
    PathPair,
};

use crate::container_specs::config::ExecutionConfig;
use std::{collections::HashMap, path::PathBuf};

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
    external_execution_configs: Vec<ExecutionConfig>,
) -> Result<
    (
        container_specs::ConfigDelta,
        container_specs::Manifest,
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

    let mut cfg = ConfigDelta::default();
    let mut manifest = Manifest::default();

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
            cfg = ConfigDelta::parse_file(&cfg_path)?;
        }

        if let Some(base_manifest) = &remote_info.manifest {
            let p = rel_as_path(&base_manifest.path);

            if !p.exists() {
                bail!(
                    "Probably a bad search path or config path, unable to find {:?}",
                    p
                )
            }
            manifest = Manifest::parse_file(&p)?;
        }
    }

    struct ShaData {
        pub compressed: bool,
        pub path: PathBuf,
        pub sha: Sha256Value,
        pub len: DataLen,
    }

    let mut pass_one_data = Vec::default();
    for info in merge_config.infos.iter() {
        if let Some(layer) = &info.data {
            let pb = rel_as_path(&layer.path);
            if !pb.exists() {
                bail!("Layer path likely incorrect, unable to find {:?}", pb)
            }

            let pb2 = pb.clone();
            let sp = tokio::spawn(async {
                let pb2 = pb2;
                Sha256Value::from_path(&pb2).await.map(|(s, c)| ShaData {
                    compressed: true,
                    path: pb2,
                    sha: s,
                    len: c,
                })
            });
            pass_one_data.push(sp);

            let pb2 = pb.clone();
            let sp = tokio::spawn(async {
                let pb2 = pb2;
                Sha256Value::from_path_uncompressed(&pb2)
                    .await
                    .map(|(s, c)| ShaData {
                        compressed: false,
                        path: pb2,
                        sha: s,
                        len: c,
                    })
            });

            pass_one_data.push(sp);
        }
    }

    let mut lut_data: HashMap<(PathBuf, bool), (Sha256Value, DataLen)> = HashMap::default();

    for m in pass_one_data {
        let s = m.await??;
        lut_data.insert((s.path, s.compressed), (s.sha, s.len));
    }

    // External configs get merged first, then rules-based configs
    for config in external_execution_configs {
        let mut external_base_cfg = ConfigDelta::default();
        external_base_cfg.config = Some(config);
        cfg.update_with(&external_base_cfg);
    }

    for info in merge_config.infos.iter() {
        if let Some(config) = &info.config {
            cfg.update_with(config);
        }
        if let Some(layer) = &info.data {
            let pb = rel_as_path(&layer.path);
            if !pb.exists() {
                bail!("Layer path likely incorrect, unable to find {:?}", pb)
            }
            let (compressed_sha_v, compressed_size) =
                lut_data.get(&(pb.clone(), true)).ok_or_else(|| {
                    anyhow::anyhow!("Unable to find data in map. shouldn't be possible")
                })?;
            let (inner_sha_v, uncompressed_size) =
                lut_data.get(&(pb.clone(), false)).ok_or_else(|| {
                    anyhow::anyhow!("Unable to find data in map. shouldn't be possible")
                })?;
            let _sha_str_fmt = format!("sha256:{}", inner_sha_v);
            cfg.add_layer(inner_sha_v);

            manifest.add_layer(
                *compressed_sha_v,
                *compressed_size,
                container_specs::blob_reference::BlobReferenceType::LayerGz,
            );
            layer_uploads.layers.push(OutputLayer {
                content: layer.clone(),
                sha256: *compressed_sha_v,
                compressed_size: *compressed_size,
                inner_sha_v: *inner_sha_v,
                uncompressed_size: *uncompressed_size,
            });
        }
    }

    Ok((cfg, manifest, layer_uploads))
}
