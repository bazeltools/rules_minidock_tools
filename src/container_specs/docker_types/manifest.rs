use std::path::Path;

use serde::{Deserialize, Serialize};

use anyhow::{bail, Error};

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Default, Clone)]
pub struct ManifestReference {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub size: u64,
    pub digest: String,
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
