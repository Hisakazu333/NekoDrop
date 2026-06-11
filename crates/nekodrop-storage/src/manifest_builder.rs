use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use nekodrop_core::{FileManifest, ManifestItem, NekoDropError, NekoDropResult};
use walkdir::WalkDir;

use crate::checksum::sha256_file;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSourceFile {
    pub manifest_path: String,
    pub source_path: PathBuf,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSourcePlan {
    pub manifest: FileManifest,
    pub files: Vec<TransferSourceFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferPlanScanPhase {
    Started,
    Scanning,
    Hashing,
    Completed,
}

impl TransferPlanScanPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Scanning => "scanning",
            Self::Hashing => "hashing",
            Self::Completed => "completed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferPlanScanProgress {
    pub phase: TransferPlanScanPhase,
    pub current_path: Option<String>,
    pub files_found: usize,
    pub directories_found: usize,
    pub bytes_found: u64,
}

impl TransferPlanScanProgress {
    fn new(phase: TransferPlanScanPhase) -> Self {
        Self {
            phase,
            current_path: None,
            files_found: 0,
            directories_found: 0,
            bytes_found: 0,
        }
    }
}

impl TransferSourcePlan {
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|file| file.size).sum()
    }
}

pub fn create_manifest_from_paths(paths: &[PathBuf]) -> NekoDropResult<FileManifest> {
    Ok(create_source_plan_from_paths(paths)?.manifest)
}

pub fn create_source_plan_from_paths(paths: &[PathBuf]) -> NekoDropResult<TransferSourcePlan> {
    create_source_plan_from_paths_with_progress(paths, |_| {})
}

pub fn create_source_plan_from_paths_with_progress<F>(
    paths: &[PathBuf],
    mut on_progress: F,
) -> NekoDropResult<TransferSourcePlan>
where
    F: FnMut(TransferPlanScanProgress),
{
    if paths.is_empty() {
        return Err(NekoDropError::Storage(
            "at least one file or directory must be selected".into(),
        ));
    }

    on_progress(TransferPlanScanProgress::new(
        TransferPlanScanPhase::Started,
    ));

    let root_name = manifest_root_name(paths)?;
    let mut builder = ManifestBuilder::new(&mut on_progress);

    for path in paths {
        builder.push_path(path)?;
    }

    builder.emit(TransferPlanScanPhase::Completed, None);

    let manifest = FileManifest::new(root_name, builder.items);
    Ok(TransferSourcePlan {
        manifest,
        files: builder.files,
    })
}

struct ManifestBuilder<'a> {
    items: Vec<ManifestItem>,
    files: Vec<TransferSourceFile>,
    seen_paths: HashSet<String>,
    progress: TransferPlanScanProgress,
    on_progress: &'a mut dyn FnMut(TransferPlanScanProgress),
}

impl<'a> ManifestBuilder<'a> {
    fn new(on_progress: &'a mut dyn FnMut(TransferPlanScanProgress)) -> Self {
        Self {
            items: Vec::new(),
            files: Vec::new(),
            seen_paths: HashSet::new(),
            progress: TransferPlanScanProgress::new(TransferPlanScanPhase::Started),
            on_progress,
        }
    }

