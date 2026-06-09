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

export interface ManifestItemDto {
  path: string;
  kind: "file" | "directory";
  size: number;
  modified_at: string | null;
  sha256: string | null;
}

export interface TransferSourceFileDto {
  manifest_path: string;
  source_path: string;
  size: number;
  sha256: string;
}

export interface TransferPlanDto {
  root_name: string;
  file_count: number;
  total_bytes: number;
  items: ManifestItemDto[];
  files: TransferSourceFileDto[];
}
