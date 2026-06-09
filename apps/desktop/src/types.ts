export type PageId = "home" | "devices" | "transfers" | "settings";

export interface AppSnapshot {
  device_name: string;
  receive_dir: string;
  discovery_enabled: boolean;
  tray_enabled: boolean;
}

export interface DeviceDto {
  id: string;
  name: string;
  platform: string;
  host: string;
  port: number;
  trust_state: string;
}

export interface TransferDto {
  id: string;
  peer_device_id: string;
  direction: string;
  status: string;
  file_count: number;
  total_bytes: number;
  transferred_bytes: number;
  progress: number;
}

