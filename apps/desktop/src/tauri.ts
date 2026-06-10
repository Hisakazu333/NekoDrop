import { invoke } from "@tauri-apps/api/core";

type CommandName =
  | "get_app_snapshot"
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
  | "select_receive_dir"
  | "open_path"
  | "start_receive_once"
  | "stop_receive_once"
  | "get_receive_status"
  | "get_receive_session"
  | "get_last_receive_report"
  | "get_pending_receive_offer"
  | "get_pending_pairing_request"
  | "respond_receive_offer"
  | "respond_pairing_request"
  | "get_transfer_status"
  | "delete_transfer"
  | "clear_transfer_history";

export async function invokeCommand<T>(
  command: CommandName,
  args?: Record<string, unknown>
): Promise<T> {
  if (!("__TAURI_INTERNALS__" in window)) {
    throw new Error("NekoDrop 必须在 Tauri 桌面端中运行，不能用浏览器预览代替桌面软件。");
  }
  return invoke<T>(command, args);
}
