import { invoke } from "@tauri-apps/api/core";

type CommandName =
  | "get_app_snapshot"
  | "list_nearby_devices"
  | "list_transfers";

export async function invokeCommand<T>(command: CommandName): Promise<T> {
  if (!("__TAURI_INTERNALS__" in window)) {
    throw new Error("NekoDrop 必须在 Tauri 桌面端中运行，不能用浏览器预览代替桌面软件。");
  }
  return invoke<T>(command);
}
