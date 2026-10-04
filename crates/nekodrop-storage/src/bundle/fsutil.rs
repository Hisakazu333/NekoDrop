// bundle 模块共享工具：JSON 原子读写、manifest 路径规范化、
// bundle_id 安全校验与协议错误映射。
use std::{fs, io::Write, path::Path};

use nekodrop_core::{NekoDropError, NekoDropResult};
use nekolink_protocol::ProtocolError;

pub(crate) fn read_json_file<T: for<'de> serde::Deserialize<'de>>(
    path: &Path,
) -> NekoDropResult<T> {
    let bytes = fs::read(path).map_err(|error| {
        NekoDropError::Storage(format!("failed to read {}: {error}", path.display()))
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        NekoDropError::Storage(format!("failed to parse {}: {error}", path.display()))
    })
}

pub(crate) fn write_json_file<T: serde::Serialize>(path: &Path, value: &T) -> NekoDropResult<()> {
    let json = serde_json::to_vec_pretty(value).map_err(|error| {
        NekoDropError::Storage(format!("failed to serialize {}: {error}", path.display()))
    })?;

    // Write-through-temp then rename so a crash never leaves a truncated JSON
    // file behind (bundle manifests and import receipts must stay parseable).
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "bundle.json".to_string());
    let temp_path = parent.join(format!(".{file_name}.nekodrop-tmp"));

    let result = (|| {
        let mut file = fs::File::create(&temp_path).map_err(|error| {
            NekoDropError::Storage(format!("failed to create {}: {error}", temp_path.display()))
        })?;
        file.write_all(&json).map_err(|error| {
            NekoDropError::Storage(format!("failed to write {}: {error}", temp_path.display()))
        })?;
        file.sync_all().map_err(|error| {
            NekoDropError::Storage(format!("failed to sync {}: {error}", temp_path.display()))
        })
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    fs::rename(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        NekoDropError::Storage(format!("failed to write {}: {error}", path.display()))
    })
}

pub(crate) fn map_protocol_error(result: Result<(), ProtocolError>) -> NekoDropResult<()> {
    result.map_err(protocol_to_storage_error)
}

pub(crate) fn protocol_to_storage_error(error: ProtocolError) -> NekoDropError {
    NekoDropError::Storage(format!("invalid bundle: {}", error.message))
}

pub(crate) fn path_to_bundle_manifest_path(path: &Path) -> NekoDropResult<String> {
    let path = path.to_str().ok_or_else(|| {
        NekoDropError::Storage(format!("bundle path is not UTF-8: {}", path.display()))
    })?;
    Ok(format!("files/{}", path.replace('\\', "/")))
}

pub(crate) fn validate_bundle_id_for_staging(bundle_id: &str) -> NekoDropResult<()> {
    let trimmed = bundle_id.trim();
    if trimmed.is_empty()
        || trimmed != bundle_id
        || bundle_id.contains('/')
        || bundle_id.contains('\\')
        || bundle_id.contains("..")
        || bundle_id.contains(':')
        || bundle_id.contains('\0')
    {
        return Err(NekoDropError::Storage(format!(
            "bundle_id is not safe for staging: {bundle_id}"
        )));
    }
    Ok(())
}
