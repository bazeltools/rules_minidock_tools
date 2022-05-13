use std::path::Path;

use serde::{Deserialize, Serialize};

use anyhow::Error;

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct Layer {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub size: u64,
    digest: String,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct ManifestConfig {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub size: u64,
    digest: String,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct Manifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u16,

    #[serde(rename = "mediaType")]
    pub media_type: String,

    pub config: ManifestConfig,

    pub layers: Vec<Layer>,
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

    pub fn parse_file(f: impl AsRef<Path>) -> Result<Manifest, Error> {
        use std::fs::File;
        use std::io::BufReader;

        // Open the file in read-only mode with buffer.
        let file = File::open(f.as_ref())?;
        let reader = BufReader::new(file);

        let u: Manifest = serde_json::from_reader(reader)?;

        Ok(u)
    }
}
