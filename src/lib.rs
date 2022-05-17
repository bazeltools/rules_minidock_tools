use std::path::Path;

use anyhow::Error;
use merge_config::RemoteMetadata;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
pub struct PathPair {
    pub short_path: String,
    pub path: String,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
pub struct LayerConfig {
    pub layer_data: String,
    pub compressed_length: u64,
    pub outer_sha256: String,
    pub inner_sha256: String,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
pub struct UploadMetadata {
    pub layer_configs: Vec<LayerConfig>,
    pub remote_metadata: Option<RemoteMetadata>,
}

impl UploadMetadata {
    pub fn parse_file(f: impl AsRef<Path>) -> Result<UploadMetadata, Error> {
        use std::fs::File;
        use std::io::BufReader;

        // Open the file in read-only mode with buffer.
        let file = File::open(f.as_ref())?;
        let reader = BufReader::new(file);

        let u: UploadMetadata = serde_json::from_reader(reader)?;

        Ok(u)
    }
}

pub mod container_specs;
pub mod hash;
pub mod merge_config;
pub mod merge_outputs;

pub mod registry;
