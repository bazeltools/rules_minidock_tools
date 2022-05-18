use std::path::Path;

use serde::{Deserialize, Serialize};

use anyhow::{bail, Error};

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
pub struct HistoryItem {
    pub author: Option<String>,
    pub created: String,
    pub created_by: String,
}

impl TryFrom<crate::container_specs::docker_types::config::HistoryItem> for HistoryItem {
    type Error = anyhow::Error;

    fn try_from(
        value: crate::container_specs::docker_types::config::HistoryItem,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            author: value.author,
            created: value.created,
            created_by: value.created_by,
        })
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
pub struct RootFs {
    #[serde(rename = "type")]
    pub root_type: String,
    pub diff_ids: Vec<String>,
}
impl RootFs {
    pub fn add_layer(&mut self, digest: impl AsRef<str>) {
        self.diff_ids.push(digest.as_ref().to_string());
    }
}
impl Default for RootFs {
    fn default() -> Self {
        Self {
            root_type: String::from("layers"),
            diff_ids: Default::default(),
        }
    }
}

impl TryFrom<crate::container_specs::docker_types::config::RootFs> for RootFs {
    type Error = anyhow::Error;

    fn try_from(
        value: crate::container_specs::docker_types::config::RootFs,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            root_type: String::from("layers"),
            diff_ids: value.diff_ids,
        })
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Default, Clone)]
pub struct InnerConfig {
    #[serde(rename = "Entrypoint", alias = "entrypoint")]
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

impl TryFrom<crate::container_specs::docker_types::config::InnerConfig> for InnerConfig {
    type Error = anyhow::Error;

    fn try_from(
        value: crate::container_specs::docker_types::config::InnerConfig,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            entrypoint: value.entrypoint,
            env: value.env,
            cmd: value.cmd,
            image: value.image,
            args_escaped: value.args_escaped,
            user: value.user,
            workdir: value.workdir,
        })
    }
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

fn invert<T, E>(d: Option<Result<T, E>>) -> Result<Option<T>, E> {
    match d {
        Some(Ok(e)) => Ok(Some(e)),
        Some(Err(e)) => Err(e),
        None => Ok(None),
    }
}

impl TryFrom<crate::container_specs::docker_types::config::Config> for Config {
    type Error = anyhow::Error;

    fn try_from(
        value: crate::container_specs::docker_types::config::Config,
    ) -> Result<Self, Self::Error> {
        let history = invert(value.history.map(|h| {
            h.into_iter()
                .map(|e| e.try_into())
                .collect::<Result<Vec<HistoryItem>, Self::Error>>()
        }));
        Ok(Self {
            architecture: value.architecture,
            author: value.author,
            created: value.created,
            os: value.os,
            history: history?,
            rootfs: invert(value.rootfs.map(|e| e.try_into()))?,
            config: invert(value.config.map(|e| e.try_into()))?,
        })
    }
}

impl Config {
    pub fn add_layer(&mut self, digest: impl AsRef<str>) {
        if self.rootfs.is_none() {
            self.rootfs = Some(Default::default());
        }
        if let Some(r) = &mut self.rootfs {
            r.add_layer(digest);
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

pub fn merge_config<'a>(current: &'a mut Config, next: &Config) -> Result<&'a mut Config, Error> {
    fn merge_inner_cfgs(cur_cfg: &mut InnerConfig, e: &InnerConfig) -> Result<(), Error> {
        if e.entrypoint
            .as_ref()
            .map(|e| !e.is_empty())
            .unwrap_or(false)
        {
            cur_cfg.entrypoint = e.entrypoint.clone();
        }

        if e.env.as_ref().map(|e| !e.is_empty()).unwrap_or(false) {
            cur_cfg.env = e.env.clone();
        }

        if e.cmd.as_ref().map(|e| !e.is_empty()).unwrap_or(false) {
            cur_cfg.cmd = e.cmd.clone();
        }

        if e.image.as_ref().map(|e| !e.is_empty()).unwrap_or(false) {
            cur_cfg.image = e.image.clone();
        }

        if e.args_escaped.is_some() {
            cur_cfg.args_escaped = e.args_escaped.clone();
        }
        Ok(())
    }

    if let Some(arch) = &next.architecture {
        if !arch.is_empty() {
            current.architecture = Some(arch.clone());
        }
    }

    if let Some(e) = &next.author {
        if !e.is_empty() {
            current.author = Some(e.clone());
        }
    }

    if let Some(e) = &next.created {
        if !e.is_empty() {
            current.created = Some(e.clone());
        }
    }

    if let Some(e) = &next.created {
        if !e.is_empty() {
            current.created = Some(e.clone());
        }
    }

    if let Some(e) = &next.os {
        if !e.is_empty() {
            current.os = Some(e.clone());
        }
    }

    if let Some(e) = &next.rootfs {
        if current.rootfs.is_some() {
            bail!(
                "Unexpected setting root fs when already have a root fs, was: {:#?} -> to {:#?}",
                current.rootfs,
                next.rootfs
            )
        }
        current.rootfs = Some(e.clone());
    }

    if let Some(e) = &next.history {
        if !e.is_empty() {
            if let Some(cur_h) = &mut current.history {
                let mut next_h = e.clone();
                next_h.append(cur_h);
                current.history = Some(next_h);
            } else {
                current.history = Some(e.clone());
            }
        }
    }

    if let Some(e) = &next.config {
        if let Some(cur_cfg) = &mut current.config {
            merge_inner_cfgs(cur_cfg, e)?;
        } else {
            current.config = Some(e.clone());
        }
    }
    Ok(current)
}
