pub mod blob_reference;
pub mod config;
pub mod manifest;
pub mod serde_impl;

pub use config::ConfigDelta;
pub use manifest::Manifest;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum SpecificationType {
    Oci,
    Docker,
}

impl Default for SpecificationType {
    fn default() -> Self {
        SpecificationType::Oci
    }
}
