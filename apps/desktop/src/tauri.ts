import { invoke } from "@tauri-apps/api/core";

type CommandName =
  | "get_app_snapshot"
  | "get_desktop_realtime_snapshot"
  | "get_discovery_status"
  | "list_nearby_devices"
  | "list_trusted_devices"
  | "trust_nearby_device"
  | "request_device_pairing"
  | "forget_trusted_device"
  | "list_transfers"
  | "create_transfer_plan"
  | "create_transfer_plan_from_text"
  | "send_paths_to_code"
  | "send_paths_to_device"
  | "resend_transfer"
  | "open_transfer_location"
  | "select_send_files"
  | "select_send_folders"
  | "select_manual_bundle_source_dir"
  | "create_manual_bundle"
  | "select_receive_dir"
  | "set_receive_dir"
  | "set_receive_port"
  | "set_receive_policy"
  | "set_device_name"
  | "open_path"
  | "start_receive_once"
  | "stop_receive_once"
  | "cancel_current_transfer"
  | "get_receive_status"
  | "get_receive_session"
  | "get_receive_port_diagnostics"
  | "get_last_receive_report"
  | "get_pending_receive_offer"
  | "get_pending_pairing_request"
  | "respond_receive_offer"
  | "respond_pairing_request"
  | "get_transfer_status"
  | "delete_transfer"
  | "clear_transfer_history"
  | "list_staged_bundles"
  | "prune_staged_bundles"
  | "delete_staged_bundle"
  | "import_staged_bundle"
  | "handle_local_bridge_request"
  | "confirm_local_bridge_authorization";

export async function invokeCommand<T>(
  command: CommandName,
  args?: Record<string, unknown>
): Promise<T> {
  if (!isTauriRuntime()) {
    throw new Error("NekoDrop 必须在 Tauri 桌面端中运行，不能用浏览器预览代替桌面软件。");
  }
  return invoke<T>(command, args);
}

export function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}
