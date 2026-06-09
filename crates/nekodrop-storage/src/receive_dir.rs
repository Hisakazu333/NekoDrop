use std::path::{Component, Path, PathBuf};

use nekodrop_core::{NekoDropError, NekoDropResult};

pub fn safe_join_receive_path(receive_dir: &Path, manifest_path: &str) -> NekoDropResult<PathBuf> {
    let relative = Path::new(manifest_path);
    if relative.is_absolute() {
        return Err(NekoDropError::InvalidManifestPath(
            manifest_path.to_string(),
        ));
    }

    for component in relative.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(NekoDropError::InvalidManifestPath(
                manifest_path.to_string(),
            ));
        }
    }

    Ok(receive_dir.join(relative))
}
