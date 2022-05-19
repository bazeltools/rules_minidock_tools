use std::{collections::HashMap, path::Path};

use serde::{Deserialize, Serialize};

use anyhow::Error;

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]

pub struct ExecutionConfig {
    // The username or UID which is a platform-specific structure that allows specific control over which user the process run as. This acts as a default value to use when the value is not specified when creating a container. For Linux based systems, all of the following are valid: user, uid, user:group, uid:gid, uid:group, user:gid. If group/gid is not specified, the default group and supplementary groups of the given user/uid in /etc/passwd from the container are applied.
    #[serde(rename = "User")]
    pub user: Option<String>,

    // A set of ports to expose from a container running this image. Its keys can be in the format of: port/tcp, port/udp, port with the default protocol being tcp if not specified. These values act as defaults and are merged with any specified when creating a container. NOTE: This JSON structure value is unusual because it is a direct JSON serialization of the Go type map[string]struct{} and is represented in JSON as an object mapping its keys to an empty object.
    #[serde(rename = "ExposedPorts")]
    pub exposed_ports: Option<HashMap<String, ()>>,

    // Entries are in the format of VARNAME=VARVALUE. These values act as defaults and are merged with any specified when creating a container.
    #[serde(rename = "Env")]
    pub env: Option<Vec<String>>,

    // A list of arguments to use as the command to execute when the container starts. These values act as defaults and may be replaced by an entrypoint specified when creating a container.
    #[serde(rename = "Entrypoint", alias="entrypoint")]
    pub entrypoint: Option<Vec<String>>,

    // Default arguments to the entrypoint of the container. These values act as defaults and may be replaced by any specified when creating a container. If an Entrypoint value is not specified, then the first entry of the Cmd array SHOULD be interpreted as the executable to run.
    #[serde(rename = "Cmd")]
    pub cmd: Option<Vec<String>>,

    // A set of directories describing where the process is likely to write data specific to a container instance. NOTE: This JSON structure value is unusual because it is a direct JSON serialization of the Go type map[string]struct{} and is represented in JSON as an object mapping its keys to an empty object.
    #[serde(rename = "Volumes")]
    pub volumes: Option<Vec<String>>,

    // Sets the current working directory of the entrypoint process in the container. This value acts as a default and may be replaced by a working directory specified when creating a container.
    #[serde(rename = "WorkingDir")]
    pub working_dir: Option<String>,

    // The field contains arbitrary metadata for the container. This property MUST use the annotation rules.
    #[serde(rename = "Labels")]
    pub labels: Option<HashMap<String, String>>,

    // The field contains the system call signal that will be sent to the container to exit. The signal can be a signal name in the format SIGNAME, for instance SIGKILL or SIGRTMIN+3.
    #[serde(rename = "StopSignal")]
    pub stop_signal: Option<String>,

    // This property is reserved for use, to maintain compatibility.
    #[serde(rename = "Memory")]
    pub memory: Option<i64>,

    // This property is reserved for use, to maintain compatibility.
    #[serde(rename = "MemorySwap")]
    pub memory_swap: Option<i64>,

    // This property is reserved for use, to maintain compatibility.
    #[serde(rename = "CpuShares")]
    pub cpu_shares: Option<i64>,

    // This property is reserved for use, to maintain compatibility.
    #[serde(rename = "Healthcheck")]
    pub healthcheck: Option<HashMap<String, String>>,
}

