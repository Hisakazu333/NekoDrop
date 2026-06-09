use tauri::Manager;

pub fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.set_title("NekoDrop")?;
    }
    Ok(())
}
