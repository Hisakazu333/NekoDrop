use nekodrop_core::{FileManifest, ManifestItem, ManifestItemKind};
use nekodrop_network::TransferOffer;
use nekodrop_service::{
    ReceivedBundleReport, TransferPlanScanProgress, TransferReceiveReport, TransferSecurityMode,
    TransferSendReport, TransferSourceFile, TransferSourcePlan,
};
use nekodrop_storage::{build_resume_plan_for_files, ResumeExpectedFile, ResumePlan};

use crate::app_state::{
    PendingPairingRequest, PendingReceiveOffer, PendingReceiveResumeSummary, TransferStatusState,
};
use crate::transfer_history::TransferHistoryRecord;

use super::{
    bundle_type_label, ManifestItemDto, PendingPairingRequestDto, PendingReceiveFileDto,
    PendingReceiveOfferDto, ReceiveReportDto, ReceiveResumeSummaryDto, ReceivedBundleDto,
    ReceivedFileDto, SendReportDto, SentFileDto, TransferDto, TransferPlanDto,
    TransferScanProgressDto, TransferSourceFileDto, TransferStatusDto,
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
        import_plan_files: Vec::new(),
        import_conflict_count: 0,
        import_conflict_strategies: Vec::new(),
        imported_with_strategy: None,
        import_skipped_file_count: 0,
        import_receipt_path: None,
        imported_manifest_paths: Vec::new(),
        skipped_manifest_paths: Vec::new(),
        rollback_file_count: 0,
        can_rollback_now: false,
        rollback_blocking_reason: None,
    }
}

pub(super) fn pending_offer_to_dto(offer: &PendingReceiveOffer) -> PendingReceiveOfferDto {
    PendingReceiveOfferDto {
        transfer_id: offer.transfer_id.clone(),
        root_name: offer.root_name.clone(),
        file_count: offer.file_count,
        total_bytes: offer.total_bytes,
        sender_device_id: offer.sender_device_id.clone(),
        sender_device_name: offer.sender_device_name.clone(),
        sender_public_key_fingerprint: offer.sender_public_key_fingerprint.clone(),
        preview_file_count: offer.files.len().min(RECEIVE_FILE_PREVIEW_LIMIT),
        files: offer
            .files
            .iter()
            .take(RECEIVE_FILE_PREVIEW_LIMIT)
            .map(|file| PendingReceiveFileDto {
                manifest_path: file.manifest_path.clone(),
                size: file.size,
                sha256: file.sha256.clone(),
            })
            .collect(),
        resume_summary: offer.resume_summary.map(|summary| ReceiveResumeSummaryDto {
            resumable_file_count: summary.resumable_file_count,
            completed_file_count: summary.completed_file_count,
            partial_file_count: summary.partial_file_count,
            received_bytes: summary.received_bytes,
        }),
    }
}

pub(super) fn pending_resume_summary_from_offer(
    receive_dir: &std::path::Path,
    offer: &TransferOffer,
) -> Option<PendingReceiveResumeSummary> {
    let mut expected_files = Vec::with_capacity(offer.files.len());
    for file in &offer.files {
        expected_files.push(
            ResumeExpectedFile::new(
                file.manifest_path.clone(),
                file.size,
                Some(file.sha256.clone()),
            )
            .ok()?,
        );
    }

    let plan =
        build_resume_plan_for_files(receive_dir, &offer.transfer_id, &expected_files).ok()?;
    pending_resume_summary_from_plan(&plan)
}

pub(super) fn pending_resume_summary_from_plan(
    plan: &ResumePlan,
) -> Option<PendingReceiveResumeSummary> {
    if plan.is_empty() {
        return None;
    }

    Some(PendingReceiveResumeSummary {
        resumable_file_count: plan.files.len(),
        completed_file_count: plan.completed_file_count(),
        partial_file_count: plan.partial_file_count(),
        received_bytes: plan.total_received_bytes(),
    })
}

pub(super) fn pending_pairing_request_to_dto(
    request: &PendingPairingRequest,
) -> PendingPairingRequestDto {
    PendingPairingRequestDto {
        request_id: request.request_id.clone(),
        device_id: request.device_id.clone(),
        device_name: request.device_name.clone(),
        platform: request.platform.clone(),
        host: request.host.clone(),
        port: request.port,
        public_key: request.public_key.clone(),
        public_key_fingerprint: request.public_key_fingerprint.clone(),
        pairing_code: request.pairing_code.clone(),
    }
}

pub(super) fn transfer_status_to_dto(status: &TransferStatusState) -> TransferStatusDto {
    let progress = if status.total_bytes == 0 {
        0.0
    } else {
        (status.bytes_transferred as f32 / status.total_bytes as f32).clamp(0.0, 1.0)
    };
    TransferStatusDto {
        direction: status.direction.clone(),
        phase: status.phase.clone(),
        root_name: status.root_name.clone(),
        file_count: status.file_count,
        file_index: status.file_index,
        current_file: status.current_file.clone(),
        bytes_transferred: status.bytes_transferred,
        total_bytes: status.total_bytes,
        progress,
        message: status.message.clone(),
        updated_at_ms: status.updated_at_ms,
    }
}

pub(super) fn transfer_to_dto(record: &TransferHistoryRecord) -> TransferDto {
    let progress = if record.total_bytes == 0 {
        0.0
    } else {
        (record.transferred_bytes as f32 / record.total_bytes as f32).clamp(0.0, 1.0)
    };
    TransferDto {
        id: record.id.clone(),
        root_name: record.root_name.clone(),
        peer_device_id: record.peer_device_id.clone(),
        peer_name: record.peer_name.clone(),
        target_host: record.target_host.clone(),
        source_paths: record.source_paths.clone(),
        received_paths: record.received_paths.clone(),
        direction: record.direction.clone(),
        status: record.status.clone(),
        file_count: record.file_count,
        total_bytes: record.total_bytes,
        transferred_bytes: record.transferred_bytes,
        progress,
        receive_dir: record.receive_dir.clone(),
        error_message: record.error_message.clone(),
        security_mode: record.security_mode.clone(),
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
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