impl ExecutionConfig {
    pub(crate) fn update_with<'a>(
        &'a mut self,
        other: &ExecutionConfig,
    ) -> &'a mut ExecutionConfig {
        if let Some(e) = &other.user {
            self.user = Some(e.clone());
        }

        if let Some(e) = &other.exposed_ports {
            self.exposed_ports = Some(e.clone());
        }

        if let Some(e) = &other.env {
            if let Some(our_e) = self.env.as_mut() {
                our_e.extend_from_slice(e)
            } else {
                self.env = Some(e.clone());
            }
        }

        if let Some(e) = &other.entrypoint {
            self.entrypoint = Some(e.clone());
        }

        if let Some(e) = &other.cmd {
            self.cmd = Some(e.clone());
        }

        if let Some(e) = &other.volumes {
            if let Some(our_e) = self.volumes.as_mut() {
                our_e.extend_from_slice(e)
            } else {
                self.volumes = Some(e.clone());
            }
        }

        if let Some(e) = &other.working_dir {
            self.working_dir = Some(e.clone());
        }

        if let Some(e) = &other.labels {
            if let Some(our_e) = self.labels.as_mut() {
                our_e.extend(e.clone())
            } else {
                self.labels = Some(e.clone());
            }
        }

        if let Some(e) = &other.stop_signal {
            self.stop_signal = Some(e.clone());
        }

        if let Some(e) = &other.memory {
            self.memory = Some(*e);
        }

        if let Some(e) = &other.memory_swap {
            self.memory_swap = Some(*e);
        }

        if let Some(e) = &other.cpu_shares {
            self.cpu_shares = Some(*e);
        }

        if let Some(e) = &other.healthcheck {
            self.healthcheck = Some(e.clone());
        }

        self
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]

pub struct HistoryItem {
    // A combined date and time at which the layer was created, formatted as defined by RFC 3339, section 5.6.
    pub created: Option<String>,

    // The author of the build point.
    pub author: Option<String>,

    // The command which created the layer.
    pub created_by: Option<String>,

    // A custom message set when creating the layer.
    pub comment: Option<String>,

    // This field is used to mark if the history item created a filesystem diff. It is set to true if this history item doesn't correspond to an actual layer in the rootfs section (for example, Dockerfile's ENV command results in no change to the filesystem).
    pub empty_layer: Option<bool>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone, Default)]

pub struct RootFs {
    // MUST be set to layers. Implementations MUST generate an error if they encounter a unknown value while verifying or unpacking an image.
    #[serde(rename = "type")]
    pub fs_type: Option<String>,

    // An array of layer content hashes (DiffIDs), in order from first to last.
    pub diff_ids: Option<Vec<String>>,
}
impl RootFs {
    pub fn valid_config(&self) -> Result<(), &'static str> {
        if self.fs_type.as_ref().filter(|e| !e.is_empty()).is_none() {
            return Err("fs_type field in RootFs is empty or none");
        }

        if self.diff_ids.as_ref().filter(|e| !e.is_empty()).is_none() {
            return Err("diff_ids field in RootFs is empty or none");
        }
        Ok(())
    }

    pub(crate) fn update_with<'a>(&'a mut self, other: &RootFs) -> &'a mut RootFs {
        if let Some(e) = &other.fs_type {
            self.fs_type = Some(e.clone());
        }

        if let Some(e) = &other.diff_ids {
            self.diff_ids = Some(e.clone());
        }
        self
    }

    pub fn add_layer(&mut self, uncompressed_sha_v: &crate::hash::sha256_value::Sha256Value) {
        if self.diff_ids.is_none() {
            self.diff_ids = Some(Vec::default());
        }
        if let Some(diff_ids) = self.diff_ids.as_mut() {
            diff_ids.push(format!("sha256:{}", uncompressed_sha_v));
        } else {
            unreachable!()
        }
    }
}
impl ConfigDelta {
    pub fn valid_config(&self) -> Result<(), &'static str> {
        if self
            .architecture
            .as_ref()
            .filter(|e| !e.is_empty())
            .is_none()
        {
            return Err("architecture field is empty or none");
        }

        if self.os.as_ref().filter(|e| !e.is_empty()).is_none() {
            return Err("os field is empty or none");
        }