    fn push_path(&mut self, path: &Path) -> NekoDropResult<()> {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to read metadata for {}: {error}",
                path.display()
            ))
        })?;

        if metadata.file_type().is_symlink() {
            return Err(NekoDropError::Storage(format!(
                "symbolic links are not supported yet: {}",
                path.display()
            )));
        }

        let base = path.parent().unwrap_or_else(|| Path::new(""));

        if metadata.is_file() {
            self.push_file(base, path, &metadata)?;
            return Ok(());
        }

        if metadata.is_dir() {
            self.push_directory_tree(base, path)?;
            return Ok(());
        }

        Err(NekoDropError::Storage(format!(
            "unsupported file system entry: {}",
            path.display()
        )))
    }

    fn push_directory_tree(&mut self, base: &Path, root: &Path) -> NekoDropResult<()> {
        for entry in WalkDir::new(root).follow_links(false).sort_by_file_name() {
            let entry = entry.map_err(|error| {
                NekoDropError::Storage(format!("failed to scan {}: {error}", root.display()))
            })?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(path).map_err(|error| {
                NekoDropError::Storage(format!(
                    "failed to read metadata for {}: {error}",
                    path.display()
                ))
            })?;

            if metadata.file_type().is_symlink() {
                return Err(NekoDropError::Storage(format!(
                    "symbolic links are not supported yet: {}",
                    path.display()
                )));
            }

            if metadata.is_dir() {
                self.push_directory(base, path, &metadata)?;
            } else if metadata.is_file() {
                self.push_file(base, path, &metadata)?;
            } else {
                return Err(NekoDropError::Storage(format!(
                    "unsupported file system entry: {}",
                    path.display()
                )));
            }
        }

        Ok(())
    }

    fn push_directory(
        &mut self,
        base: &Path,
        path: &Path,
        metadata: &fs::Metadata,
    ) -> NekoDropResult<()> {
        let manifest_path = relative_manifest_path(base, path)?;
        self.ensure_unique(&manifest_path)?;
        self.progress.directories_found += 1;
        self.emit(TransferPlanScanPhase::Scanning, Some(manifest_path.clone()));
        let mut item = ManifestItem::directory(manifest_path)?;
        item.modified_at = modified_at_seconds(metadata);
        self.items.push(item);
        Ok(())
    }

    fn push_file(
        &mut self,
        base: &Path,
        path: &Path,
        metadata: &fs::Metadata,
    ) -> NekoDropResult<()> {
        let manifest_path = relative_manifest_path(base, path)?;
        self.ensure_unique(&manifest_path)?;
        self.progress.files_found += 1;
        self.progress.bytes_found += metadata.len();
        self.emit(TransferPlanScanPhase::Hashing, Some(manifest_path.clone()));
        let checksum = sha256_file(path)?;
        let mut item = ManifestItem::file(manifest_path, metadata.len())?;
        item.modified_at = modified_at_seconds(metadata);
        item.sha256 = Some(checksum.value.clone());
        self.files.push(TransferSourceFile {
            manifest_path: item.path.clone(),
            source_path: path.to_path_buf(),
            size: item.size,
            sha256: checksum.value,
        });
        self.items.push(item);
        Ok(())
    }

    fn ensure_unique(&mut self, manifest_path: &str) -> NekoDropResult<()> {
        if !self.seen_paths.insert(manifest_path.to_string()) {
            return Err(NekoDropError::Storage(format!(
                "duplicate manifest path: {manifest_path}"
            )));
        }
        Ok(())
    }

    fn emit(&mut self, phase: TransferPlanScanPhase, current_path: Option<String>) {
        self.progress.phase = phase;
        self.progress.current_path = current_path;
        (self.on_progress)(self.progress.clone());
    }
}

fn manifest_root_name(paths: &[PathBuf]) -> NekoDropResult<String> {
    if paths.len() != 1 {
        return Ok("NekoDrop Transfer".to_string());
    }

    let file_name = paths[0]
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| {
            NekoDropError::Storage(format!(
                "path has no valid file name: {}",
                paths[0].display()
            ))
        })?;

    if file_name.trim().is_empty() {
        return Err(NekoDropError::Storage(format!(
            "path has no valid file name: {}",
            paths[0].display()
        )));
    }

    Ok(file_name.to_string())
}

fn relative_manifest_path(base: &Path, path: &Path) -> NekoDropResult<String> {
    let relative = path.strip_prefix(base).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to make {} relative to {}: {error}",
            path.display(),
            base.display()
        ))
    })?;

    path_to_manifest_string(relative)
}

fn path_to_manifest_string(path: &Path) -> NekoDropResult<String> {
    let mut parts = Vec::new();

    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let part = value.to_str().ok_or_else(|| {
                    NekoDropError::Storage(format!("path is not valid UTF-8: {}", path.display()))
                })?;
                parts.push(part.to_string());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(NekoDropError::InvalidManifestPath(
                    path.to_string_lossy().to_string(),
                ));
            }
        }
    }

    if parts.is_empty() {
        return Err(NekoDropError::InvalidManifestPath(
            path.to_string_lossy().to_string(),
        ));
    }

    Ok(parts.join("/"))
}

