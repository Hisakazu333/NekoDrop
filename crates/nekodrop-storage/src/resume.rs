use std::collections::HashSet;
use std::fs;
use std::path::Path;

use nekodrop_core::{FileManifest, ManifestItem, ManifestItemKind, NekoDropError, NekoDropResult};

use crate::checksum::verify_sha256_file;
use crate::receive_dir::safe_join_receive_path;
use crate::received_file::partial_path_for;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeFileState {
    pub path: String,
    pub received_bytes: u64,
    pub expected_bytes: u64,
    pub sha256: Option<String>,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumePlan {
    pub transfer_id: String,
    pub files: Vec<ResumeFileState>,
}

impl ResumePlan {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn total_received_bytes(&self) -> u64 {
        self.files.iter().map(|file| file.received_bytes).sum()
    }

    pub fn completed_file_count(&self) -> usize {
        self.files.iter().filter(|file| file.completed).count()
    }

    pub fn partial_file_count(&self) -> usize {
        self.files.iter().filter(|file| !file.completed).count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeExpectedFile {
    pub path: String,
    pub size: u64,
    pub sha256: Option<String>,
}

impl ResumeExpectedFile {
    pub fn new(path: impl Into<String>, size: u64, sha256: Option<String>) -> NekoDropResult<Self> {
        let path = path.into();
        if path.trim().is_empty() {
            return Err(NekoDropError::InvalidManifestPath(path));
        }
        Ok(Self { path, size, sha256 })
    }
}

pub fn build_resume_plan(
    receive_dir: &Path,
    transfer_id: impl Into<String>,
    manifest: &FileManifest,
) -> NekoDropResult<ResumePlan> {
    let expected_files = manifest
        .items
        .iter()
        .filter(|item| item.kind == ManifestItemKind::File)
        .map(expected_file_from_manifest_item)
        .collect::<NekoDropResult<Vec<_>>>()?;

    build_resume_plan_for_files(receive_dir, transfer_id, &expected_files)
}

pub fn build_resume_plan_for_files(
    receive_dir: &Path,
    transfer_id: impl Into<String>,
    expected_files: &[ResumeExpectedFile],
) -> NekoDropResult<ResumePlan> {
    let transfer_id = transfer_id.into();
    if transfer_id.trim().is_empty() {
        return Err(NekoDropError::Storage(
            "resume transfer_id cannot be empty".into(),
        ));
    }

    let mut seen_paths = HashSet::new();
    let mut files = Vec::new();
    for expected in expected_files {
        if !seen_paths.insert(expected.path.clone()) {
            return Err(NekoDropError::Storage(format!(
                "duplicate resume path: {}",
                expected.path
            )));
        }
        if let Some(state) = inspect_resume_file_state(receive_dir, expected)? {
            files.push(state);
        }
    }

    Ok(ResumePlan { transfer_id, files })
}

pub fn inspect_resume_file_state(
    receive_dir: &Path,
    expected: &ResumeExpectedFile,
) -> NekoDropResult<Option<ResumeFileState>> {
    let destination = safe_join_receive_path(receive_dir, &expected.path)?;
    let partial_path = partial_path_for(&destination)?;
    let destination_exists = destination.exists();
    let partial_exists = partial_path.exists();

    if destination_exists && partial_exists {
        return Err(NekoDropError::Storage(format!(
            "resume conflict for {}: destination and partial file both exist",
            expected.path
        )));
    }

    if destination_exists {
        let metadata = fs::metadata(&destination).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to read metadata for {}: {error}",
                destination.display()
            ))
        })?;
        if !metadata.is_file() {
            return Err(NekoDropError::Storage(format!(
                "resume destination is not a file: {}",
                destination.display()
            )));
        }
        let size = metadata.len();
        if size > expected.size {
            return Err(NekoDropError::Storage(format!(
                "resume destination is larger than expected for {}: {} > {}",
                expected.path, size, expected.size
            )));
        }
        if size < expected.size {
            return Err(NekoDropError::Storage(format!(
                "resume destination is incomplete but not partial for {}: {} < {}",
                expected.path, size, expected.size
            )));
        }
        if let Some(expected_sha256) = normalized_sha256(expected.sha256.as_deref()) {
            let verified = verify_sha256_file(&destination, expected_sha256)?;
            if !verified {
                return Err(NekoDropError::Storage(format!(
                    "resume destination checksum mismatch for {}",
                    expected.path
                )));
            }
        }
        return Ok(Some(ResumeFileState {
            path: expected.path.clone(),
            received_bytes: size,
            expected_bytes: expected.size,
            sha256: expected.sha256.clone(),
            completed: true,
        }));
    }

    if partial_exists {
        let metadata = fs::metadata(&partial_path).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to read metadata for {}: {error}",
                partial_path.display()
            ))
        })?;
        if !metadata.is_file() {
            return Err(NekoDropError::Storage(format!(
                "resume partial is not a file: {}",
                partial_path.display()
            )));
        }
        let size = metadata.len();
        if size == 0 {
            return Ok(None);
        }
        if size > expected.size {
            return Err(NekoDropError::Storage(format!(
                "resume partial is larger than expected for {}: {} > {}",
                expected.path, size, expected.size
            )));
        }
        return Ok(Some(ResumeFileState {
            path: expected.path.clone(),
            received_bytes: size,
            expected_bytes: expected.size,
            sha256: expected.sha256.clone(),
            completed: false,
        }));
    }

    Ok(None)
}

