import { getCurrentWebview } from "@tauri-apps/api/webview";

import { isTauriRuntime } from "./tauri";

export type DragDropHandlers = {
  onActiveChange: (active: boolean) => void;
  onDrop: (paths: string[]) => void;
  onError: (message: string) => void;
};

export async function bindWindowDragDrop(handlers: DragDropHandlers) {
  if (!isTauriRuntime()) {
    return () => undefined;
  }

  try {
    return await getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "enter" || event.payload.type === "over") {
        handlers.onActiveChange(true);
        return;
      }

      if (event.payload.type === "leave") {
        handlers.onActiveChange(false);
        return;
      }

      if (event.payload.type === "drop") {
        handlers.onActiveChange(false);
        if (event.payload.paths.length > 0) {
          handlers.onDrop(event.payload.paths);
        }
      }
    });
  } catch (error) {
    handlers.onError(error instanceof Error ? error.message : String(error));
    return () => undefined;
  }
}
