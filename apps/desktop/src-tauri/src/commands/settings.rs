use super::*;

#[tauri::command(async)]
pub fn select_send_files() -> Result<Vec<String>, String> {
    choose_paths(PathDialogKind::Files)
}

#[tauri::command(async)]
pub fn select_send_folders() -> Result<Vec<String>, String> {
    choose_paths(PathDialogKind::Folders)
}

#[tauri::command(async)]
pub fn select_manual_bundle_source_dir() -> Result<Option<String>, String> {
    Ok(choose_paths(PathDialogKind::BundleSourceFolder)?
        .into_iter()
        .next())
}

#[tauri::command(async)]
pub fn select_receive_dir() -> Result<Option<String>, String> {
    Ok(choose_paths(PathDialogKind::SingleFolder)?
        .into_iter()
        .next())
}

#[tauri::command(async)]
pub fn set_receive_dir(state: State<'_, AppState>, receive_dir: String) -> Result<(), String> {
    persist_receive_dir(&state, &receive_dir)
}

#[tauri::command(async)]
pub fn set_receive_port(state: State<'_, AppState>, receive_port: u16) -> Result<(), String> {
    persist_receive_port(&state, receive_port)
}

#[tauri::command(async)]
pub fn set_receive_policy(
    state: State<'_, AppState>,
    receive_policy: String,
) -> Result<(), String> {
    let receive_policy = receive_policy_from_input(&receive_policy)?;
    persist_receive_policy(&state, receive_policy)
}

#[tauri::command(async)]
pub fn set_device_name(state: State<'_, AppState>, device_name: String) -> Result<String, String> {
    persist_device_name(&state, &device_name)
}

#[tauri::command(async)]
pub fn open_path(path: String) -> Result<(), String> {
    let target = expand_home_dir(path.trim());
    if !target.exists() {
        return Err(format!("路径不存在：{}", target.display()));
    }

    open_path_with_system(target)
}

pub(crate) fn persist_receive_dir(state: &AppState, receive_dir: &str) -> Result<(), String> {
    if receive_dir.trim().is_empty() {
        return Err("接收目录不能为空".to_string());
    }
    let receive_dir_path = expand_home_dir(receive_dir);
    fs::create_dir_all(&receive_dir_path)
        .map_err(|error| format!("无法创建接收目录 {}: {error}", receive_dir_path.display()))?;
    persist_receive_dir_path(state, &receive_dir_path)
}

pub(crate) fn persist_receive_dir_path(
    state: &AppState,
    receive_dir_path: &PathBuf,
) -> Result<(), String> {
    let receive_dir = receive_dir_path.display().to_string();
    let mut config = state.config.lock().map_err(|error| error.to_string())?;
    if config.receive_dir == receive_dir {
        return Ok(());
    }
    let mut next_config = config.clone();
    next_config.receive_dir = receive_dir;
    save_app_config(&next_config)?;
    *config = next_config;
    Ok(())
}

pub(crate) fn persist_receive_policy(
    state: &AppState,
    receive_policy: ReceivePolicy,
) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|error| error.to_string())?;
    if config.receive_policy == receive_policy {
        return Ok(());
    }

    let mut next_config = config.clone();
    next_config.receive_policy = receive_policy;
    save_app_config(&next_config)?;
    *config = next_config;
    Ok(())
}

pub(crate) fn persist_receive_port(state: &AppState, receive_port: u16) -> Result<(), String> {
    if receive_port == 0 {
        return Err("端口必须是 1-65535".to_string());
    }

    let mut config = state.config.lock().map_err(|error| error.to_string())?;
    if config.receive_port == receive_port {
        return Ok(());
    }

    let mut next_config = config.clone();
    next_config.receive_port = receive_port;
    save_app_config(&next_config)?;
    *config = next_config;
    Ok(())
}

pub(crate) fn persist_device_name(state: &AppState, device_name: &str) -> Result<String, String> {
    let device_name = state.device_identity.save_device_name(device_name)?;
    let mut config = state.config.lock().map_err(|error| error.to_string())?;
    if config.device_name == device_name {
        return Ok(device_name);
    }

    config.device_name = device_name.clone();
    Ok(device_name)
}

pub(crate) fn receive_policy_from_input(value: &str) -> Result<ReceivePolicy, String> {
    match value {
        "always_ask" => Ok(ReceivePolicy::AlwaysAsk),
        "auto_accept_trusted" => Ok(ReceivePolicy::AutoAcceptTrusted),
        "block_all" => Ok(ReceivePolicy::BlockAll),
        _ => Err("未知接收策略".to_string()),
    }
}