        if let Some(root_fs) = self.rootfs.as_ref() {
            root_fs.valid_config()?;
        } else {
            return Err("rootfs is none");
        }
        Ok(())
    }

    pub(crate) fn update_with<'a>(&'a mut self, other: &ConfigDelta) -> &'a mut ConfigDelta {
        if let Some(e) = &other.created {
            self.created = Some(e.clone());
        }

        if let Some(e) = &other.author {
            self.author = Some(e.clone());
        }

        if let Some(e) = &other.architecture {
            self.architecture = Some(e.clone());
        }

        if let Some(e) = &other.os {
            self.os = Some(e.clone());
        }

        if let Some(e) = &other.os_version {
            self.os_version = Some(e.clone());
        }
        if let Some(e) = &other.os_features {
            self.os_features = Some(e.clone());
        }

        if let Some(e) = &other.variant {
            self.variant = Some(e.clone());
        }

        if let Some(e) = &other.config {
            if let Some(cfg) = self.config.as_mut() {
                cfg.update_with(e);
            } else {
                self.config = Some(e.clone());
            }
        }

        if let Some(e) = &other.rootfs {
            if let Some(cfg) = self.rootfs.as_mut() {
                cfg.update_with(e);
            } else {
                self.rootfs = Some(e.clone());
            }
        }
        if let Some(e) = &other.history {
            if let Some(cfg) = self.history.as_mut() {
                cfg.extend_from_slice(e);
            } else {
                self.history = Some(e.clone());
            }
        }

        self
    }
}
// This obey's the OCI Configuration spec, however everything is optional.
// As such, the `valid_config` method can be used to query if this is oci/docker compatible
#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone, Default)]

pub struct ConfigDelta {
    //An combined date and time at which the image was created, formatted as defined by RFC 3339, section 5.6.
    pub created: Option<String>,

    // Gives the name and/or email address of the person or entity which created and is responsible for maintaining the image.
    pub author: Option<String>,

    //The CPU architecture which the binaries in this image are built to run on. Configurations SHOULD use, and implementations SHOULD understand, values listed in the Go Language document for GOARCH.
    pub architecture: Option<String>,

    // The name of the operating system which the image is built to run on. Configurations SHOULD use, and implementations SHOULD understand, values listed in the Go Language document for GOOS.
    pub os: Option<String>,

    //This OPTIONAL property specifies the version of the operating system targeted by the referenced blob. Implementations MAY refuse to use manifests where os.version is not known to work with the host OS version. Valid values are implementation-defined. e.g. 10.0.14393.1066 on windows.
    #[serde(rename = "os.version")]
    pub os_version: Option<String>,

    //    This OPTIONAL property specifies an array of strings, each specifying a mandatory OS feature. When os is windows, image indexes SHOULD use, and implementations SHOULD understand the following values:
    #[serde(rename = "os.features")]
    pub os_features: Option<String>,

    // The variant of the specified CPU architecture. Configurations SHOULD use, and implementations SHOULD understand, variant values listed in the Platform Variants table.
    pub variant: Option<String>,

    // The execution parameters which SHOULD be used as a base when running a container using the image. This field can be null, in which case any execution parameters should be specified at creation of the container.
    pub config: Option<ExecutionConfig>,

    // The rootfs key references the layer content addresses used by the image. This makes the image config hash depend on the filesystem hash.
    pub rootfs: Option<RootFs>,

    // Describes the history of each layer. The array is ordered from first to last. The object has the following fields:
    pub history: Option<Vec<HistoryItem>>,
}

impl ConfigDelta {
    pub fn write_file(&self, f: impl AsRef<Path>) -> Result<(), Error> {
        use std::fs::File;
        use std::io::BufWriter;

        // Open the file in read-only mode with buffer.
        let file = File::create(f.as_ref())?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }

    pub fn parse_str(f: impl AsRef<str>) -> Result<ConfigDelta, Error> {
        let u: ConfigDelta = serde_json::from_str(f.as_ref())?;
        Ok(u)
    }

    pub fn parse(config_bytes: &[u8]) -> Result<ConfigDelta, Error> {
        let u: ConfigDelta = serde_json::from_slice(config_bytes)?;
        Ok(u)
    }

    pub fn parse_file(f: impl AsRef<Path>) -> Result<ConfigDelta, Error> {
        use std::fs::File;
        use std::io::BufReader;

        // Open the file in read-only mode with buffer.
        let file = File::open(f.as_ref())?;
        let reader = BufReader::new(file);

        let u: ConfigDelta = serde_json::from_reader(reader)?;

        Ok(u)
    }

    pub fn add_layer(&mut self, uncompressed_sha_v: &crate::hash::sha256_value::Sha256Value) {
        if self.rootfs.is_none() {
            self.rootfs = Some(RootFs::default());
        }
        if let Some(root_fs) = self.rootfs.as_mut() {
            root_fs.add_layer(uncompressed_sha_v);
        } else {
            unreachable!()
        }
    }
}
