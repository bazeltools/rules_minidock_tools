use super::SpecificationType;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum BlobReferenceType {
    Config,
    LayerGz,
    LayerZstd,
    Layer,
}
impl Default for BlobReferenceType {
    fn default() -> Self {
        BlobReferenceType::Config
    }
}

#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct BlobReference {
    pub blob_reference_type: BlobReferenceType,
    pub specification_type: SpecificationType,
    pub size: u64,
    pub digest: String,
}
