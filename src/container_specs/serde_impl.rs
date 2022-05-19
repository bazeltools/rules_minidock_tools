use std::path::Path;

use serde::{ser::SerializeStruct, Deserialize, Serialize, Serializer};

use super::{
    blob_reference::BlobReference, blob_reference::BlobReferenceType, manifest::Manifest,
    SpecificationType,
};
use anyhow::{bail, Error};
use serde::de::Error as SerdeError;

impl<'de> Deserialize<'de> for BlobReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawVersion {
            #[serde(rename = "mediaType")]
            pub media_type: String,
            pub size: u64,
            pub digest: String,
        }
        let r = RawVersion::deserialize(deserializer)?;
        let (specification_type, blob_reference_type) = match r.media_type.as_str() {
            "application/vnd.oci.image.config.v1+json" => {
                (SpecificationType::Oci, BlobReferenceType::Config)
            }
            "application/vnd.docker.container.image.v1+json" => {
                (SpecificationType::Docker, BlobReferenceType::Config)
            }
            "application/vnd.oci.image.layer.v1.tar+gzip" => {
                (SpecificationType::Oci, BlobReferenceType::LayerGz)
            }
            "application/vnd.oci.image.layer.v1.tar" => {
                (SpecificationType::Oci, BlobReferenceType::Layer)
            }
            "application/vnd.docker.image.rootfs.diff.tar.gzip" => {
                (SpecificationType::Docker, BlobReferenceType::Layer)
            }
            "application/vnd.docker.image.rootfs.diff.tar" => {
                (SpecificationType::Docker, BlobReferenceType::Layer)
            }
            other => return Err(D::Error::custom(format!("Invalid media type: {}", other))),
        };

        Ok(BlobReference {
            blob_reference_type,
            specification_type,
            size: r.size,
            digest: r.digest,
        })
    }
}
impl Serialize for BlobReference {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // 3 is the number of fields in the struct.
        let mut state = serializer.serialize_struct("BlobReference", 3)?;

        let media_type = match (&self.specification_type, &self.blob_reference_type) {
            (SpecificationType::Oci, BlobReferenceType::Config) => {
                "application/vnd.oci.image.config.v1+json"
            }
            (SpecificationType::Docker, BlobReferenceType::Config) => {
                "application/vnd.docker.container.image.v1+json"
            }
            (SpecificationType::Oci, BlobReferenceType::LayerGz) => {
                "application/vnd.oci.image.layer.v1.tar+gzip"
            }
            (SpecificationType::Oci, BlobReferenceType::Layer) => {
                "application/vnd.oci.image.layer.v1.tar"
            }
            (SpecificationType::Docker, BlobReferenceType::LayerGz) => {
                "application/vnd.docker.image.rootfs.diff.tar.gzip"
            }
            (SpecificationType::Docker, BlobReferenceType::Layer) => {
                "application/vnd.docker.image.rootfs.diff.tar"
            }
        };

        state.serialize_field("mediaType", media_type)?;
        state.serialize_field("size", &self.size)?;
        state.serialize_field("digest", &self.digest)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Manifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawVersion {
            #[serde(rename = "schemaVersion")]
            pub schema_version: u16,
            #[serde(rename = "mediaType")]
            pub media_type: String,
            pub config: BlobReference,
            pub layers: Vec<BlobReference>,
        }
        let r = RawVersion::deserialize(deserializer)?;
        let specification_type = match r.media_type.as_str() {
            "application/vnd.oci.image.config.v1+json" => SpecificationType::Oci,
            "application/vnd.docker.distribution.manifest.v2+json" => SpecificationType::Docker,
            other => return Err(D::Error::custom(format!("Invalid media type: {}", other))),
        };

        Ok(Manifest {
            schema_version: r.schema_version,
            specification_type,
            config: r.config,
            layers: r.layers,
        })
    }
}

impl Serialize for Manifest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // 3 is the number of fields in the struct.
        let mut state = serializer.serialize_struct("Manifest", 4)?;

        state.serialize_field("mediaType", self.media_type())?;
        state.serialize_field("schemaVersion", &self.schema_version)?;
        state.serialize_field("config", &self.config)?;
        state.serialize_field("layers", &self.layers)?;
        state.end()
    }
}
