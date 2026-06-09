mod app_state;
mod commands;
mod tray;

use app_state::AppState;

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_snapshot,
            commands::list_nearby_devices,
            commands::list_transfers,
            commands::create_transfer_plan
        ])
        .setup(|app| {
            tray::setup_tray(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running NekoDrop desktop app");
}

fn main() {
    run();
}