fn expected_file_from_manifest_item(item: &ManifestItem) -> NekoDropResult<ResumeExpectedFile> {
    ResumeExpectedFile::new(item.path.clone(), item.size, item.sha256.clone())
}

fn normalized_sha256(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use nekodrop_core::{FileManifest, ManifestItem};

    use super::*;
    use crate::checksum::sha256_file;

    #[test]
    fn builds_resume_plan_from_partial_files() {
        let dir = unique_temp_dir("resume-partial");
        let manifest = FileManifest::new(
            "drop",
            vec![ManifestItem::file("drop/sample.txt", 10).unwrap()],
        );
        fs::create_dir_all(dir.join("drop")).unwrap();
        fs::write(dir.join("drop/sample.txt.nekodrop-part"), b"hello").unwrap();

        let plan = build_resume_plan(&dir, "transfer-1", &manifest).unwrap();

        assert_eq!(plan.transfer_id, "transfer-1");
        assert_eq!(plan.total_received_bytes(), 5);
        assert_eq!(plan.partial_file_count(), 1);
        assert_eq!(
            plan.files,
            vec![ResumeFileState {
                path: "drop/sample.txt".to_string(),
                received_bytes: 5,
                expected_bytes: 10,
                sha256: None,
                completed: false,
            }]
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn includes_verified_completed_files() {
        let dir = unique_temp_dir("resume-complete");
        fs::create_dir_all(dir.join("drop")).unwrap();
        let file = dir.join("drop/sample.txt");
        fs::write(&file, b"hello").unwrap();
        let checksum = sha256_file(&file).unwrap().value;
        let mut item = ManifestItem::file("drop/sample.txt", 5).unwrap();
        item.sha256 = Some(checksum.clone());
        let manifest = FileManifest::new("drop", vec![item]);

        let plan = build_resume_plan(&dir, "transfer-1", &manifest).unwrap();

        assert_eq!(plan.completed_file_count(), 1);
        assert_eq!(plan.partial_file_count(), 0);
        assert_eq!(plan.total_received_bytes(), 5);
        assert_eq!(
            plan.files,
            vec![ResumeFileState {
                path: "drop/sample.txt".to_string(),
                received_bytes: 5,
                expected_bytes: 5,
                sha256: Some(checksum),
                completed: true,
            }]
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_partial_files_larger_than_expected() {
        let dir = unique_temp_dir("resume-large-partial");
        let expected = ResumeExpectedFile::new("sample.txt", 3, None).unwrap();
        fs::write(dir.join("sample.txt.nekodrop-part"), b"too large").unwrap();

        let error = inspect_resume_file_state(&dir, &expected).unwrap_err();

        assert!(error
            .to_string()
            .contains("resume partial is larger than expected"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_destination_and_partial_conflicts() {
        let dir = unique_temp_dir("resume-conflict");
        let expected = ResumeExpectedFile::new("sample.txt", 5, None).unwrap();
        fs::write(dir.join("sample.txt"), b"hello").unwrap();
        fs::write(dir.join("sample.txt.nekodrop-part"), b"he").unwrap();

        let error = inspect_resume_file_state(&dir, &expected).unwrap_err();

        assert!(error.to_string().contains("destination and partial"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_completed_files_with_wrong_checksum() {
        let dir = unique_temp_dir("resume-bad-checksum");
        let expected = ResumeExpectedFile::new(
            "sample.txt",
            5,
            Some("0000000000000000000000000000000000000000000000000000000000000000".into()),
        )
        .unwrap();
        fs::write(dir.join("sample.txt"), b"hello").unwrap();

        let error = inspect_resume_file_state(&dir, &expected).unwrap_err();

        assert!(error
            .to_string()
            .contains("resume destination checksum mismatch"));

        fs::remove_dir_all(dir).unwrap();
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nekodrop-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
