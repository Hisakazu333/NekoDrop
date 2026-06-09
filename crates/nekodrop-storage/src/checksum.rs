#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumAlgorithm {
    Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checksum {
    pub algorithm: ChecksumAlgorithm,
    pub value: String,
}

impl Checksum {
    pub fn sha256(value: impl Into<String>) -> Self {
        Self {
            algorithm: ChecksumAlgorithm::Sha256,
            value: value.into(),
        }
    }

    pub fn is_present(&self) -> bool {
        !self.value.trim().is_empty()
    }
}
