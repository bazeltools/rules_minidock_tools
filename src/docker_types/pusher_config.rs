use std::path::Path;

use serde::{Deserialize, Serialize};

use anyhow::Error;

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct HistoryItem {
    pub author: Option<String>,
    pub created: String,
    pub created_by: String,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct RemoteFetchConfig {
    pub image_digest: String,
    pub image_registry: String,
    pub image_repository: String,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct Layer {
    pub base_config: Option<String>,
    pub base_manifest: Option<String>,
    pub remote_fetch_config: Option<RemoteFetchConfig>,
    pub current_layer: Option<String>,
    pub config: Option<super::config::Config>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct Layers(pub Vec<Layer>);

impl Layers {
    pub fn write_file(&self, f: impl AsRef<Path>) -> Result<(), Error> {
        use std::fs::File;
        use std::io::BufWriter;

        // Open the file in read-only mode with buffer.
        let file = File::create(f.as_ref())?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }

    pub fn parse_file(f: impl AsRef<Path>) -> Result<Layers, Error> {
        use std::fs::File;
        use std::io::BufReader;

        // Open the file in read-only mode with buffer.
        let file = File::open(f.as_ref())?;
        let reader = BufReader::new(file);

        let u: Layers = serde_json::from_reader(reader)?;

        Ok(u)
    }
}
