use crate::errors::{NekoDropError, NekoDropResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestItemKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestItem {
    pub path: String,
    pub kind: ManifestItemKind,
    pub size: u64,
    pub modified_at: Option<String>,
    pub sha256: Option<String>,
}

impl ManifestItem {
    pub fn file(path: impl Into<String>, size: u64) -> NekoDropResult<Self> {
        let path = normalize_manifest_path(path.into())?;
        Ok(Self {
            path,
            kind: ManifestItemKind::File,
            size,
            modified_at: None,
            sha256: None,
        })
    }

    pub fn directory(path: impl Into<String>) -> NekoDropResult<Self> {
        let path = normalize_manifest_path(path.into())?;
        Ok(Self {
            path,
            kind: ManifestItemKind::Directory,
            size: 0,
            modified_at: None,
            sha256: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileManifest {
    pub root_name: String,
    pub items: Vec<ManifestItem>,
}

impl FileManifest {
    pub fn new(root_name: impl Into<String>, items: Vec<ManifestItem>) -> Self {
        Self {
            root_name: root_name.into(),
            items,
        }
    }

    pub fn file_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.kind == ManifestItemKind::File)
            .count()
    }

    pub fn total_bytes(&self) -> u64 {
        self.items.iter().map(|item| item.size).sum()
    }
}

pub fn normalize_manifest_path(path: String) -> NekoDropResult<String> {
    let trimmed = path.trim().replace('\\', "/");
    if trimmed.is_empty()
        || trimmed.starts_with('/')
        || trimmed.contains("../")
        || trimmed == ".."
        || trimmed.contains('\0')
    {
        return Err(NekoDropError::InvalidManifestPath(path));
    }

    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_that_escape_destination() {
        assert!(ManifestItem::file("../secret.txt", 1).is_err());
        assert!(ManifestItem::file("/tmp/secret.txt", 1).is_err());
    }

    #[test]
    fn totals_file_sizes_only() {
        let manifest = FileManifest::new(
            "sample",
            vec![
                ManifestItem::file("a.txt", 10).unwrap(),
                ManifestItem::directory("folder").unwrap(),
                ManifestItem::file("folder/b.txt", 15).unwrap(),
            ],
        );

        assert_eq!(manifest.file_count(), 2);
        assert_eq!(manifest.total_bytes(), 25);
    }
}
