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
            commands::create_transfer_plan,
            commands::create_transfer_plan_from_text,
            commands::send_paths_to_code,
            commands::select_send_files,
            commands::select_send_folders,
            commands::select_receive_dir,
            commands::open_path,
            commands::start_receive_once,
            commands::stop_receive_once,
            commands::get_receive_status,
            commands::get_receive_session,
            commands::get_last_receive_report,
            commands::get_pending_receive_offer,
            commands::respond_receive_offer,
            commands::get_transfer_status
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
