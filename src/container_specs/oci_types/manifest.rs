use std::path::Path;

use serde::{Deserialize, Serialize};

use anyhow::{bail, Error};

fn from_docker_media_type(docker_media_type: &str) -> Result<&'static str, Error> {
    match docker_media_type {
        "application/vnd.docker.image.rootfs.diff.tar.gzip" => Ok("application/vnd.oci.image.layer.v1.tar+gzip"),
        "application/vnd.oci.image.config.v1+json" => Ok("application/vnd.oci.image.config.v1+json"),
        _other => bail!("Unknown media type {:#?}, or unable to perform conversion. Config manifest conversions require fetching the config so must be done first/separately.", docker_media_type)
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Default, Clone)]
pub struct ManifestReference {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub size: u64,
    pub digest: String,
}

impl TryFrom<crate::container_specs::docker_types::manifest::ManifestReference>
    for ManifestReference
{
    type Error = anyhow::Error;

    fn try_from(
        value: crate::container_specs::docker_types::manifest::ManifestReference,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            size: value.size,
            digest: value.digest,
            media_type: from_docker_media_type(&value.media_type)?.to_string(),
        })
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct Manifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u16,

    #[serde(rename = "mediaType")]
    pub media_type: String,

    pub config: ManifestReference,

    pub layers: Vec<ManifestReference>,
}

impl TryFrom<crate::container_specs::docker_types::manifest::Manifest> for Manifest {
    type Error = anyhow::Error;

    fn try_from(
        value: crate::container_specs::docker_types::manifest::Manifest,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            schema_version: value.schema_version,
            media_type: String::from("application/vnd.oci.image.manifest.v1+json"),
            config: value.config.try_into()?,
            layers: value
                .layers
                .into_iter()
                .map(|e| e.try_into())
                .collect::<Result<Vec<ManifestReference>, Self::Error>>()?,
        })
    }
}

pub fn merge_manifest<'a>(
    current: &'a mut Manifest,
    next: &Manifest,
) -> Result<&'a mut Manifest, Error> {
    if !current.layers.is_empty() && !next.layers.is_empty() {
        bail!("Tried to merge manifests where both have layers, unclear what to do here. merge {:#?} into {:#?}", next, current)
    }

    current.layers = next.layers.clone();
    Ok(current)
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            schema_version: 2,
            media_type: String::from("application/vnd.oci.image.manifest.v1+json"),
            config: Default::default(),
            layers: Default::default(),
        }
    }
}

impl Manifest {
    pub fn write_file(&self, f: impl AsRef<Path>) -> Result<(), Error> {
        use std::fs::File;
        use std::io::BufWriter;

        // Open the file in read-only mode with buffer.
        let file = File::create(f.as_ref())?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }

    pub fn parse_str(f: impl AsRef<str>) -> Result<Manifest, Error> {
        let u: Manifest = serde_json::from_str(f.as_ref())?;
        Ok(u)
    }

    pub fn parse(manifest_bytes: &[u8]) -> Result<Manifest, Error> {
        let u: Manifest = serde_json::from_slice(manifest_bytes)?;
        Ok(u)
    }

    pub fn parse_file(f: impl AsRef<Path>) -> Result<Manifest, Error> {
        use std::fs::File;
        use std::io::BufReader;

        // Open the file in read-only mode with buffer.
        let file = File::open(f.as_ref())?;
        let reader = BufReader::new(file);

        let u: Manifest = serde_json::from_reader(reader)?;

        Ok(u)
    }

    pub fn update_config(
        &mut self,
        compressed_sha_v: crate::hash::sha256_value::Sha256Value,
        compressed_size: crate::hash::sha256_value::DataLen,
    ) {
        self.config = ManifestReference {
            media_type: String::from("application/vnd.oci.image.layer.v1.tar+gzip"),
            size: compressed_size.0 as u64,
            digest: format!("sha256:{}", compressed_sha_v),
        };
    }

    pub fn add_layer(
        &mut self,
        compressed_sha_v: crate::hash::sha256_value::Sha256Value,
        compressed_size: crate::hash::sha256_value::DataLen,
    ) {
        self.layers.push(ManifestReference {
            media_type: String::from("application/vnd.oci.image.layer.v1.tar+gzip"),
            size: compressed_size.0 as u64,
            digest: format!("sha256:{}", compressed_sha_v),
        });
    }
}
