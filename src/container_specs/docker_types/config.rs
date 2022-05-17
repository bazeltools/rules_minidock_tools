use serde::{Deserialize, Serialize};

use anyhow::Error;

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
pub struct HistoryItem {
    pub author: Option<String>,
    pub created: String,
    pub created_by: String,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Default, Clone)]
pub struct RootFs {
    #[serde(rename = "type")]
    pub root_type: String,
    pub diff_ids: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Default, Clone)]
pub struct InnerConfig {
    #[serde(rename = "Entrypoint", alias = "entry_point")]
    pub entrypoint: Option<Vec<String>>,

    #[serde(rename = "Env", alias = "env")]
    pub env: Option<Vec<String>>,

    #[serde(rename = "Cmd", alias = "cmd")]
    pub cmd: Option<Vec<String>>,

    #[serde(rename = "Image")]
    pub image: Option<String>,

    #[serde(rename = "ArgsEscaped")]
    pub args_escaped: Option<bool>,

    pub user: Option<String>,
    pub workdir: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct ManifestConfig {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub size: u64,
    digest: String,
}

#[derive(Deserialize, Serialize, Default, Debug, PartialEq, Eq, Clone)]
pub struct Config {
    pub architecture: Option<String>,
    pub author: Option<String>,
    pub created: Option<String>,
    pub history: Option<Vec<HistoryItem>>,
    pub os: Option<String>,
    pub rootfs: Option<RootFs>,
    pub config: Option<InnerConfig>,
}

impl Config {
    pub fn parse_str(f: impl AsRef<str>) -> Result<Config, Error> {
        let u: Config = serde_json::from_str(f.as_ref())?;
        Ok(u)
    }
}
