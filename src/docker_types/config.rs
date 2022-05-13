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
pub struct RootFs {
    #[serde(rename = "type")]
    pub root_type: String,
    pub diff_ids: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct InnerConfig {
    #[serde(rename = "Entrypoint")]
    pub entrypoint: Option<Vec<String>>,

    #[serde(rename = "Env", default = "Vec::default")]
    pub env: Vec<String>,

    #[serde(rename = "Cmd")]
    pub cmd: Option<Vec<String>>,

    #[serde(rename = "Image")]
    pub image: String,

    #[serde(rename = "ArgsEscaped")]
    pub args_escaped: Option<bool>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct ManifestConfig {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub size: u64,
    digest: String,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct Config {
    pub architecture: String,
    pub author: String,
    pub created: String,
    pub history: Vec<HistoryItem>,
    pub os: String,
    pub rootfs: RootFs,
    pub config: InnerConfig,
}

impl Config {
    pub fn write_file(&self, f: impl AsRef<Path>) -> Result<(), Error> {
        use std::fs::File;
        use std::io::BufWriter;

        // Open the file in read-only mode with buffer.
        let file = File::create(f.as_ref())?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }

    pub fn parse_file(f: impl AsRef<Path>) -> Result<Config, Error> {
        use std::fs::File;
        use std::io::BufReader;

        // Open the file in read-only mode with buffer.
        let file = File::open(f.as_ref())?;
        let reader = BufReader::new(file);

        let u: Config = serde_json::from_reader(reader)?;

        Ok(u)
    }
}
