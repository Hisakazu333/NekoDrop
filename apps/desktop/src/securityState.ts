import type { ReceiveReportDto, TransferSecurityMode } from "./types";

export type TransferSecurityTone = "trusted" | "encrypted" | "warning";

export interface TransferSecurityViewModel {
  label: string;
  detail: string;
  tone: TransferSecurityTone;
}

export function buildTransferSecurityViewModel(
  mode: TransferSecurityMode | string | null | undefined
): TransferSecurityViewModel | null {
  switch (mode) {
    case "authenticated_encrypted_session":
      return {
        label: "已认证加密",
        detail: "双方身份已验签，文件流已加密",
        tone: "trusted"
      };
    case "encrypted_session":
      return {
        label: "已加密",
        detail: "文件流已加密，未绑定可信设备公钥",
        tone: "encrypted"
      };
    case "legacy_plain":
      return {
        label: "兼容明文",
        detail: "仅手动确认，不会刷新可信设备",
        tone: "warning"
      };
    default:
      return null;
  }
}

export function receiveSecuritySummaryLine(report: ReceiveReportDto) {
  const model = buildTransferSecurityViewModel(report.security_mode);
  if (!model) return null;

  const parts = [model.label];
  if (report.sender_device_name?.trim()) {
    parts.push(report.sender_device_name.trim());
  }
  if (report.sender_public_key_fingerprint?.trim()) {
    parts.push(report.sender_public_key_fingerprint.trim());
  }

  return parts.join(" · ");
}
