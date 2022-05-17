use std::path::Path;

use serde::{Deserialize, Serialize};

use anyhow::{Context, Error};

use super::PathPair;

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct HistoryItem {
    pub author: Option<String>,
    pub created: String,
    pub created_by: String,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
pub struct RemoteMetadata {
    pub config: Option<PathPair>,
    pub manifest: Option<PathPair>,
    pub registry: Option<String>,
    pub repository: Option<String>,
    pub digest: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct Info {
    pub data: Option<PathPair>,
    pub config: Option<super::container_specs::oci_types::config::Config>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct MergeConfig {
    pub infos: Vec<Info>,
    pub remote_metadata: Option<RemoteMetadata>,
}

impl MergeConfig {
    // {"infos":[{"data":{"path":"bazel-out/darwin_arm64-fastbuild/bin/file_a_data.tgz","short_path":"file_a_data.tgz"}},{"data":{"path":"bazel-out/darwin_arm64-fastbuild/bin/file_b_data.tgz","short_path":"file_b_data.tgz"}}],"remote_metadata":null}impl MergeConfig {
    pub fn write_file(&self, f: impl AsRef<Path>) -> Result<(), Error> {
        use std::fs::File;
        use std::io::BufWriter;

        // Open the file in read-only mode with buffer.
        let file = File::create(f.as_ref())?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }

    pub fn parse_file(f: impl AsRef<Path>) -> Result<MergeConfig, Error> {
        // Open the file in read-only mode with buffer.
        let content = std::fs::read_to_string(f.as_ref())?;
        let u: MergeConfig = serde_json::from_str(content.as_str()).with_context(|| {
            format!(
                "Attempting to parse layers from file: {},content:\n{}",
                f.as_ref().to_string_lossy(),
                content
            )
        })?;

        Ok(u)
    }
}
