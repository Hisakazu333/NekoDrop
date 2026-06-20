use nekodrop_core::{FileManifest, ManifestItem, ManifestItemKind};
use nekodrop_service::{
    ReceivedBundleReport, TransferPlanScanProgress, TransferReceiveReport, TransferSecurityMode,
    TransferSendReport, TransferSourceFile, TransferSourcePlan,
};

use super::{
    bundle_type_label, ManifestItemDto, ReceiveReportDto, ReceivedBundleDto, ReceivedFileDto,
    SendReportDto, SentFileDto, TransferPlanDto, TransferScanProgressDto, TransferSourceFileDto,
};

pub(super) const RECEIVE_FILE_PREVIEW_LIMIT: usize = 20;

pub(super) fn source_plan_to_dto(plan: &TransferSourcePlan) -> TransferPlanDto {
    TransferPlanDto {
        root_name: plan.manifest.root_name.clone(),
        file_count: plan.file_count(),
        total_bytes: plan.total_bytes(),
        items: manifest_items_to_dto(&plan.manifest),
        files: plan.files.iter().map(source_file_to_dto).collect(),
    }
}

pub(super) fn transfer_scan_progress_to_dto(
    progress: TransferPlanScanProgress,
) -> TransferScanProgressDto {
    TransferScanProgressDto {
        phase: progress.phase.as_str().to_string(),
        current_path: progress.current_path,
        files_found: progress.files_found,
        directories_found: progress.directories_found,
        bytes_found: progress.bytes_found,
    }
}

fn manifest_items_to_dto(manifest: &FileManifest) -> Vec<ManifestItemDto> {
    manifest.items.iter().map(manifest_item_to_dto).collect()
}

fn manifest_item_to_dto(item: &ManifestItem) -> ManifestItemDto {
    ManifestItemDto {
        path: item.path.clone(),
        kind: match item.kind {
            ManifestItemKind::File => "file",
            ManifestItemKind::Directory => "directory",
        }
        .to_string(),
        size: item.size,
        modified_at: item.modified_at.clone(),
        sha256: item.sha256.clone(),
    }
}

fn source_file_to_dto(file: &TransferSourceFile) -> TransferSourceFileDto {
    TransferSourceFileDto {
        manifest_path: file.manifest_path.clone(),
        source_path: file.source_path.display().to_string(),
        size: file.size,
        sha256: file.sha256.clone(),
    }
}

pub(super) fn send_report_to_dto(report: &TransferSendReport) -> SendReportDto {
    SendReportDto {
        root_name: report.plan.manifest.root_name.clone(),
        file_count: report.plan.file_count(),
        total_bytes: report.plan.total_bytes(),
        sent_files: report
            .sent_files
            .iter()
            .map(|file| SentFileDto {
                manifest_path: file.manifest_path.clone(),
                bytes_sent: file.bytes_sent,
            })
            .collect(),
    }
}

pub(super) fn receive_report_to_dto(report: &TransferReceiveReport) -> ReceiveReportDto {
    ReceiveReportDto {
        transfer_id: report.transfer_id.clone(),
        root_name: report.root_name.clone(),
        security_mode: transfer_security_mode_label(report.security_mode).to_string(),
        sender_device_id: report.sender_device_id.clone(),
        sender_device_name: report.sender_device_name.clone(),
        sender_public_key_fingerprint: report.sender_public_key_fingerprint.clone(),
        file_count: report.files.len(),
        bundle: report.bundle.as_ref().map(received_bundle_to_dto),
        files: report
            .files
            .iter()
            .take(RECEIVE_FILE_PREVIEW_LIMIT)
            .map(|file| ReceivedFileDto {
                path: file.path.display().to_string(),
                manifest_path: file.manifest_path.clone(),
                bytes_written: file.bytes_written,
                sha256: file.sha256.clone(),
                verified: file.verified,
            })
            .collect(),
    }
}