fn modified_at_seconds(metadata: &fs::Metadata) -> Option<String> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs().to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use nekodrop_core::ManifestItemKind;

    use super::*;

    #[test]
    fn builds_manifest_from_real_directory_tree() {
        let dir = unique_temp_dir("manifest-tree");
        let root = dir.join("drop");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("hello.txt"), b"hello nekodrop").unwrap();
        fs::write(root.join("nested").join("data.bin"), b"abc").unwrap();

        let manifest = create_manifest_from_paths(&[root.clone()]).unwrap();

        assert_eq!(manifest.root_name, "drop");
        assert_eq!(manifest.file_count(), 2);
        assert_eq!(manifest.total_bytes(), 17);
        assert!(manifest
            .items
            .iter()
            .any(|item| item.path == "drop" && item.kind == ManifestItemKind::Directory));
        let hello = manifest
            .items
            .iter()
            .find(|item| item.path == "drop/hello.txt")
            .unwrap();
        assert_eq!(hello.size, 14);
        assert_eq!(
            hello.sha256.as_deref(),
            Some("f49094b3f5b957c2ee590959502e4ff231b997c3ca527f7ce83dcd375d2ebc2d")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn builds_manifest_from_multiple_real_files() {
        let dir = unique_temp_dir("manifest-files");
        let first = dir.join("a.txt");
        let second = dir.join("b.txt");
        fs::write(&first, b"a").unwrap();
        fs::write(&second, b"bb").unwrap();

        let manifest = create_manifest_from_paths(&[first, second]).unwrap();

        assert_eq!(manifest.root_name, "NekoDrop Transfer");
        assert_eq!(manifest.file_count(), 2);
        assert_eq!(manifest.total_bytes(), 3);
        assert_eq!(
            manifest
                .items
                .iter()
                .map(|item| item.path.as_str())
                .collect::<Vec<_>>(),
            vec!["a.txt", "b.txt"]
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn builds_source_plan_with_real_source_paths() {
        let dir = unique_temp_dir("source-plan");
        let root = dir.join("drop");
        fs::create_dir_all(root.join("nested")).unwrap();
        let first = root.join("nested").join("one.txt");
        let second = root.join("two.txt");
        fs::write(&first, b"one").unwrap();
        fs::write(&second, b"two").unwrap();

        let plan = create_source_plan_from_paths(&[root]).unwrap();

        assert_eq!(plan.file_count(), 2);
        assert_eq!(plan.total_bytes(), 6);
        assert_eq!(
            plan.files
                .iter()
                .map(|file| (file.manifest_path.as_str(), file.source_path.as_path()))
                .collect::<Vec<_>>(),
            vec![
                ("drop/nested/one.txt", first.as_path()),
                ("drop/two.txt", second.as_path())
            ]
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn emits_scan_progress_while_building_source_plan() {
        let dir = unique_temp_dir("manifest-progress");
        let root = dir.join("drop");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested").join("one.txt"), b"one").unwrap();
        fs::write(root.join("two.txt"), b"two").unwrap();

        let mut events = Vec::new();
        let plan = create_source_plan_from_paths_with_progress(&[root], |event| events.push(event))
            .unwrap();

        assert_eq!(plan.file_count(), 2);
        assert_eq!(plan.total_bytes(), 6);
        assert_eq!(
            events.first().map(|event| event.phase.as_str()),
            Some("started")
        );
        assert_eq!(
            events.last().map(|event| event.phase.as_str()),
            Some("completed")
        );
        assert!(events
            .iter()
            .any(|event| event.phase.as_str() == "scanning" && event.directories_found >= 1));
        assert!(events.iter().any(|event| event.phase.as_str() == "hashing"
            && event.current_path.as_deref() == Some("drop/nested/one.txt")));
        assert!(events.iter().any(|event| event.phase.as_str() == "hashing"
            && event.files_found == 2
            && event.bytes_found == 6));

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
