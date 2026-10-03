mod app_config;
mod app_state;
mod commands;
mod device_identity;
mod discovery;
mod local_bridge_authorizations;
mod local_bridge_runtime;
mod network;
mod transfer_history;
mod tray;
mod trusted_devices;

use app_state::AppState;
use tauri::Manager;

pub fn run() {
    let app_state = AppState::new().expect("failed to initialize NekoDrop app state");
    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_app_snapshot,
            commands::get_desktop_realtime_snapshot,
            commands::get_discovery_status,
            commands::list_nearby_devices,
            commands::list_trusted_devices,
            commands::trust_nearby_device,
            commands::request_device_pairing,
            commands::forget_trusted_device,
            commands::list_transfers,
            commands::create_transfer_plan,
            commands::create_transfer_plan_from_text,
            commands::stage_text_snippet,
            commands::read_received_text,
            commands::set_trusted_device_alias,
            commands::send_paths_to_code,
            commands::send_paths_to_device,
            commands::resend_transfer,
            commands::open_transfer_location,
            commands::select_send_files,
            commands::select_send_folders,
            commands::select_manual_bundle_source_dir,
            commands::select_receive_dir,
            commands::create_manual_bundle,
            commands::set_receive_dir,
            commands::set_receive_port,
            commands::set_receive_policy,
            commands::set_device_name,
            commands::open_path,
            commands::start_receive_once,
            commands::stop_receive_once,
            commands::cancel_current_transfer,
            commands::get_receive_status,
            commands::get_receive_session,
            commands::get_receive_port_diagnostics,
            commands::get_last_receive_report,
            commands::get_pending_receive_offer,
            commands::get_pending_pairing_request,
            commands::respond_receive_offer,
            commands::respond_pairing_request,
            commands::get_transfer_status,
            commands::delete_transfer,
            commands::clear_transfer_history,
            commands::list_staged_bundles,
            commands::prune_staged_bundles,
            commands::delete_staged_bundle,
            commands::import_staged_bundle,
            commands::rollback_imported_bundle,
            commands::get_local_bridge_runtime_status,
            commands::list_local_bridge_authorizations,
            commands::revoke_local_bridge_authorization,
            commands::list_local_bridge_pending_actions,
            commands::remove_local_bridge_pending_action,
            commands::respond_local_bridge_pending_action,
            commands::list_local_bridge_pending_action_results,
            commands::take_next_local_bridge_pending_action,
            commands::preflight_next_local_bridge_bundle_send,
            commands::execute_next_local_bridge_bundle_send,
            commands::execute_next_local_bridge_bundle_import,
            commands::run_local_bridge_runtime_worker_once,
            commands::prune_local_bridge_authorizations,
            commands::handle_local_bridge_request,
            commands::confirm_local_bridge_authorization
        ])
        .setup(|app| {
            apply_window_glass(app);
            tray::setup_tray(app)?;
            let state = app.state::<AppState>();
            discovery::start_discovery(&state);
            local_bridge_runtime::start_local_bridge_runtime(&state);
            commands::start_local_bridge_runtime_worker(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running NekoDrop desktop app");
}

/// 让窗口本身成为磨砂玻璃（macOS NSVisualEffectView / Windows Mica）。
/// 失败时通知前端回退到实色底，避免透明窗口露出异常底色。
fn apply_window_glass(app: &tauri::App) {
    let window = match app.get_webview_window("main") {
        Some(window) => window,
        None => return,
    };
    let applied = {
        #[cfg(target_os = "macos")]
        {
            window_vibrancy::apply_vibrancy(
                &window,
                window_vibrancy::NSVisualEffectMaterial::UnderWindowBackground,
                None,
                None,
            )
            .is_ok()
        }
        #[cfg(target_os = "windows")]
        {
            window_vibrancy::apply_mica(&window, None).is_ok()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            false
        }
    };
    if !applied {
        let _ = window.eval("document.documentElement.dataset.vibrancy = 'off'");
    }
}

fn main() {
    run();
}