fn received_bundle_to_dto(bundle: &ReceivedBundleReport) -> ReceivedBundleDto {
    ReceivedBundleDto {
        bundle_id: bundle.bundle_id.clone(),
        bundle_type: bundle_type_label(bundle.bundle_type).to_string(),
        display_name: bundle.display_name.clone(),
        source_app: bundle.source_app.clone(),
        file_count: bundle.file_count,
        total_bytes: bundle.total_bytes,
        staging_path: bundle.staging_path.display().to_string(),
        import_allowed: bundle.import_allowed,
        staging_status: "saved".to_string(),
        can_import_now: false,
        import_path: None,
        import_destination: None,
        import_conflict: false,
        import_blocking_reason: None,
    }
}

pub(super) fn transfer_security_mode_label(mode: TransferSecurityMode) -> &'static str {
    match mode {
        TransferSecurityMode::LegacyPlain => "legacy_plain",
        TransferSecurityMode::EncryptedSession => "encrypted_session",
        TransferSecurityMode::AuthenticatedEncryptedSession => "authenticated_encrypted_session",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use nekodrop_service::{ReceivedBundleReport, TransferPlanScanPhase};

    use super::*;

    #[test]
    fn transfer_scan_progress_dto_uses_stable_wire_labels() {
        let dto = transfer_scan_progress_to_dto(TransferPlanScanProgress {
            phase: TransferPlanScanPhase::Hashing,
            current_path: Some("drop/audio.m4a".to_string()),
            files_found: 2,
            directories_found: 1,
            bytes_found: 4096,
        });

        assert_eq!(dto.phase, "hashing");
        assert_eq!(dto.current_path.as_deref(), Some("drop/audio.m4a"));
        assert_eq!(dto.files_found, 2);
        assert_eq!(dto.directories_found, 1);
        assert_eq!(dto.bytes_found, 4096);
    }

    #[test]
    fn receive_report_dto_limits_file_preview_for_large_folders() {
        let report = TransferReceiveReport {
            transfer_id: "transfer-a".to_string(),
            root_name: "drop".to_string(),
            security_mode: TransferSecurityMode::LegacyPlain,
            sender_device_id: None,
            sender_device_name: None,
            sender_public_key_fingerprint: None,
            bundle: None,
            files: (0..100)
                .map(|index| nekodrop_storage::ReceivedFile {
                    path: PathBuf::from(format!("/tmp/drop/file-{index:03}.txt")),
                    manifest_path: format!("drop/file-{index:03}.txt"),
                    bytes_written: 1,
                    sha256: "a".repeat(64),
                    verified: true,
                })
                .collect(),
        };

        let dto = receive_report_to_dto(&report);

        assert_eq!(dto.security_mode, "legacy_plain");
        assert_eq!(dto.file_count, 100);
        assert_eq!(dto.files.len(), RECEIVE_FILE_PREVIEW_LIMIT);
        assert_eq!(dto.files[0].manifest_path, "drop/file-000.txt");
    }

    #[test]
    fn receive_report_dto_includes_bundle_preview() {
        let report = TransferReceiveReport {
            transfer_id: "transfer-a".to_string(),
            root_name: "bundle".to_string(),
            security_mode: TransferSecurityMode::AuthenticatedEncryptedSession,
            sender_device_id: None,
            sender_device_name: None,
            sender_public_key_fingerprint: None,
            bundle: Some(ReceivedBundleReport {
                bundle_id: "bundle_1234567890".to_string(),
                bundle_type: nekolink_protocol::BundleType::Skill,
                display_name: "voice_transcribe".to_string(),
                source_app: "Generic Agent App".to_string(),
                file_count: 2,
                total_bytes: 28,
                staging_path: PathBuf::from("/tmp/bundle_1234567890"),
                import_allowed: true,
            }),
            files: Vec::new(),
        };

        let dto = receive_report_to_dto(&report);
        let bundle = dto.bundle.expect("bundle preview should be exposed");

        assert_eq!(bundle.bundle_id, "bundle_1234567890");
        assert_eq!(bundle.bundle_type, "skill");
        assert_eq!(bundle.display_name, "voice_transcribe");
        assert_eq!(bundle.source_app, "Generic Agent App");
        assert_eq!(bundle.file_count, 2);
        assert_eq!(bundle.total_bytes, 28);
        assert_eq!(bundle.staging_path, "/tmp/bundle_1234567890");
        assert!(bundle.import_allowed);
    }
}
