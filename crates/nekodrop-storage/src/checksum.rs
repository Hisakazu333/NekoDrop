use std::fs::File;
use std::io::Read;
use std::path::Path;

use nekodrop_core::{NekoDropError, NekoDropResult};
use sha2::{Digest, Sha256};

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

pub fn sha256_file(path: &Path) -> NekoDropResult<Checksum> {
    let mut file = File::open(path).map_err(|error| {
        NekoDropError::Storage(format!("failed to open {}: {error}", path.display()))
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            NekoDropError::Storage(format!("failed to read {}: {error}", path.display()))
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(Checksum::sha256(hex::encode(hasher.finalize())))
}

pub fn verify_sha256_file(path: &Path, expected: &str) -> NekoDropResult<bool> {
    let actual = sha256_file(path)?;
    Ok(actual.value.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn calculates_real_sha256_for_file() {
        let dir =
            std::env::temp_dir().join(format!("nekodrop-checksum-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("sample.txt");
        fs::write(&file, b"hello nekodrop").unwrap();

        let checksum = sha256_file(&file).unwrap();

        assert_eq!(
            checksum.value,
            "f49094b3f5b957c2ee590959502e4ff231b997c3ca527f7ce83dcd375d2ebc2d"
        );

        fs::remove_dir_all(&dir).unwrap();
    }
}
