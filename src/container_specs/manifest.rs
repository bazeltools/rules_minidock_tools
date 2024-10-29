use std::path::Path;

use super::{
    blob_reference::{BlobReference, BlobReferenceType},
    SpecificationType,
};
use anyhow::Error;

#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct Manifest {
    pub schema_version: u16,
    pub name: Option<String>,
    pub specification_type: SpecificationType,
    pub config: BlobReference,
    pub layers: Vec<BlobReference>,
}

impl Manifest {
    pub fn media_type(&self) -> &'static str {
        match self.specification_type {
            SpecificationType::Oci => "application/vnd.oci.image.manifest.v1+json",
            SpecificationType::Docker => "application/vnd.docker.distribution.manifest.v2+json",
        }
    }

    pub fn write_file(&self, f: impl AsRef<Path>) -> Result<(), Error> {
        use std::fs::File;
        use std::io::BufWriter;

        // Open the file in read-only mode with buffer.
        let file = File::create(f.as_ref())?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        use std::io::BufWriter;

        // Open the file in read-only mode with buffer.
        let mut buf = Vec::default();
        let writer = BufWriter::new(&mut buf);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(buf)
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

    pub fn set_specification_type(mut self, specification_type: SpecificationType) -> Manifest {
        self.config.specification_type = specification_type;
        self.layers
            .iter_mut()
            .map(|l| l.specification_type = specification_type)
            .count();
        self
    }

    pub fn update_config(
        &mut self,
        compressed_sha_v: crate::hash::sha256_value::Sha256Value,
        compressed_size: crate::hash::sha256_value::DataLen,
    ) {
        self.config = BlobReference {
            size: compressed_size.0 as u64,
            digest: format!("sha256:{}", compressed_sha_v),
            ..self.config
        };
    }

    pub fn add_layer(
        &mut self,
        compressed_sha_v: crate::hash::sha256_value::Sha256Value,
        compressed_size: crate::hash::sha256_value::DataLen,
        blob_reference_type: BlobReferenceType,
    ) {
        self.layers.push(BlobReference {
            blob_reference_type,
            specification_type: self.specification_type,
            size: compressed_size.0 as u64,
            digest: format!("sha256:{}", compressed_sha_v),
        });
    }
}
