use super::*;

pub(crate) const STAGED_BUNDLE_RETENTION_SECS: u64 = 14 * 24 * 60 * 60;

#[tauri::command(async)]
pub fn list_staged_bundles() -> Result<Vec<ReceivedBundleDto>, String> {
    let staging_root = bundle_staging_root()?;
    let import_root = bundle_import_root()?;
    list_staged_bundle_dtos_at(&staging_root, &import_root)
}

#[tauri::command(async)]
pub fn prune_staged_bundles() -> Result<Vec<String>, String> {
    let staging_root = bundle_staging_root()?;
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(STAGED_BUNDLE_RETENTION_SECS))
        .unwrap_or(UNIX_EPOCH);
    prune_staged_bundle_dtos_at(&staging_root, cutoff)
}

#[tauri::command(async)]
pub fn delete_staged_bundle(bundle_id: String) -> Result<bool, String> {
    let staging_root = bundle_staging_root()?;
    delete_staged_bundle_at(&staging_root, &bundle_id)
}

#[tauri::command(async)]
pub fn import_staged_bundle(
    request: ImportStagedBundleRequestDto,
) -> Result<ReceivedBundleDto, String> {
    let staging_root = bundle_staging_root()?;
    let import_root = bundle_import_root()?;
    let strategy = parse_import_conflict_strategy(request.conflict_strategy.as_deref())?;
    import_staged_bundle_with_strategy_at(&staging_root, &import_root, &request.bundle_id, strategy)
}

#[tauri::command(async)]
pub fn rollback_imported_bundle(
    request: RollbackImportedBundleRequestDto,
) -> Result<ReceivedBundleDto, String> {
    let import_root = bundle_import_root()?;
    rollback_imported_bundle_at(&import_root, &request.bundle_id)
}

#[tauri::command(async)]
pub fn create_manual_bundle(
    state: State<'_, AppState>,
    request: ManualBundleCreateRequestDto,
) -> Result<ManualBundleCreateDto, String> {
    let bundle_type = parse_bundle_type(&request.bundle_type)?;
    let display_name = request.display_name.trim().to_string();
    if display_name.is_empty() {
        return Err("资料包名称不能为空".to_string());
    }
    let source_app = request.source_app.trim().to_string();
    if source_app.is_empty() {
        return Err("来源应用不能为空".to_string());
    }
    let source_path = expand_home_dir(&request.source_path);
    let output_root = manual_bundle_output_root()?;
    fs::create_dir_all(&output_root)
        .map_err(|error| format!("无法创建资料包输出目录 {}: {error}", output_root.display()))?;

    let identity = state.device_identity.public_identity();
    let sender = BundleSender {
        device_id: identity.device_id,
        device_name: identity.device_name,
        fingerprint: identity.public_key_fingerprint,
    };
    let created = create_manual_bundle_directory(ManualBundleCreateRequest {
        source_path: source_path.clone(),
        output_root,
        bundle_id: manual_bundle_id(&display_name, &bundle_type, &source_path),
        bundle_type,
        display_name,
        source_app,
        sender,
        created_at: current_utc_timestamp(),
        permissions: Some(manual_bundle_permissions(&bundle_type)),
    })
    .map_err(|error| error.to_string())?;

    let manifest = &created.detected.manifest;
    Ok(ManualBundleCreateDto {
        bundle_id: manifest.bundle_id.clone(),
        bundle_type: bundle_type_label(manifest.bundle_type).to_string(),
        display_name: manifest.display_name.clone(),
        source_app: manifest.source_app.clone(),
        staging_path: created.staging_path.display().to_string(),
        file_count: manifest.summary.file_count,
        total_bytes: manifest.summary.total_bytes,
    })
}

pub(crate) fn received_root_name(report: &TransferReceiveReport) -> String {
    if !report.root_name.trim().is_empty() {
        return report.root_name.clone();
    }

    let Some(first_file) = report.files.first() else {
        return "接收文件".to_string();
    };
    let first_path = first_file.manifest_path.trim_matches('/');
    let Some((root, _)) = first_path.split_once('/') else {
        return first_path.to_string();
    };
    if root.trim().is_empty() {
        "接收文件".to_string()
    } else {
        root.to_string()
    }
}

pub(crate) fn bundle_staging_root() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("bundle_staging"))
}

pub(crate) fn bundle_import_root() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("bundle_imports"))
}

pub(crate) fn manual_bundle_output_root() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("manual_bundles"))
}

pub(crate) fn current_utc_timestamp() -> String {
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};

    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}
