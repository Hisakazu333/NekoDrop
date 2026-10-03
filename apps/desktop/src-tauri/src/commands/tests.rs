use super::staged_bundles::import_staged_bundle_at;
use super::*;
use nekodrop_core::Device;
use nekolink_protocol::SignedSessionIdentityBinding;
use std::net::IpAddr;

use crate::commands::transfer_dtos::RECEIVE_FILE_PREVIEW_LIMIT;
use nekolink_protocol::{
    BundleChecksums, BundleCompatibility, BundleFile, BundleManifest, BundlePermissionScope,
    BundlePermissions, BundleSecretsPolicy, BundleSender, BundleSummary, BundleType,
    BundleWriteMode, BundleWritePermission, Capability, BUNDLE_CHECKSUM_SHA256, BUNDLE_SCHEMA_V1,
    PROTOCOL_VERSION,
};
use std::collections::BTreeMap;

#[test]
fn desktop_endpoint_preflight_rejects_unusable_addresses() {
    assert!(validate_endpoint_for_desktop_send(&Endpoint::tcp("192.168.1.20", 45821)).is_ok());

    let loopback =
        validate_endpoint_for_desktop_send(&Endpoint::tcp("127.0.0.1", 45821)).unwrap_err();
    assert!(loopback.contains("指向了本机"));

    let unspecified =
        validate_endpoint_for_desktop_send(&Endpoint::tcp("0.0.0.0", 45821)).unwrap_err();
    assert!(unspecified.contains("监听地址"));

    let benchmark =
        validate_endpoint_for_desktop_send(&Endpoint::tcp("198.18.0.1", 45821)).unwrap_err();
    assert!(benchmark.contains("198.18/198.19"));

    let link_local =
        validate_endpoint_for_desktop_send(&Endpoint::tcp("169.254.0.2", 45821)).unwrap_err();
    assert!(link_local.contains("169.254"));
}

#[test]
fn current_lan_ip_is_treated_as_self_target() {
    let current = vec![IpAddr::from([10, 0, 0, 8]), IpAddr::from([192, 168, 1, 20])];

    assert!(is_current_lan_ip(IpAddr::from([192, 168, 1, 20]), &current));
    assert!(!is_current_lan_ip(
        IpAddr::from([192, 168, 1, 30]),
        &current
    ));
}

#[test]
fn connection_input_accepts_endpoint_label_as_manual_fallback() {
    let (endpoint, peer) = endpoint_and_peer_from_connection_input("192.168.1.20:45821").unwrap();

    assert_eq!(endpoint, Endpoint::tcp("192.168.1.20", 45821));
    assert_eq!(peer.target_host.as_deref(), Some("192.168.1.20:45821"));
    assert!(peer.device_id.is_none());
}

#[test]
fn connection_input_keeps_connection_ticket_identity() {
    let code = ConnectionTicket::new(Endpoint::tcp("192.168.1.20", 45821))
        .unwrap()
        .with_device_id("device-a")
        .with_device_name("MacBook")
        .with_fingerprint("sha256:abc")
        .to_code()
        .unwrap();
    let (endpoint, peer) = endpoint_and_peer_from_connection_input(&code).unwrap();

    assert_eq!(endpoint, Endpoint::tcp("192.168.1.20", 45821));
    assert_eq!(peer.device_id.as_deref(), Some("device-a"));
    assert_eq!(peer.name.as_deref(), Some("MacBook"));
    assert_eq!(peer.fingerprint.as_deref(), Some("sha256:abc"));
}

#[test]
fn nearby_device_requires_trusted_identity_before_send() {
    let device = nearby_device("device-a", "sha256:device-a");

    let result = trusted_peer_from_nearby_device(&device, &[]);

    assert!(result.unwrap_err().contains("可信配对"));
}

#[test]
fn nearby_device_uses_current_endpoint_after_trust_match() {
    let device = nearby_device("device-a", "sha256:device-a");
    let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];

    let (endpoint, peer) = trusted_peer_from_nearby_device(&device, &trusted).unwrap();

    assert_eq!(endpoint, Endpoint::tcp("192.168.1.20", 45821));
    assert_eq!(peer.device_id.as_deref(), Some("device-a"));
    assert_eq!(peer.fingerprint.as_deref(), Some("sha256:device-a"));
}

#[test]
fn nearby_device_send_pins_saved_trusted_public_key() {
    let public_key = test_public_key("device-a");
    let mut device = nearby_device("device-a", public_key.fingerprint.as_str());
    device.public_key = Some(public_key.public_key.clone());
    let trusted = vec![trusted_record_with_public_key(
        "device-a",
        "MacBook",
        public_key.public_key.as_str(),
        public_key.fingerprint.as_str(),
    )];

    let (_endpoint, peer) = trusted_peer_from_nearby_device(&device, &trusted).unwrap();

    assert_eq!(
        peer.trusted_public_key.as_deref(),
        Some(public_key.public_key.as_str())
    );
    assert_eq!(
        peer.trusted_public_key_fingerprint.as_deref(),
        Some(public_key.fingerprint.as_str())
    );
}

#[test]
fn trusted_session_pin_accepts_matching_signed_public_key() {
    let key = test_identity_signing_key("device-a");
    let identity = test_identity_with_signing_key("device-a", &key);
    let binding = nekolink_protocol::SessionIdentityBinding::new(
        nekolink_protocol::SessionParticipantRole::Initiator,
        "session-trusted-pin",
        &identity,
        "x25519:session-key",
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    )
    .unwrap();
    let signed = SignedSessionIdentityBinding::sign(binding, &key).unwrap();
    let trusted = vec![trusted_record_with_public_key(
        "device-a",
        "MacBook",
        signed.public_key.as_str(),
        signed.public_key_fingerprint.as_str(),
    )];

    let result = verify_incoming_peer_against_trusted_devices(&trusted, &identity, &signed);

    assert!(result.is_ok());
}

#[test]
fn trusted_session_pin_rejects_public_key_rotation_for_same_device() {
    let key = test_identity_signing_key("device-a");
    let rotated_key = test_identity_signing_key("device-a-rotated");
    let identity = test_identity_with_signing_key("device-a", &key);
    let binding = nekolink_protocol::SessionIdentityBinding::new(
        nekolink_protocol::SessionParticipantRole::Initiator,
        "session-trusted-pin-rotated",
        &identity,
        "x25519:session-key",
        "sha256:2222222222222222222222222222222222222222222222222222222222222222",
    )
    .unwrap();
    let signed = SignedSessionIdentityBinding::sign(binding, &key).unwrap();
    let rotated_public_key = rotated_key.public_key();
    let trusted = vec![trusted_record_with_public_key(
        "device-a",
        "MacBook",
        rotated_public_key.public_key.as_str(),
        rotated_public_key.fingerprint.as_str(),
    )];

    let error =
        verify_incoming_peer_against_trusted_devices(&trusted, &identity, &signed).unwrap_err();

    assert!(error.contains("可信设备身份校验失败"));
}

#[test]
fn trusted_session_pin_rejects_binding_identity_mismatch() {
    let key = test_identity_signing_key("device-a");
    let identity = test_identity_with_signing_key("device-a", &key);
    let mismatched_identity = test_identity_with_signing_key("device-b", &key);
    let binding = nekolink_protocol::SessionIdentityBinding::new(
        nekolink_protocol::SessionParticipantRole::Initiator,
        "session-trusted-pin-mismatch",
        &mismatched_identity,
        "x25519:session-key",
        "sha256:6666666666666666666666666666666666666666666666666666666666666666",
    )
    .unwrap();
    let signed = SignedSessionIdentityBinding::sign(binding, &key).unwrap();
    let trusted = vec![trusted_record_with_public_key(
        "device-a",
        "MacBook",
        signed.public_key.as_str(),
        signed.public_key_fingerprint.as_str(),
    )];

    let error =
        verify_incoming_peer_against_trusted_devices(&trusted, &identity, &signed).unwrap_err();

    assert!(error.contains("binding"));
}

#[test]
fn untrusted_authenticated_session_is_not_pinned_to_trusted_devices() {
    let key = test_identity_signing_key("device-b");
    let identity = test_identity_with_signing_key("device-b", &key);
    let binding = nekolink_protocol::SessionIdentityBinding::new(
        nekolink_protocol::SessionParticipantRole::Initiator,
        "session-untrusted",
        &identity,
        "x25519:session-key",
        "sha256:3333333333333333333333333333333333333333333333333333333333333333",
    )
    .unwrap();
    let signed = SignedSessionIdentityBinding::sign(binding, &key).unwrap();
    let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];

    let result = verify_incoming_peer_against_trusted_devices(&trusted, &identity, &signed);

    assert!(result.is_ok());
}

#[test]
fn connection_ticket_peer_rejects_session_fingerprint_mismatch() {
    let key = test_identity_signing_key("device-a");
    let identity = test_identity_with_signing_key("device-a", &key);
    let binding = nekolink_protocol::SessionIdentityBinding::new(
        nekolink_protocol::SessionParticipantRole::Responder,
        "session-ticket-peer",
        &identity,
        "x25519:session-key",
        "sha256:4444444444444444444444444444444444444444444444444444444444444444",
    )
    .unwrap();
    let signed = SignedSessionIdentityBinding::sign(binding, &key).unwrap();
    let peer = TransferPeer {
        device_id: Some("device-a".to_string()),
        name: Some("MacBook".to_string()),
        fingerprint: Some("sha256:different".to_string()),
        trusted_public_key: None,
        trusted_public_key_fingerprint: None,
        target_host: Some("192.168.1.20:45821".to_string()),
    };

    let error = verify_peer_matches_transfer_peer(&peer, &identity, &signed).unwrap_err();

    assert!(error.contains("指纹不匹配"));
}

#[test]
fn manual_endpoint_peer_without_identity_allows_authenticated_session() {
    let key = test_identity_signing_key("device-a");
    let identity = test_identity_with_signing_key("device-a", &key);
    let binding = nekolink_protocol::SessionIdentityBinding::new(
        nekolink_protocol::SessionParticipantRole::Responder,
        "session-manual-peer",
        &identity,
        "x25519:session-key",
        "sha256:5555555555555555555555555555555555555555555555555555555555555555",
    )
    .unwrap();
    let signed = SignedSessionIdentityBinding::sign(binding, &key).unwrap();
    let peer = TransferPeer {
        device_id: None,
        name: None,
        fingerprint: None,
        trusted_public_key: None,
        trusted_public_key_fingerprint: None,
        target_host: Some("192.168.1.20:45821".to_string()),
    };

    let result = verify_peer_matches_transfer_peer(&peer, &identity, &signed);

    assert!(result.is_ok());
}

#[test]
fn self_peer_is_rejected_by_device_id() {
    let identity = test_identity("device-a");
    let peer = TransferPeer {
        device_id: Some("device-a".to_string()),
        name: Some("This Mac".to_string()),
        fingerprint: Some("sha256:self".to_string()),
        trusted_public_key: None,
        trusted_public_key_fingerprint: None,
        target_host: Some("192.168.1.20:45821".to_string()),
    };

    let error = reject_self_peer(&identity, &peer).unwrap_err();

    assert!(error.contains("本机"));
}

#[test]
fn manual_endpoint_without_identity_is_not_self_rejected() {
    let identity = test_identity("device-a");
    let peer = TransferPeer {
        device_id: None,
        name: None,
        fingerprint: None,
        trusted_public_key: None,
        trusted_public_key_fingerprint: None,
        target_host: Some("192.168.1.30:45821".to_string()),
    };

    assert!(reject_self_peer(&identity, &peer).is_ok());
}

#[test]
fn history_retry_reuses_existing_transfer_id() {
    assert_eq!(
        history_transfer_id(42, Some("send-original")),
        "send-original"
    );
    assert_eq!(history_transfer_id(42, None), "send-42");
}

#[test]
fn send_auto_retry_retries_once_for_transient_network_error() {
    let mut attempts = 0;
    let mut retry_events = Vec::new();

    let result = send_with_auto_retry(
        || {
            attempts += 1;
            if attempts == 1 {
                Err("failed to connect to 192.168.1.20:45821: Connection refused".to_string())
            } else {
                Ok("sent")
            }
        },
        |retry_number, retry_limit, error| {
            retry_events.push((retry_number, retry_limit, error.to_string()));
        },
    );

    assert_eq!(result.unwrap(), "sent");
    assert_eq!(attempts, 2);
    assert_eq!(retry_events.len(), 1);
    assert_eq!(retry_events[0].0, 1);
    assert_eq!(retry_events[0].1, 1);
    assert!(retry_events[0].2.contains("Connection refused"));
}

#[test]
fn send_auto_retry_does_not_retry_terminal_failures() {
    for error in [
        "transfer cancelled",
        "receiver declined transfer: no reason provided",
        "incoming file does not match accepted offer",
    ] {
        let mut attempts = 0;

        let result = send_with_auto_retry(
            || {
                attempts += 1;
                Err::<(), _>(error.to_string())
            },
            |_, _, _| panic!("terminal send failures must not be retried"),
        );

        assert_eq!(result.unwrap_err(), error);
        assert_eq!(attempts, 1);
    }
}

#[test]
fn send_auto_retry_stops_after_retry_limit() {
    let mut attempts = 0;

    let result = send_with_auto_retry(
        || {
            attempts += 1;
            Err::<(), _>("connection reset by peer".to_string())
        },
        |_, _, _| {},
    );

    assert_eq!(result.unwrap_err(), "connection reset by peer");
    assert_eq!(attempts, 2);
}

#[test]
fn current_utc_timestamp_uses_utc_iso_8601_shape() {
    let timestamp = current_utc_timestamp();

    assert!(timestamp.contains('T'));
    assert!(timestamp.ends_with('Z'));
}

#[test]
fn pending_receive_offer_dto_includes_resume_summary() {
    let offer = PendingReceiveOffer {
        transfer_id: "transfer-a".to_string(),
        root_name: "drop".to_string(),
        file_count: 2,
        total_bytes: 4096,
        sender_device_id: None,
        sender_device_name: None,
        sender_public_key_fingerprint: None,
        files: Vec::new(),
        resume_summary: Some(PendingReceiveResumeSummary {
            resumable_file_count: 2,
            completed_file_count: 1,
            partial_file_count: 1,
            received_bytes: 1536,
        }),
        decision: Arc::new((Mutex::new(None), Condvar::new())),
    };

    let dto = pending_offer_to_dto(&offer);

    let summary = dto
        .resume_summary
        .expect("resume summary should be present");
    assert_eq!(summary.resumable_file_count, 2);
    assert_eq!(summary.completed_file_count, 1);
    assert_eq!(summary.partial_file_count, 1);
    assert_eq!(summary.received_bytes, 1536);
}

#[test]
fn pending_receive_offer_dto_limits_file_preview_for_large_folders() {
    let offer = PendingReceiveOffer {
        transfer_id: "transfer-a".to_string(),
        root_name: "drop".to_string(),
        file_count: 100,
        total_bytes: 4096,
        sender_device_id: None,
        sender_device_name: None,
        sender_public_key_fingerprint: None,
        files: (0..100)
            .map(|index| PendingReceiveFile {
                manifest_path: format!("drop/file-{index:03}.txt"),
                size: 1,
                sha256: "a".repeat(64),
            })
            .collect(),
        resume_summary: None,
        decision: Arc::new((Mutex::new(None), Condvar::new())),
    };

    let dto = pending_offer_to_dto(&offer);

    assert_eq!(dto.file_count, 100);
    assert_eq!(dto.preview_file_count, RECEIVE_FILE_PREVIEW_LIMIT);
    assert_eq!(dto.files.len(), RECEIVE_FILE_PREVIEW_LIMIT);
    assert_eq!(dto.files[0].manifest_path, "drop/file-000.txt");
}

#[test]
fn transfer_history_dto_exposes_optional_security_mode() {
    let mut record = new_transfer_history_record(
        "receive-a".to_string(),
        "receive",
        "completed",
        "drop",
        1,
        10,
        10,
        20,
    );
    record.security_mode = Some("authenticated_encrypted_session".to_string());

    let dto = transfer_to_dto(&record);

    assert_eq!(
        dto.security_mode.as_deref(),
        Some("authenticated_encrypted_session")
    );
}

#[test]
fn legacy_plain_receive_report_does_not_refresh_trusted_device_contact() {
    let public_key = test_public_key("device-a");
    let trusted = Arc::new(Mutex::new(vec![TrustedDeviceRecord {
        schema_version: 1,
        device_id: "device-a".to_string(),
        device_name: "Known Mac".to_string(),
        platform: "macos".to_string(),
        host: "192.168.1.20".to_string(),
        port: 45821,
        public_key: public_key.public_key,
        public_key_fingerprint: public_key.fingerprint.clone(),
        pairing_code: "AAA-BBB".to_string(),
        paired_at_ms: 1,
        last_seen_at_ms: 1,
    }]));
    let report = TransferReceiveReport {
        transfer_id: "transfer-a".to_string(),
        root_name: "drop".to_string(),
        security_mode: TransferSecurityMode::LegacyPlain,
        sender_device_id: Some("device-a".to_string()),
        sender_device_name: Some("Spoofed Name".to_string()),
        sender_public_key_fingerprint: Some(public_key.fingerprint),
        bundle: None,
        files: Vec::new(),
    };

    refresh_trusted_device_contact_from_receive_report(&trusted, &report);

    let trusted = trusted.lock().unwrap();
    assert_eq!(trusted[0].device_name, "Known Mac");
    assert_eq!(trusted[0].last_seen_at_ms, 1);
}

#[test]
fn staged_bundle_dto_marks_saved_status() {
    let dir = unique_bundle_temp_dir("desktop-bundle-list");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();

    let bundles = list_staged_bundle_dtos_at(&staging_root, &import_root).unwrap();

    assert_eq!(bundles.len(), 1);
    assert_eq!(bundles[0].bundle_id, "bundle_1234567890");
    assert_eq!(bundles[0].staging_status, "saved");
    assert!(bundles[0].can_import_now);
    assert!(!bundles[0].has_import_receipt);
    assert!(!bundles[0].can_request_rollback);
    assert!(!bundles[0].import_conflict);
    assert_eq!(
        bundles[0].import_destination.as_deref(),
        Some(
            import_root
                .join("bundle_1234567890")
                .to_string_lossy()
                .as_ref()
        )
    );
    assert_eq!(bundles[0].import_conflict_count, 0);
    assert_eq!(bundles[0].import_plan_files.len(), 2);
    assert_eq!(
        bundles[0].import_plan_files[0].manifest_path,
        "files/manifest.json"
    );
    assert!(bundles[0]
        .import_plan_files
        .iter()
        .all(|file| !file.destination_exists));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn staged_bundle_dto_marks_import_destination_conflict() {
    let dir = unique_bundle_temp_dir("desktop-bundle-list-conflict");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
    fs::create_dir_all(import_root.join("bundle_1234567890")).unwrap();

    let bundles = list_staged_bundle_dtos_at(&staging_root, &import_root).unwrap();

    assert_eq!(bundles.len(), 1);
    assert_eq!(bundles[0].bundle_id, "bundle_1234567890");
    assert!(!bundles[0].can_import_now);
    assert!(bundles[0].import_conflict);
    assert_eq!(
        bundles[0].import_blocking_reason.as_deref(),
        Some("destination_exists")
    );
    assert_eq!(bundles[0].import_conflict_count, 0);
    assert_eq!(bundles[0].import_plan_files.len(), 2);
    assert!(bundles[0]
        .import_plan_files
        .iter()
        .all(|file| !file.destination_exists));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn staged_bundle_dto_includes_conflicting_import_files() {
    let dir = unique_bundle_temp_dir("desktop-bundle-list-file-conflict");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
    fs::create_dir_all(import_root.join("bundle_1234567890")).unwrap();
    fs::write(
        import_root.join("bundle_1234567890").join("content.bin"),
        b"existing",
    )
    .unwrap();

    let bundles = list_staged_bundle_dtos_at(&staging_root, &import_root).unwrap();

    assert_eq!(bundles.len(), 1);
    assert!(!bundles[0].can_import_now);
    assert!(bundles[0].import_conflict);
    assert_eq!(bundles[0].import_conflict_count, 1);
    assert_eq!(bundles[0].import_plan_files.len(), 2);
    let conflicted = bundles[0]
        .import_plan_files
        .iter()
        .find(|file| file.destination_exists)
        .expect("one planned import file should conflict");
    assert_eq!(conflicted.manifest_path, "files/content.bin");

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_staged_bundle_at_marks_bundle_imported() {
    let dir = unique_bundle_temp_dir("desktop-bundle-import");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();

    let imported =
        import_staged_bundle_at(&staging_root, &import_root, "bundle_1234567890").unwrap();

    assert_eq!(imported.bundle_id, "bundle_1234567890");
    assert_eq!(imported.staging_status, "imported");
    assert!(!imported.can_import_now);
    assert!(imported.has_import_receipt);
    assert!(imported.can_request_rollback);
    assert_eq!(
        imported.import_path.as_deref(),
        Some(
            import_root
                .join("bundle_1234567890")
                .to_string_lossy()
                .as_ref()
        )
    );
    assert!(import_root
        .join("bundle_1234567890")
        .join("content.bin")
        .is_file());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn staged_bundle_dto_keeps_imported_status_after_refresh() {
    let dir = unique_bundle_temp_dir("desktop-bundle-imported-refresh");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
    import_staged_bundle_at(&staging_root, &import_root, "bundle_1234567890").unwrap();

    let bundles = list_staged_bundle_dtos_at(&staging_root, &import_root).unwrap();

    assert_eq!(bundles.len(), 1);
    assert_eq!(bundles[0].bundle_id, "bundle_1234567890");
    assert_eq!(bundles[0].staging_status, "imported");
    assert!(bundles[0].can_rollback_now);
    assert!(bundles[0].has_import_receipt);
    assert!(bundles[0].can_request_rollback);
    assert_eq!(bundles[0].rollback_file_count, 2);
    assert!(bundles[0].import_receipt_path.is_none());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_staged_bundle_at_rejects_unsafe_bundle_id() {
    let dir = unique_bundle_temp_dir("desktop-bundle-import-unsafe");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");

    let error = import_staged_bundle_at(&staging_root, &import_root, "../bundle").unwrap_err();

    assert!(error.contains("bundle_id"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn delete_staged_bundle_at_removes_saved_bundle() {
    let dir = unique_bundle_temp_dir("desktop-bundle-delete");
    let staging_root = dir.join("bundle_staging");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();

    let removed = delete_staged_bundle_at(&staging_root, "bundle_1234567890").unwrap();

    assert!(removed);
    assert!(!staging_root.join("bundle_1234567890").exists());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn prune_staged_bundle_dtos_at_removes_expired_bundles() {
    let dir = unique_bundle_temp_dir("desktop-bundle-prune");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let expired_root = create_desktop_test_bundle(&dir, "expired", "bundle_expired");
    let fresh_root = create_desktop_test_bundle(&dir, "fresh", "bundle_fresh");
    nekodrop_storage::stage_bundle_directory(&expired_root, &staging_root).unwrap();
    let cutoff = std::time::SystemTime::now();
    nekodrop_storage::stage_bundle_directory(&fresh_root, &staging_root).unwrap();

    let pruned = prune_staged_bundle_dtos_at(&staging_root, cutoff).unwrap();

    assert_eq!(pruned, vec!["bundle_expired"]);
    let remaining = list_staged_bundle_dtos_at(&staging_root, &import_root).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].bundle_id, "bundle_fresh");

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_devices_list_returns_trusted_devices_without_bundle_scope() {
    let dir = unique_bundle_temp_dir("local-bridge-devices");
    let staging_root = dir.join("bundle_staging");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
    let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];
    let request = serde_json::json!({
        "kind": "devices.list",
        "payload": {
            "request_id": "bridge-request-1",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "trusted_only": true
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_auth_at(
        &request,
        &trusted,
        None,
        &staging_root,
        &dir.join("bundle_imports"),
        &[local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::DeviceRead],
            1_000,
            5_000,
        )],
        2_000,
    )
    .unwrap();

    assert_eq!(response.request_id, "bridge-request-1");
    assert_eq!(response.status, "ok");
    assert_eq!(response.devices.len(), 1);
    assert_eq!(response.devices[0].device_id, "device-a");
    assert!(response.staged_bundles.is_empty());
    assert!(response.transfer_status.is_none());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_devices_list_includes_staged_bundles_with_bundle_scope() {
    let dir = unique_bundle_temp_dir("local-bridge-devices-with-bundle-scope");
    let staging_root = dir.join("bundle_staging");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
    let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];
    let request = serde_json::json!({
        "kind": "devices.list",
        "payload": {
            "request_id": "bridge-request-1",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "trusted_only": true
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_auth_at(
        &request,
        &trusted,
        None,
        &staging_root,
        &dir.join("bundle_imports"),
        &[local_bridge_authorization(
            "local-agent-app",
            &[
                LocalBridgePermissionScope::DeviceRead,
                LocalBridgePermissionScope::BundleRead,
            ],
            1_000,
            5_000,
        )],
        2_000,
    )
    .unwrap();

    assert_eq!(response.request_id, "bridge-request-1");
    assert_eq!(response.status, "ok");
    assert_eq!(response.devices.len(), 1);
    assert_eq!(response.devices[0].device_id, "device-a");
    assert_eq!(response.staged_bundles.len(), 1);
    assert_eq!(response.staged_bundles[0].bundle_id, "bundle_1234567890");
    assert!(response.staged_bundles[0].staging_path.is_empty());
    assert!(response.staged_bundles[0].import_destination.is_none());
    assert!(response.transfer_status.is_none());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_read_only_requests_require_matching_scope() {
    let dir = unique_bundle_temp_dir("local-bridge-read-only-security");
    let staging_root = dir.join("bundle_staging");
    let request = serde_json::json!({
        "kind": "transfer.status",
        "payload": {
            "request_id": "bridge-request-status",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "transfer_id": null
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_auth_at(
        &request,
        &[],
        None,
        &staging_root,
        &dir.join("bundle_imports"),
        &[local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::TransferStatusRead],
            1_000,
            5_000,
        )],
        2_000,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.security_state, "read_only");
    assert!(!response.requires_user_confirmation);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_read_only_requests_without_scope_require_authorization() {
    let dir = unique_bundle_temp_dir("local-bridge-read-only-requires-scope");
    let staging_root = dir.join("bundle_staging");
    let request = serde_json::json!({
        "kind": "transfer.status",
        "payload": {
            "request_id": "bridge-request-status",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "transfer_id": null
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_auth_at(
        &request,
        &[],
        None,
        &staging_root,
        &dir.join("bundle_imports"),
        &[local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::DeviceRead],
            1_000,
            5_000,
        )],
        2_000,
    )
    .unwrap();

    assert_eq!(response.status, "pending_auth");
    assert_eq!(response.security_state, "requires_user_confirmation");
    assert!(response.requires_user_confirmation);
    assert!(response.transfer_status.is_none());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_unauthorized_read_only_response_marks_anonymous_client() {
    let dir = unique_bundle_temp_dir("local-bridge-client-anonymous-pending");
    let staging_root = dir.join("bundle_staging");
    let request = serde_json::json!({
        "kind": "transfer.status",
        "payload": {
            "request_id": "bridge-request-status",
            "transfer_id": null
        }
    })
    .to_string();

    let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

    assert_eq!(response.status, "pending_auth");
    assert_eq!(response.client_state, "anonymous");
    assert!(response.client_id.is_none());
    assert!(response.client_display_name.is_none());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_authorized_read_only_response_echoes_identified_client() {
    let dir = unique_bundle_temp_dir("local-bridge-client-read-identified");
    let staging_root = dir.join("bundle_staging");
    let request = serde_json::json!({
        "kind": "transfer.status",
        "payload": {
            "request_id": "bridge-request-status",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "transfer_id": null
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_auth_at(
        &request,
        &[],
        None,
        &staging_root,
        &dir.join("bundle_imports"),
        &[local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::TransferStatusRead],
            1_000,
            5_000,
        )],
        2_000,
    )
    .unwrap();

    assert_eq!(response.client_state, "identified");
    assert_eq!(response.client_id.as_deref(), Some("local-agent-app"));
    assert_eq!(
        response.client_display_name.as_deref(),
        Some("Local Agent App")
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_response_echoes_identified_client() {
    let dir = unique_bundle_temp_dir("local-bridge-client-identified");
    let staging_root = dir.join("bundle_staging");
    let request = serde_json::json!({
        "kind": "transfer.status",
        "payload": {
            "request_id": "bridge-request-status",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "transfer_id": null
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_auth_at(
        &request,
        &[],
        None,
        &staging_root,
        &dir.join("bundle_imports"),
        &[local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::TransferStatusRead],
            1_000,
            5_000,
        )],
        2_000,
    )
    .unwrap();

    assert_eq!(response.client_state, "identified");
    assert_eq!(response.client_id.as_deref(), Some("local-agent-app"));
    assert_eq!(
        response.client_display_name.as_deref(),
        Some("Local Agent App")
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_detail_returns_matching_staged_bundle() {
    let dir = unique_bundle_temp_dir("local-bridge-bundle-detail");
    let staging_root = dir.join("bundle_staging");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
    let request = serde_json::json!({
        "kind": "bundle.detail",
        "payload": {
            "request_id": "bridge-request-detail",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_1234567890"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_auth_at(
        &request,
        &[],
        None,
        &staging_root,
        &dir.join("bundle_imports"),
        &[local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleRead],
            1_000,
            5_000,
        )],
        2_000,
    )
    .unwrap();

    assert_eq!(response.request_id, "bridge-request-detail");
    assert_eq!(response.status, "ok");
    assert_eq!(response.security_state, "read_only");
    assert_eq!(response.staged_bundles.len(), 1);
    assert_eq!(response.staged_bundles[0].bundle_id, "bundle_1234567890");
    assert!(response.staged_bundles[0].staging_path.is_empty());
    assert!(response.staged_bundles[0].import_destination.is_none());
    assert!(response.staged_bundles[0].import_receipt_path.is_none());
    assert!(!response.staged_bundles[0].has_import_receipt);
    assert!(!response.staged_bundles[0].can_request_rollback);
    assert!(response.staged_bundles[0]
        .import_plan_files
        .iter()
        .all(|file| file.destination_path.is_empty()));
    assert!(!response.requires_user_confirmation);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_detail_requires_bundle_read_scope() {
    let dir = unique_bundle_temp_dir("local-bridge-bundle-detail-scope");
    let staging_root = dir.join("bundle_staging");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
    let request = serde_json::json!({
        "kind": "bundle.detail",
        "payload": {
            "request_id": "bridge-request-detail-no-scope",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_1234567890"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_auth_at(
        &request,
        &[],
        None,
        &staging_root,
        &dir.join("bundle_imports"),
        &[local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_000,
            5_000,
        )],
        2_000,
    )
    .unwrap();

    assert_eq!(response.request_id, "bridge-request-detail-no-scope");
    assert_eq!(response.status, "pending_auth");
    assert!(response.requires_user_confirmation);
    assert!(response.staged_bundles.is_empty());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_detail_returns_imported_status_without_local_paths() {
    let dir = unique_bundle_temp_dir("local-bridge-bundle-detail-imported");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
    import_staged_bundle_at(&staging_root, &import_root, "bundle_1234567890").unwrap();
    let request = serde_json::json!({
        "kind": "bundle.detail",
        "payload": {
            "request_id": "bridge-request-detail-imported",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_1234567890"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_auth_at(
        &request,
        &[],
        None,
        &staging_root,
        &import_root,
        &[local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleRead],
            1_000,
            5_000,
        )],
        2_000,
    )
    .unwrap();

    assert_eq!(response.request_id, "bridge-request-detail-imported");
    assert_eq!(response.status, "ok");
    assert_eq!(response.security_state, "read_only");
    assert_eq!(response.staged_bundles.len(), 1);
    let bundle = &response.staged_bundles[0];
    assert_eq!(bundle.bundle_id, "bundle_1234567890");
    assert_eq!(bundle.staging_status, "imported");
    assert!(bundle.staging_path.is_empty());
    assert!(bundle.import_path.is_none());
    assert!(bundle.import_destination.is_none());
    assert!(bundle.import_receipt_path.is_none());
    assert!(bundle.has_import_receipt);
    assert_eq!(bundle.rollback_file_count, 2);
    assert!(bundle.can_rollback_now);
    assert!(bundle.can_request_rollback);
    assert!(bundle.rollback_blocking_reason.is_none());
    assert_eq!(bundle.rolled_back_file_count, 0);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_detail_returns_rolled_back_status_without_local_paths() {
    let dir = unique_bundle_temp_dir("local-bridge-bundle-detail-rolled-back");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
    import_staged_bundle_at(&staging_root, &import_root, "bundle_1234567890").unwrap();
    rollback_imported_bundle_at(&import_root, "bundle_1234567890").unwrap();
    let request = serde_json::json!({
        "kind": "bundle.detail",
        "payload": {
            "request_id": "bridge-request-detail-rolled-back",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_1234567890"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_auth_at(
        &request,
        &[],
        None,
        &staging_root,
        &import_root,
        &[local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleRead],
            1_000,
            5_000,
        )],
        2_000,
    )
    .unwrap();

    assert_eq!(response.request_id, "bridge-request-detail-rolled-back");
    assert_eq!(response.status, "ok");
    assert_eq!(response.staged_bundles.len(), 1);
    let bundle = &response.staged_bundles[0];
    assert_eq!(bundle.bundle_id, "bundle_1234567890");
    assert_eq!(bundle.staging_status, "rolled_back");
    assert!(bundle.staging_path.is_empty());
    assert!(bundle.import_path.is_none());
    assert!(bundle.import_destination.is_none());
    assert!(bundle.import_receipt_path.is_none());
    assert!(bundle.has_import_receipt);
    assert_eq!(bundle.rollback_file_count, 2);
    assert!(!bundle.can_rollback_now);
    assert!(!bundle.can_request_rollback);
    assert_eq!(
        bundle.rollback_blocking_reason.as_deref(),
        Some("destination_missing")
    );
    assert_eq!(bundle.rolled_back_file_count, 2);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_detail_returns_unsupported_for_missing_bundle() {
    let dir = unique_bundle_temp_dir("local-bridge-bundle-detail-missing");
    let staging_root = dir.join("bundle_staging");
    let request = serde_json::json!({
        "kind": "bundle.detail",
        "payload": {
            "request_id": "bridge-request-detail",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_1234567890"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_auth_at(
        &request,
        &[],
        None,
        &staging_root,
        &dir.join("bundle_imports"),
        &[local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleRead],
            1_000,
            5_000,
        )],
        2_000,
    )
    .unwrap();

    assert_eq!(response.request_id, "bridge-request-detail");
    assert_eq!(response.status, "unsupported");
    assert_eq!(response.security_state, "read_only");
    assert!(response.staged_bundles.is_empty());
    assert!(response.message.contains("not found"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_mutating_requests_require_user_confirmation() {
    let dir = unique_bundle_temp_dir("local-bridge-mutating-security");
    let staging_root = dir.join("bundle_staging");
    let request = serde_json::json!({
        "kind": "bundle.import",
        "payload": {
            "request_id": "bridge-request-import",
            "staged_bundle_id": "bundle_1234567890",
            "expected_bundle_type": "skill"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

    assert_eq!(response.status, "pending_auth");
    assert_eq!(response.security_state, "requires_user_confirmation");
    assert!(response.requires_user_confirmation);
    assert!(response.message.contains("user confirmation"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_mutations_are_pending_auth() {
    let dir = unique_bundle_temp_dir("local-bridge-pending");
    let staging_root = dir.join("bundle_staging");
    let send_request = serde_json::json!({
        "kind": "bundle.send",
        "payload": {
            "request_id": "bridge-request-send",
            "target_device_id": "device-a",
            "bundle_root": "bundle",
            "bundle_type": "skill",
            "require_trusted_device": true
        }
    })
    .to_string();
    let import_request = serde_json::json!({
        "kind": "bundle.import",
        "payload": {
            "request_id": "bridge-request-import",
            "staged_bundle_id": "bundle_1234567890",
            "expected_bundle_type": "skill"
        }
    })
    .to_string();

    let send_response =
        handle_local_bridge_request_at(&send_request, &[], None, &staging_root).unwrap();
    let import_response =
        handle_local_bridge_request_at(&import_request, &[], None, &staging_root).unwrap();

    assert_eq!(send_response.request_id, "bridge-request-send");
    assert_eq!(send_response.status, "pending_auth");
    assert!(send_response.message.contains("auth"));
    assert_eq!(import_response.request_id, "bridge-request-import");
    assert_eq!(import_response.status, "pending_auth");
    assert!(import_response.message.contains("runtime"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_authorized_client_can_pass_import_gate() {
    let dir = unique_bundle_temp_dir("local-bridge-authorized-import");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let authorizations = vec![local_bridge_authorization(
        "local-agent-app",
        &[LocalBridgePermissionScope::BundleImportRequest],
        1_000,
        2_000,
    )];
    let import_request = serde_json::json!({
        "kind": "bundle.import",
        "payload": {
            "request_id": "bridge-request-import",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_1234567890",
            "expected_bundle_type": "skill"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_auth_at(
        &import_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &authorizations,
        1_500,
    )
    .unwrap();

    assert_eq!(response.status, "pending_runtime");
    assert_eq!(response.security_state, "authorized");
    assert!(!response.requires_user_confirmation);
    assert!(response.message.contains("authorized"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_expired_authorization_requires_confirmation_again() {
    let dir = unique_bundle_temp_dir("local-bridge-expired-auth");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let authorizations = vec![local_bridge_authorization(
        "local-agent-app",
        &[LocalBridgePermissionScope::BundleImportRequest],
        1_000,
        1_100,
    )];
    let import_request = serde_json::json!({
        "kind": "bundle.import",
        "payload": {
            "request_id": "bridge-request-import",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_1234567890",
            "expected_bundle_type": "skill"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_auth_at(
        &import_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &authorizations,
        1_500,
    )
    .unwrap();

    assert_eq!(response.status, "pending_auth");
    assert_eq!(response.security_state, "requires_user_confirmation");

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_authorization_is_bound_to_client_app_kind() {
    let dir = unique_bundle_temp_dir("local-bridge-app-kind-auth");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let authorizations = vec![local_bridge_authorization(
        "local-agent-app",
        &[LocalBridgePermissionScope::BundleImportRequest],
        1_000,
        5_000,
    )];
    let import_request = serde_json::json!({
        "kind": "bundle.import",
        "payload": {
            "request_id": "bridge-request-import",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "automation"
            },
            "staged_bundle_id": "bundle_1234567890",
            "expected_bundle_type": "skill"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_auth_at(
        &import_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &authorizations,
        1_500,
    )
    .unwrap();

    assert_eq!(response.status, "pending_auth");
    assert_eq!(response.security_state, "requires_user_confirmation");

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_authorization_request_waits_for_user_confirmation() {
    let dir = unique_bundle_temp_dir("local-bridge-authorization");
    let staging_root = dir.join("bundle_staging");
    let request = serde_json::json!({
        "kind": "authorization.request",
        "payload": {
            "request_id": "bridge-auth-1",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "requested_scopes": [
                "device.read",
                "bundle.send"
            ],
            "reason": "Send a skill bundle to a trusted desktop device",
            "ttl_seconds": 900
        }
    })
    .to_string();

    let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

    assert_eq!(response.request_id, "bridge-auth-1");
    assert_eq!(response.status, "pending_auth");
    assert_eq!(response.security_state, "requires_user_confirmation");
    assert!(response.requires_user_confirmation);
    assert_eq!(response.client_state, "identified");
    assert_eq!(response.client_id.as_deref(), Some("local-agent-app"));
    assert_eq!(
        response.authorization_scopes,
        vec!["device.read".to_string(), "bundle.send".to_string()]
    );
    assert_eq!(
        response.authorization_reason.as_deref(),
        Some("Send a skill bundle to a trusted desktop device")
    );
    assert_eq!(response.authorization_ttl_seconds, Some(900));
    assert!(response.authorization_code.as_deref().is_some_and(|code| {
        code.len() == 7
            && code.as_bytes()[3] == b'-'
            && code
                .chars()
                .filter(|character| *character != '-')
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_lowercase())
    }));
    assert!(response.authorization_expires_at_ms.is_some());
    assert!(response.message.contains("authorization"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_authorization_request_creates_short_code() {
    let request = LocalBridgeAuthorizationRequest {
        request_id: "bridge-auth-1".to_string(),
        client: LocalBridgeClientIdentity {
            client_id: "local-agent-app".to_string(),
            display_name: "Local Agent App".to_string(),
            app_kind: Some("agent".to_string()),
        },
        requested_scopes: vec![
            LocalBridgePermissionScope::DeviceRead,
            LocalBridgePermissionScope::BundleSend,
        ],
        reason: "Send a skill bundle".to_string(),
        ttl_seconds: Some(900),
    };

    let pending = pending_local_bridge_authorization_from_request(&request, 1_000).unwrap();

    assert_eq!(pending.request_id, "bridge-auth-1");
    assert_eq!(pending.client.client_id, "local-agent-app");
    assert_eq!(pending.requested_scopes, request.requested_scopes);
    assert_eq!(pending.expires_at_ms, 901_000);
    assert_eq!(pending.authorization_code.len(), 7);
    assert_eq!(pending.authorization_code.as_bytes()[3], b'-');
    assert!(pending
        .authorization_code
        .chars()
        .filter(|character| *character != '-')
        .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_lowercase()));
}

#[test]
fn local_bridge_runtime_stores_pending_authorization_request() {
    let dir = unique_bundle_temp_dir("local-bridge-runtime-pending-auth");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    let request = serde_json::json!({
        "kind": "authorization.request",
        "payload": {
            "request_id": "bridge-auth-runtime",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "requested_scopes": [
                "bundle.send"
            ],
            "reason": "Send a local bundle",
            "ttl_seconds": 900
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_000,
    )
    .unwrap();

    assert_eq!(response.status, "pending_auth");
    let pending = runtime
        .pending_authorization
        .lock()
        .unwrap()
        .clone()
        .unwrap();
    assert_eq!(pending.request_id, "bridge-auth-runtime");
    assert_eq!(pending.client.client_id, "local-agent-app");
    assert_eq!(
        response.authorization_code.as_deref(),
        Some(pending.authorization_code.as_str())
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn confirmed_runtime_authorization_allows_future_mutating_request() {
    let dir = unique_bundle_temp_dir("local-bridge-runtime-confirmed-auth");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    let auth_request = serde_json::json!({
        "kind": "authorization.request",
        "payload": {
            "request_id": "bridge-auth-runtime",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "requested_scopes": [
                "bundle.import.request"
            ],
            "reason": "Import a staged bundle",
            "ttl_seconds": 900
        }
    })
    .to_string();
    let import_request = serde_json::json!({
        "kind": "bundle.import",
        "payload": {
            "request_id": "bridge-request-import",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_1234567890",
            "expected_bundle_type": "skill"
        }
    })
    .to_string();

    let auth_response = handle_local_bridge_request_with_runtime_at(
        &auth_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_000,
    )
    .unwrap();
    let authorization = confirm_local_bridge_runtime_authorization_at(
        &runtime,
        auth_response.authorization_code.as_deref().unwrap(),
        1_500,
    )
    .unwrap();
    let response = handle_local_bridge_request_with_runtime_at(
        &import_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_600,
    )
    .unwrap();

    assert_eq!(authorization.client_id, "local-agent-app");
    assert_eq!(response.status, "pending_runtime");
    assert_eq!(response.security_state, "authorized");
    assert!(!response.requires_user_confirmation);
    assert!(runtime.pending_authorization.lock().unwrap().is_none());
    assert_eq!(runtime.authorizations.lock().unwrap().len(), 1);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn authorized_local_bridge_bundle_send_is_queued_as_pending_action() {
    let dir = unique_bundle_temp_dir("local-bridge-runtime-pending-send-action");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_000,
            5_000,
        ));
    let send_request = serde_json::json!({
        "kind": "bundle.send",
        "payload": {
            "request_id": "bridge-request-send",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "target_device_id": "device-a",
            "bundle_root": "bundle",
            "bundle_type": "skill",
            "require_trusted_device": true
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &send_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();

    assert_eq!(response.status, "pending_runtime");
    let actions = runtime.pending_actions.lock().unwrap();
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        crate::app_state::LocalBridgePendingAction::SendBundle(action) => {
            assert_eq!(action.request_id, "bridge-request-send");
            assert_eq!(action.client.client_id, "local-agent-app");
            assert_eq!(action.target_device_id.as_deref(), Some("device-a"));
            assert_eq!(action.bundle_root, "bundle");
            assert_eq!(action.bundle_type, BundleType::Skill);
            assert!(action.require_trusted_device);
            assert_eq!(action.requested_at_ms, 1_500);
        }
        other => panic!("expected send bundle action, got {other:?}"),
    }
    drop(actions);
    let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].request_id, "bridge-request-send");
    assert_eq!(results[0].status, "queued");
    assert_eq!(results[0].lifecycle_status.as_deref(), Some("queued"));
    assert_eq!(results[0].bundle_type.as_deref(), Some("skill"));
    assert_eq!(results[0].target_device_id.as_deref(), Some("device-a"));
    assert!(results[0].bundle_root.is_none());
    let events = runtime.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    match &events[0] {
        nekolink_protocol::LocalBridgeEvent::ActionUpdated(event) => {
            assert_eq!(event.request_id, "bridge-request-send");
            assert_eq!(
                event.status,
                nekolink_protocol::LocalBridgeActionLifecycleStatus::Queued
            );
            assert_eq!(event.bundle_type, Some(BundleType::Skill));
            assert_eq!(event.target_device_id.as_deref(), Some("device-a"));
        }
        other => panic!("expected action.updated event, got {other:?}"),
    }
    assert_eq!(
        runtime.authorizations.lock().unwrap()[0].last_used_at_ms,
        1_500
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unauthorized_local_bridge_request_does_not_update_last_used_at() {
    let dir = unique_bundle_temp_dir("local-bridge-unauthorized-last-used");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_000,
            5_000,
        ));
    let send_request = serde_json::json!({
        "kind": "bundle.send",
        "payload": {
            "request_id": "bridge-request-send",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "automation"
            },
            "target_device_id": "device-a",
            "bundle_root": "bundle",
            "bundle_type": "skill",
            "require_trusted_device": true
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &send_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();

    assert_eq!(response.status, "pending_auth");
    assert_eq!(
        runtime.authorizations.lock().unwrap()[0].last_used_at_ms,
        1_000
    );
    assert!(runtime.pending_actions.lock().unwrap().is_empty());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn authorized_local_bridge_mutation_reuses_duplicate_pending_action() {
    let dir = unique_bundle_temp_dir("local-bridge-runtime-dedupe-pending-action");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            5_000,
        ));
    let import_request = serde_json::json!({
        "kind": "bundle.import",
        "payload": {
            "request_id": "bridge-request-import",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_1234567890",
            "expected_bundle_type": "skill",
            "conflict_strategy": "rename"
        }
    })
    .to_string();

    let first_response = handle_local_bridge_request_with_runtime_at(
        &import_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();
    let second_response = handle_local_bridge_request_with_runtime_at(
        &import_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_700,
    )
    .unwrap();

    assert_eq!(first_response.status, "pending_runtime");
    assert_eq!(second_response.status, "pending_runtime");
    let actions = runtime.pending_actions.lock().unwrap();
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        crate::app_state::LocalBridgePendingAction::ImportBundle(action) => {
            assert_eq!(action.request_id, "bridge-request-import");
            assert_eq!(action.staged_bundle_id, "bundle_1234567890");
            assert_eq!(action.conflict_strategy, "rename");
            assert_eq!(action.requested_at_ms, 1_500);
        }
        other => panic!("expected import bundle action, got {other:?}"),
    }
    drop(actions);
    let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].request_id, "bridge-request-import");
    assert_eq!(results[0].status, "queued");
    assert_eq!(results[0].bundle_id.as_deref(), Some("bundle_1234567890"));
    assert_eq!(results[0].claimed_at_ms, 1_500);
    assert_eq!(
        runtime.authorizations.lock().unwrap()[0].last_used_at_ms,
        1_700
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn authorized_local_bridge_mutation_retry_matches_identity_not_display_name() {
    let dir = unique_bundle_temp_dir("local-bridge-runtime-dedupe-client-display-name");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            5_000,
        ));
    let import_request = |display_name: &str, staged_bundle_id: &str| {
        serde_json::json!({
            "kind": "bundle.import",
            "payload": {
                "request_id": "bridge-request-import",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": display_name,
                    "app_kind": "agent"
                },
                "staged_bundle_id": staged_bundle_id,
                "expected_bundle_type": "skill",
                "conflict_strategy": "rename"
            }
        })
        .to_string()
    };

    handle_local_bridge_request_with_runtime_at(
        &import_request("Local Agent App", "bundle_first"),
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();
    let second_response = handle_local_bridge_request_with_runtime_at(
        &import_request("Renamed Agent App", "bundle_second"),
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_700,
    )
    .unwrap();

    assert_eq!(second_response.status, "conflict");
    assert_eq!(
        second_response.message,
        "local bridge request_id already belongs to a different payload"
    );

    let actions = runtime.pending_actions.lock().unwrap();
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        crate::app_state::LocalBridgePendingAction::ImportBundle(action) => {
            assert_eq!(action.request_id, "bridge-request-import");
            assert_eq!(action.client.display_name, "Local Agent App");
            assert_eq!(action.staged_bundle_id, "bundle_first");
            assert_eq!(action.requested_at_ms, 1_500);
        }
        other => panic!("expected import bundle action, got {other:?}"),
    }

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn authorized_local_bridge_mutations_do_not_dedupe_different_action_kinds() {
    let dir = unique_bundle_temp_dir("local-bridge-runtime-dedupe-action-kind");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[
                LocalBridgePermissionScope::BundleSend,
                LocalBridgePermissionScope::BundleImportRequest,
            ],
            1_000,
            5_000,
        ));
    let send_request = serde_json::json!({
        "kind": "bundle.send",
        "payload": {
            "request_id": "bridge-shared-request",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "target_device_id": "device-a",
            "bundle_root": "bundle",
            "bundle_type": "skill",
            "require_trusted_device": true
        }
    })
    .to_string();
    let import_request = serde_json::json!({
        "kind": "bundle.import",
        "payload": {
            "request_id": "bridge-shared-request",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_1234567890",
            "expected_bundle_type": "skill",
            "conflict_strategy": "reject"
        }
    })
    .to_string();

    handle_local_bridge_request_with_runtime_at(
        &send_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();
    handle_local_bridge_request_with_runtime_at(
        &import_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_700,
    )
    .unwrap();

    let actions = runtime.pending_actions.lock().unwrap();
    assert_eq!(actions.len(), 2);
    assert!(matches!(
        actions[0],
        crate::app_state::LocalBridgePendingAction::SendBundle(_)
    ));
    assert!(matches!(
        actions[1],
        crate::app_state::LocalBridgePendingAction::ImportBundle(_)
    ));
    drop(actions);
    let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results
        .iter()
        .any(|result| result.action_kind == "bundle.send"));
    assert!(results
        .iter()
        .any(|result| result.action_kind == "bundle.import"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn authorized_local_bridge_mutation_retry_returns_existing_terminal_result() {
    let dir = unique_bundle_temp_dir("local-bridge-runtime-terminal-retry");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            5_000,
        ));
    runtime
        .pending_action_results
        .lock()
        .unwrap()
        .push(LocalBridgePendingActionResult {
            request_id: "bridge-request-import".to_string(),
            action_kind: "bundle.import".to_string(),
            client_id: "local-agent-app".to_string(),
            client_display_name: "Local Agent App".to_string(),
            client_app_kind: Some("agent".to_string()),
            status: "completed".to_string(),
            lifecycle_status: Some("succeeded".to_string()),
            reason: None,
            message: "local bridge bundle was imported by the desktop runtime".to_string(),
            bundle_id: Some("bundle_1234567890".to_string()),
            bundle_type: Some("skill".to_string()),
            bundle_root: None,
            target_device_id: None,
            require_trusted_device: None,
            conflict_strategy: Some("rename".to_string()),
            skipped_file_count: 0,
            import_receipt_path: Some(
                "/private/local/nekodrop/imports/bundle_1234567890/receipt.json".to_string(),
            ),
            rollback_file_count: 2,
            rollback_blocking_reason: None,
            rolled_back_file_count: 0,
            requested_at_ms: 1_500,
            claimed_at_ms: 2_000,
        });
    let import_request = serde_json::json!({
        "kind": "bundle.import",
        "payload": {
            "request_id": "bridge-request-import",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Renamed Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_1234567890",
            "expected_bundle_type": "skill",
            "conflict_strategy": "rename"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &import_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.message, "local bridge action result snapshot");
    assert!(runtime.pending_actions.lock().unwrap().is_empty());
    assert_eq!(response.action_results.len(), 1);
    assert_eq!(
        response.action_results[0].request_id,
        "bridge-request-import"
    );
    assert_eq!(response.action_results[0].action_kind, "bundle.import");
    assert_eq!(response.action_results[0].status, "completed");
    assert_eq!(
        response.action_results[0].lifecycle_status.as_deref(),
        Some("succeeded")
    );
    assert_eq!(
        response.action_results[0].bundle_id.as_deref(),
        Some("bundle_1234567890")
    );
    assert!(response.action_results[0].import_receipt_path.is_none());
    assert!(response.action_results[0].has_import_receipt);
    assert!(response.action_results[0].can_request_rollback);
    assert_eq!(
        runtime.authorizations.lock().unwrap()[0].last_used_at_ms,
        2_500
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn authorized_local_bridge_mutation_rejects_payload_change_for_same_request_id() {
    let dir = unique_bundle_temp_dir("local-bridge-runtime-payload-change-reject");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            5_000,
        ));
    let first_request = serde_json::json!({
        "kind": "bundle.import",
        "payload": {
            "request_id": "bridge-request-import",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_1234567890",
            "expected_bundle_type": "skill",
            "conflict_strategy": "rename"
        }
    })
    .to_string();
    let second_request = serde_json::json!({
        "kind": "bundle.import",
        "payload": {
            "request_id": "bridge-request-import",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_other",
            "expected_bundle_type": "workspace",
            "conflict_strategy": "reject"
        }
    })
    .to_string();

    let first_response = handle_local_bridge_request_with_runtime_at(
        &first_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();
    let second_response = handle_local_bridge_request_with_runtime_at(
        &second_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_700,
    )
    .unwrap();

    assert_eq!(first_response.status, "pending_runtime");
    assert_eq!(second_response.status, "conflict");
    assert_eq!(
        second_response.message,
        "local bridge request_id already belongs to a different payload"
    );
    assert_eq!(second_response.action_results.len(), 1);
    assert_eq!(
        second_response.action_results[0].request_id,
        "bridge-request-import"
    );
    assert_eq!(
        second_response.action_results[0].action_kind,
        "bundle.import"
    );
    assert_eq!(
        second_response.action_results[0].bundle_id.as_deref(),
        Some("bundle_1234567890")
    );
    assert_eq!(
        runtime.pending_actions.lock().unwrap().len(),
        1,
        "the original pending action should stay queued"
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn authorized_local_bridge_bundle_import_is_queued_as_pending_action() {
    let dir = unique_bundle_temp_dir("local-bridge-runtime-pending-import-action");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            5_000,
        ));
    let import_request = serde_json::json!({
        "kind": "bundle.import",
        "payload": {
            "request_id": "bridge-request-import",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_1234567890",
            "expected_bundle_type": "skill"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &import_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();

    assert_eq!(response.status, "pending_runtime");
    let actions = runtime.pending_actions.lock().unwrap();
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        crate::app_state::LocalBridgePendingAction::ImportBundle(action) => {
            assert_eq!(action.request_id, "bridge-request-import");
            assert_eq!(action.client.client_id, "local-agent-app");
            assert_eq!(action.staged_bundle_id, "bundle_1234567890");
            assert_eq!(action.expected_bundle_type, Some(BundleType::Skill));
            assert_eq!(action.requested_at_ms, 1_500);
        }
        other => panic!("expected import bundle action, got {other:?}"),
    }
    drop(actions);
    let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].request_id, "bridge-request-import");
    assert_eq!(results[0].status, "queued");
    assert_eq!(results[0].bundle_id.as_deref(), Some("bundle_1234567890"));
    assert_eq!(results[0].bundle_type.as_deref(), Some("skill"));
    assert!(results[0].import_receipt_path.is_none());
    let events = runtime.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    match &events[0] {
        nekolink_protocol::LocalBridgeEvent::ActionUpdated(event) => {
            assert_eq!(event.request_id, "bridge-request-import");
            assert_eq!(event.bundle_id.as_deref(), Some("bundle_1234567890"));
            assert_eq!(event.bundle_type, Some(BundleType::Skill));
        }
        other => panic!("expected action.updated event, got {other:?}"),
    }

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn authorized_local_bridge_bundle_rollback_is_queued_as_pending_action() {
    let dir = unique_bundle_temp_dir("local-bridge-runtime-pending-rollback-action");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            5_000,
        ));
    let rollback_request = serde_json::json!({
        "kind": "bundle.rollback",
        "payload": {
            "request_id": "bridge-request-rollback",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "bundle_id": "bundle_1234567890"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &rollback_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();

    assert_eq!(response.status, "pending_runtime");
    let actions = runtime.pending_actions.lock().unwrap();
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        crate::app_state::LocalBridgePendingAction::RollbackBundleImport(action) => {
            assert_eq!(action.request_id, "bridge-request-rollback");
            assert_eq!(action.client.client_id, "local-agent-app");
            assert_eq!(action.bundle_id, "bundle_1234567890");
            assert_eq!(action.requested_at_ms, 1_500);
        }
        other => panic!("expected rollback bundle action, got {other:?}"),
    }
    drop(actions);
    let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].request_id, "bridge-request-rollback");
    assert_eq!(results[0].status, "queued");
    assert_eq!(results[0].bundle_id.as_deref(), Some("bundle_1234567890"));
    let events = runtime.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    match &events[0] {
        nekolink_protocol::LocalBridgeEvent::ActionUpdated(event) => {
            assert_eq!(event.request_id, "bridge-request-rollback");
            assert_eq!(event.bundle_id.as_deref(), Some("bundle_1234567890"));
            assert_eq!(event.bundle_type, None);
        }
        other => panic!("expected action.updated event, got {other:?}"),
    }

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unauthorized_local_bridge_bundle_mutation_is_not_queued() {
    let dir = unique_bundle_temp_dir("local-bridge-runtime-no-pending-action");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    let import_request = serde_json::json!({
        "kind": "bundle.import",
        "payload": {
            "request_id": "bridge-request-import",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "staged_bundle_id": "bundle_1234567890",
            "expected_bundle_type": "skill"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &import_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();

    assert_eq!(response.status, "pending_auth");
    assert!(runtime.pending_actions.lock().unwrap().is_empty());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_pending_actions_are_listed_as_safe_summaries() {
    let runtime = LocalBridgeRuntimeState::default();
    runtime.pending_actions.lock().unwrap().extend([
        LocalBridgePendingAction::SendBundle(LocalBridgePendingSendBundleAction {
            request_id: "bridge-send-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            target_device_id: Some("device-a".to_string()),
            bundle_root: "/tmp/exported/bundle".to_string(),
            bundle_type: BundleType::Workspace,
            require_trusted_device: true,
            requested_at_ms: 1_500,
        }),
        LocalBridgePendingAction::ImportBundle(LocalBridgePendingImportBundleAction {
            request_id: "bridge-import-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            staged_bundle_id: "bundle_1234567890".to_string(),
            expected_bundle_type: Some(BundleType::Skill),
            conflict_strategy: "reject".to_string(),
            requested_at_ms: 1_600,
        }),
    ]);

    let actions = list_local_bridge_pending_actions_at(&runtime).unwrap();

    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].request_id, "bridge-send-1");
    assert_eq!(actions[0].action_kind, "bundle.send");
    assert_eq!(actions[0].client_display_name, "Local Agent App");
    assert_eq!(actions[0].bundle_type.as_deref(), Some("workspace"));
    assert_eq!(actions[0].target_device_id.as_deref(), Some("device-a"));
    assert!(actions[0].bundle_root.is_none());
    assert_eq!(actions[1].request_id, "bridge-import-1");
    assert_eq!(actions[1].action_kind, "bundle.import");
    assert_eq!(
        actions[1].staged_bundle_id.as_deref(),
        Some("bundle_1234567890")
    );
    assert_eq!(actions[1].expected_bundle_type.as_deref(), Some("skill"));
}

#[test]
fn local_bridge_pending_action_can_be_removed_by_request_id() {
    let runtime = LocalBridgeRuntimeState::default();
    runtime.pending_actions.lock().unwrap().extend([
        LocalBridgePendingAction::SendBundle(LocalBridgePendingSendBundleAction {
            request_id: "bridge-send-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            target_device_id: Some("device-a".to_string()),
            bundle_root: "bundle-a".to_string(),
            bundle_type: BundleType::Workspace,
            require_trusted_device: true,
            requested_at_ms: 1_500,
        }),
        LocalBridgePendingAction::ImportBundle(LocalBridgePendingImportBundleAction {
            request_id: "bridge-import-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            staged_bundle_id: "bundle_1234567890".to_string(),
            expected_bundle_type: Some(BundleType::Skill),
            conflict_strategy: "reject".to_string(),
            requested_at_ms: 1_600,
        }),
    ]);

    let removed = remove_local_bridge_pending_action_at(&runtime, "bridge-send-1").unwrap();
    let missing = remove_local_bridge_pending_action_at(&runtime, "bridge-missing").unwrap();
    let actions = list_local_bridge_pending_actions_at(&runtime).unwrap();

    assert!(removed);
    assert!(!missing);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].request_id, "bridge-import-1");
    let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].request_id, "bridge-send-1");
    assert_eq!(results[0].status, "cancelled");
    assert_eq!(results[0].lifecycle_status.as_deref(), Some("cancelled"));
    assert!(results[0].bundle_root.is_none());
    let events = runtime.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    match &events[0] {
        nekolink_protocol::LocalBridgeEvent::ActionUpdated(event) => {
            assert_eq!(event.request_id, "bridge-send-1");
            assert_eq!(
                event.status,
                nekolink_protocol::LocalBridgeActionLifecycleStatus::Cancelled
            );
        }
        other => panic!("expected action.updated event, got {other:?}"),
    }
}

#[test]
fn local_bridge_pending_action_can_be_taken_by_request_id() {
    let runtime = LocalBridgeRuntimeState::default();
    runtime.pending_actions.lock().unwrap().extend([
        LocalBridgePendingAction::SendBundle(LocalBridgePendingSendBundleAction {
            request_id: "bridge-send-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            target_device_id: Some("device-a".to_string()),
            bundle_root: "bundle-a".to_string(),
            bundle_type: BundleType::Workspace,
            require_trusted_device: true,
            requested_at_ms: 1_500,
        }),
        LocalBridgePendingAction::ImportBundle(LocalBridgePendingImportBundleAction {
            request_id: "bridge-import-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            staged_bundle_id: "bundle_1234567890".to_string(),
            expected_bundle_type: Some(BundleType::Skill),
            conflict_strategy: "reject".to_string(),
            requested_at_ms: 1_600,
        }),
    ]);

    let taken =
        take_local_bridge_pending_action_by_request_id(&runtime, "bridge-import-1").unwrap();
    let missing =
        take_local_bridge_pending_action_by_request_id(&runtime, "bridge-missing").unwrap();
    let actions = list_local_bridge_pending_actions_at(&runtime).unwrap();

    assert!(matches!(
        taken,
        Some(LocalBridgePendingAction::ImportBundle(ref action)) if action.request_id == "bridge-import-1"
    ));
    assert!(missing.is_none());
    // Taking by id must not disturb the queue order of remaining actions.
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].request_id, "bridge-send-1");
}

#[test]
fn local_bridge_pending_action_consumer_takes_next_action_fifo() {
    let runtime = LocalBridgeRuntimeState::default();
    runtime.pending_actions.lock().unwrap().extend([
        LocalBridgePendingAction::SendBundle(LocalBridgePendingSendBundleAction {
            request_id: "bridge-send-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            target_device_id: Some("device-a".to_string()),
            bundle_root: "/tmp/exported/bundle-a".to_string(),
            bundle_type: BundleType::Workspace,
            require_trusted_device: true,
            requested_at_ms: 1_500,
        }),
        LocalBridgePendingAction::ImportBundle(LocalBridgePendingImportBundleAction {
            request_id: "bridge-import-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            staged_bundle_id: "bundle_1234567890".to_string(),
            expected_bundle_type: Some(BundleType::Skill),
            conflict_strategy: "reject".to_string(),
            requested_at_ms: 1_600,
        }),
    ]);

    let claimed = take_next_local_bridge_pending_action_at(&runtime)
        .unwrap()
        .expect("first action should be claimed");
    let remaining = list_local_bridge_pending_actions_at(&runtime).unwrap();

    assert_eq!(claimed.request_id, "bridge-send-1");
    assert_eq!(claimed.action_kind, "bundle.send");
    assert_eq!(
        claimed.bundle_root.as_deref(),
        Some("/tmp/exported/bundle-a")
    );
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].request_id, "bridge-import-1");
}

#[test]
fn local_bridge_pending_action_consumer_returns_none_for_empty_queue() {
    let runtime = LocalBridgeRuntimeState::default();

    let claimed = take_next_local_bridge_pending_action_at(&runtime).unwrap();

    assert!(claimed.is_none());
}

#[test]
fn local_bridge_bundle_import_execution_imports_staged_bundle_and_records_result() {
    let dir = unique_bundle_temp_dir("local-bridge-import-execution");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    create_desktop_test_bundle(&staging_root, "bundle_1234567890", "bundle_1234567890");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .pending_actions
        .lock()
        .unwrap()
        .push(LocalBridgePendingAction::ImportBundle(
            LocalBridgePendingImportBundleAction {
                request_id: "bridge-import-1".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                staged_bundle_id: "bundle_1234567890".to_string(),
                expected_bundle_type: Some(BundleType::Skill),
                conflict_strategy: "reject".to_string(),
                requested_at_ms: 1_500,
            },
        ));

    let result =
        execute_next_local_bridge_bundle_import_at(&runtime, &staging_root, &import_root, 2_000)
            .unwrap()
            .expect("pending bundle.import action should be executed");
    let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

    assert_eq!(result.request_id, "bridge-import-1");
    assert_eq!(result.action_kind, "bundle.import");
    assert_eq!(result.status, "completed");
    assert_eq!(result.lifecycle_status.as_deref(), Some("succeeded"));
    assert_eq!(result.bundle_id.as_deref(), Some("bundle_1234567890"));
    assert_eq!(result.bundle_type.as_deref(), Some("skill"));
    assert_eq!(result.requested_at_ms, 1_500);
    assert_eq!(result.claimed_at_ms, 2_000);
    assert!(result.bundle_root.is_none());
    assert!(runtime.pending_actions.lock().unwrap().is_empty());
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].request_id, "bridge-import-1");
    assert_eq!(results[0].status, "completed");
    assert_eq!(results[0].lifecycle_status.as_deref(), Some("succeeded"));
    assert!(results[0].bundle_root.is_none());
    assert!(import_root
        .join("bundle_1234567890")
        .join("content.bin")
        .exists());
    let events = runtime.events.lock().unwrap();
    let statuses: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            nekolink_protocol::LocalBridgeEvent::ActionUpdated(event) => Some(event.status),
            _ => None,
        })
        .collect();
    assert_eq!(
        statuses,
        vec![
            nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
            nekolink_protocol::LocalBridgeActionLifecycleStatus::Succeeded,
        ]
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_rollback_execution_removes_imported_files_and_records_result() {
    let dir = unique_bundle_temp_dir("local-bridge-rollback-execution");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    create_desktop_test_bundle(&staging_root, "bundle_1234567890", "bundle_1234567890");
    let imported =
        import_staged_bundle_at(&staging_root, &import_root, "bundle_1234567890").unwrap();
    assert!(
        std::path::Path::new(imported.import_path.as_deref().unwrap())
            .join("content.bin")
            .exists()
    );
    let action = LocalBridgePendingRollbackBundleImportAction {
        request_id: "bridge-rollback-1".to_string(),
        client: LocalBridgeClientIdentity {
            client_id: "local-agent-app".to_string(),
            display_name: "Local Agent App".to_string(),
            app_kind: Some("agent".to_string()),
        },
        bundle_id: "bundle_1234567890".to_string(),
        requested_at_ms: 1_500,
    };
    let result = execute_local_bridge_bundle_rollback_action(action, &import_root, 2_000).unwrap();

    assert_eq!(result.request_id, "bridge-rollback-1");
    assert_eq!(result.action_kind, "bundle.rollback");
    assert_eq!(result.status, "completed");
    assert_eq!(result.lifecycle_status.as_deref(), Some("succeeded"));
    assert_eq!(result.bundle_id.as_deref(), Some("bundle_1234567890"));
    assert_eq!(result.rolled_back_file_count, 2);
    assert!(result.rollback_blocking_reason.is_none());
    assert!(!std::path::Path::new(imported.import_path.as_deref().unwrap()).exists());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_rollback_execution_records_blocking_reason() {
    let dir = unique_bundle_temp_dir("local-bridge-rollback-blocked");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    create_desktop_test_bundle(&staging_root, "bundle_1234567890", "bundle_1234567890");
    let imported =
        import_staged_bundle_at(&staging_root, &import_root, "bundle_1234567890").unwrap();
    fs::remove_file(
        std::path::Path::new(imported.import_path.as_deref().unwrap()).join("content.bin"),
    )
    .unwrap();
    let action = LocalBridgePendingRollbackBundleImportAction {
        request_id: "bridge-rollback-blocked-1".to_string(),
        client: LocalBridgeClientIdentity {
            client_id: "local-agent-app".to_string(),
            display_name: "Local Agent App".to_string(),
            app_kind: Some("agent".to_string()),
        },
        bundle_id: "bundle_1234567890".to_string(),
        requested_at_ms: 1_500,
    };
    let result = execute_local_bridge_bundle_rollback_action(action, &import_root, 2_000).unwrap();

    assert_eq!(result.request_id, "bridge-rollback-blocked-1");
    assert_eq!(result.action_kind, "bundle.rollback");
    assert_eq!(result.status, "failed");
    assert_eq!(result.lifecycle_status.as_deref(), Some("failed"));
    assert_eq!(result.reason.as_deref(), Some("bundle_rollback_blocked"));
    assert_eq!(
        result.rollback_blocking_reason.as_deref(),
        Some("imported_file_missing")
    );
    assert_eq!(result.rolled_back_file_count, 0);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_import_execution_rejects_expected_type_mismatch() {
    let dir = unique_bundle_temp_dir("local-bridge-import-execution-type-mismatch");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    create_desktop_test_bundle(&staging_root, "bundle_1234567890", "bundle_1234567890");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .pending_actions
        .lock()
        .unwrap()
        .push(LocalBridgePendingAction::ImportBundle(
            LocalBridgePendingImportBundleAction {
                request_id: "bridge-import-type".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                staged_bundle_id: "bundle_1234567890".to_string(),
                expected_bundle_type: Some(BundleType::Workspace),
                conflict_strategy: "reject".to_string(),
                requested_at_ms: 1_500,
            },
        ));

    let result =
        execute_next_local_bridge_bundle_import_at(&runtime, &staging_root, &import_root, 2_000)
            .unwrap()
            .expect("pending bundle.import action should be consumed");
    let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

    assert_eq!(result.status, "failed");
    assert_eq!(result.reason.as_deref(), Some("bundle_type_mismatch"));
    assert_eq!(result.bundle_id.as_deref(), Some("bundle_1234567890"));
    assert_eq!(result.bundle_type.as_deref(), Some("skill"));
    assert!(runtime.pending_actions.lock().unwrap().is_empty());
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].request_id, "bridge-import-type");
    assert_eq!(results[0].status, "failed");
    assert_eq!(results[0].reason.as_deref(), Some("bundle_type_mismatch"));
    assert!(!import_root.join("bundle_1234567890").exists());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_import_execution_reports_name_conflict() {
    let dir = unique_bundle_temp_dir("local-bridge-import-execution-conflict");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    create_desktop_test_bundle(&staging_root, "bundle_1234567890", "bundle_1234567890");
    fs::create_dir_all(import_root.join("bundle_1234567890")).unwrap();
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .pending_actions
        .lock()
        .unwrap()
        .push(LocalBridgePendingAction::ImportBundle(
            LocalBridgePendingImportBundleAction {
                request_id: "bridge-import-conflict".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                staged_bundle_id: "bundle_1234567890".to_string(),
                expected_bundle_type: Some(BundleType::Skill),
                conflict_strategy: "reject".to_string(),
                requested_at_ms: 1_500,
            },
        ));

    let result =
        execute_next_local_bridge_bundle_import_at(&runtime, &staging_root, &import_root, 2_000)
            .unwrap()
            .expect("pending bundle.import action should be consumed");

    assert_eq!(result.status, "failed");
    assert_eq!(result.reason.as_deref(), Some("bundle_import_conflict"));
    assert_eq!(result.lifecycle_status.as_deref(), Some("conflict"));
    assert!(!import_root.join("bundle_1234567890.importing").exists());
    let events = runtime.events.lock().unwrap();
    let last_status = events.iter().rev().find_map(|event| match event {
        nekolink_protocol::LocalBridgeEvent::ActionUpdated(event) => Some(event.status),
        _ => None,
    });
    assert_eq!(
        last_status,
        Some(nekolink_protocol::LocalBridgeActionLifecycleStatus::Conflict)
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_import_execution_skips_non_import_queue_head() {
    let dir = unique_bundle_temp_dir("local-bridge-import-execution-skip");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .pending_actions
        .lock()
        .unwrap()
        .push(LocalBridgePendingAction::SendBundle(
            LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-1".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: Some("device-a".to_string()),
                bundle_root: "/tmp/exported/bundle".to_string(),
                bundle_type: BundleType::Skill,
                require_trusted_device: true,
                requested_at_ms: 1_500,
            },
        ));

    let result =
        execute_next_local_bridge_bundle_import_at(&runtime, &staging_root, &import_root, 2_000)
            .unwrap();
    let actions = list_local_bridge_pending_actions_at(&runtime).unwrap();
    let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

    assert!(result.is_none());
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].request_id, "bridge-send-1");
    assert!(results.is_empty());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_send_preflight_accepts_valid_trusted_target() {
    let dir = unique_bundle_temp_dir("local-bridge-send-preflight-ready");
    let bundle_root = create_desktop_test_bundle(&dir, "bundle", "bundle_preflight_ready");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .pending_actions
        .lock()
        .unwrap()
        .push(LocalBridgePendingAction::SendBundle(
            LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-1".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: Some("device-a".to_string()),
                bundle_root: bundle_root.display().to_string(),
                bundle_type: BundleType::Skill,
                require_trusted_device: true,
                requested_at_ms: 1_500,
            },
        ));
    let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];

    let result = preflight_next_local_bridge_bundle_send_at(&runtime, &trusted, 2_000).unwrap();

    assert_eq!(result.status, "ready");
    assert_eq!(result.request_id.as_deref(), Some("bridge-send-1"));
    assert_eq!(result.bundle_id.as_deref(), Some("bundle_preflight_ready"));
    assert_eq!(result.bundle_type.as_deref(), Some("skill"));
    assert_eq!(result.target_device_id.as_deref(), Some("device-a"));
    assert_eq!(result.claimed_at_ms, Some(2_000));
    assert!(result.message.contains("ready"));
    assert!(runtime.pending_actions.lock().unwrap().is_empty());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_send_preflight_rejects_missing_bundle_root() {
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .pending_actions
        .lock()
        .unwrap()
        .push(LocalBridgePendingAction::SendBundle(
            LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-missing".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: Some("device-a".to_string()),
                bundle_root: "/tmp/nekodrop-missing-bundle-root".to_string(),
                bundle_type: BundleType::Skill,
                require_trusted_device: false,
                requested_at_ms: 1_500,
            },
        ));

    let result = preflight_next_local_bridge_bundle_send_at(&runtime, &[], 2_000).unwrap();

    assert_eq!(result.status, "failed_preflight");
    assert_eq!(result.request_id.as_deref(), Some("bridge-send-missing"));
    assert_eq!(result.reason.as_deref(), Some("bundle_root_missing"));
    assert!(result.message.contains("bundle_root"));
    assert!(runtime.pending_actions.lock().unwrap().is_empty());
}

#[test]
fn local_bridge_bundle_send_preflight_rejects_bundle_type_mismatch() {
    let dir = unique_bundle_temp_dir("local-bridge-send-preflight-type-mismatch");
    let bundle_root = create_desktop_test_bundle(&dir, "bundle", "bundle_preflight_type");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .pending_actions
        .lock()
        .unwrap()
        .push(LocalBridgePendingAction::SendBundle(
            LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-type".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: None,
                bundle_root: bundle_root.display().to_string(),
                bundle_type: BundleType::Workspace,
                require_trusted_device: false,
                requested_at_ms: 1_500,
            },
        ));

    let result = preflight_next_local_bridge_bundle_send_at(&runtime, &[], 2_000).unwrap();

    assert_eq!(result.status, "failed_preflight");
    assert_eq!(result.reason.as_deref(), Some("bundle_type_mismatch"));
    assert_eq!(result.bundle_id.as_deref(), Some("bundle_preflight_type"));
    assert_eq!(result.bundle_type.as_deref(), Some("skill"));
    assert!(runtime.pending_actions.lock().unwrap().is_empty());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_send_preflight_rejects_sensitive_bundles_without_trusted_session_target() {
    let dir = unique_bundle_temp_dir("local-bridge-send-preflight-sensitive-policy");
    let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];

    for bundle_type in [
        BundleType::Skill,
        BundleType::Session,
        BundleType::Workspace,
        BundleType::AgentProfile,
    ] {
        let label = bundle_type_label(bundle_type);
        let bundle_id = format!("bundle_preflight_sensitive_{label}");
        let bundle_root = create_desktop_test_bundle_with_type(
            &dir,
            format!("bundle_{label}").as_str(),
            &bundle_id,
            bundle_type,
        );
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::SendBundle(
                LocalBridgePendingSendBundleAction {
                    request_id: format!("bridge-send-sensitive-{label}"),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    target_device_id: Some("device-a".to_string()),
                    bundle_root: bundle_root.display().to_string(),
                    bundle_type,
                    require_trusted_device: false,
                    requested_at_ms: 1_500,
                },
            ));

        let result = preflight_next_local_bridge_bundle_send_at(&runtime, &trusted, 2_000).unwrap();

        assert_eq!(result.status, "failed_preflight");
        assert_eq!(
            result.reason.as_deref(),
            Some("sensitive_bundle_requires_trusted_device")
        );
        assert_eq!(result.bundle_id.as_deref(), Some(bundle_id.as_str()));
        assert_eq!(result.bundle_type.as_deref(), Some(label));
        assert!(runtime.pending_actions.lock().unwrap().is_empty());
    }

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_send_preflight_rejects_missing_trusted_target() {
    let dir = unique_bundle_temp_dir("local-bridge-send-preflight-untrusted-target");
    let bundle_root = create_desktop_test_bundle(&dir, "bundle", "bundle_preflight_trusted");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .pending_actions
        .lock()
        .unwrap()
        .push(LocalBridgePendingAction::SendBundle(
            LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-untrusted".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: Some("device-a".to_string()),
                bundle_root: bundle_root.display().to_string(),
                bundle_type: BundleType::Skill,
                require_trusted_device: true,
                requested_at_ms: 1_500,
            },
        ));

    let result = preflight_next_local_bridge_bundle_send_at(&runtime, &[], 2_000).unwrap();

    assert_eq!(result.status, "failed_preflight");
    assert_eq!(result.reason.as_deref(), Some("trusted_target_missing"));
    assert_eq!(result.target_device_id.as_deref(), Some("device-a"));
    assert!(runtime.pending_actions.lock().unwrap().is_empty());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_worker_executes_bundle_send_action_and_records_result() {
    let dir = unique_bundle_temp_dir("local-bridge-worker-send");
    let bundle_root = create_desktop_test_bundle(&dir, "bundle", "bundle_worker_send");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .pending_actions
        .lock()
        .unwrap()
        .push(LocalBridgePendingAction::SendBundle(
            LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-worker".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: Some("device-a".to_string()),
                bundle_root: bundle_root.display().to_string(),
                bundle_type: BundleType::Skill,
                require_trusted_device: true,
                requested_at_ms: 1_500,
            },
        ));
    let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];
    let mut sent_bundle_root = None;

    let result = execute_next_local_bridge_bundle_send_with(&runtime, &trusted, 2_000, |action| {
        sent_bundle_root = Some(action.bundle_root.clone());
        Ok(())
    })
    .unwrap()
    .expect("worker should execute queued bundle.send");
    let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

    assert_eq!(
        sent_bundle_root.as_deref(),
        Some(bundle_root.to_str().unwrap())
    );
    assert_eq!(result.request_id, "bridge-send-worker");
    assert_eq!(result.status, "completed");
    assert_eq!(result.bundle_id.as_deref(), Some("bundle_worker_send"));
    assert!(result.bundle_root.is_none());
    assert!(runtime.pending_actions.lock().unwrap().is_empty());
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].status, "completed");

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_send_preflight_records_result_without_sensitive_path() {
    let dir = unique_bundle_temp_dir("local-bridge-send-preflight-result-history");
    let bundle_root = create_desktop_test_bundle_with_type(
        &dir,
        "bundle",
        "bundle_preflight_result",
        BundleType::ConfigSnapshot,
    );
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .pending_actions
        .lock()
        .unwrap()
        .push(LocalBridgePendingAction::SendBundle(
            LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-result".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: None,
                bundle_root: bundle_root.display().to_string(),
                bundle_type: BundleType::ConfigSnapshot,
                require_trusted_device: false,
                requested_at_ms: 1_500,
            },
        ));

    let preflight = preflight_next_local_bridge_bundle_send_at(&runtime, &[], 2_000).unwrap();
    let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

    assert_eq!(preflight.status, "ready");
    assert_eq!(preflight.bundle_type.as_deref(), Some("config_snapshot"));
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].request_id, "bridge-send-result");
    assert_eq!(results[0].action_kind, "bundle.send");
    assert_eq!(results[0].status, "ready");
    assert_eq!(
        results[0].bundle_id.as_deref(),
        Some("bundle_preflight_result")
    );
    assert!(results[0].bundle_root.is_none());
    assert_eq!(results[0].claimed_at_ms, 2_000);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_send_preflight_records_failure_reason() {
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .pending_actions
        .lock()
        .unwrap()
        .push(LocalBridgePendingAction::SendBundle(
            LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-failed-result".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: Some("device-a".to_string()),
                bundle_root: "/tmp/nekodrop-missing-bundle-result".to_string(),
                bundle_type: BundleType::Skill,
                require_trusted_device: false,
                requested_at_ms: 1_500,
            },
        ));

    let preflight = preflight_next_local_bridge_bundle_send_at(&runtime, &[], 2_000).unwrap();
    let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

    assert_eq!(preflight.status, "failed_preflight");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].request_id, "bridge-send-failed-result");
    assert_eq!(results[0].status, "failed_preflight");
    assert_eq!(results[0].reason.as_deref(), Some("bundle_root_missing"));
    assert!(results[0].message.contains("bundle_root"));
    assert!(results[0].bundle_root.is_none());
}

#[test]
fn local_bridge_bundle_send_execution_records_completed_result_without_sensitive_path() {
    let dir = unique_bundle_temp_dir("local-bridge-send-execution-completed");
    let bundle_root = create_desktop_test_bundle(&dir, "bundle", "bundle_send_execution");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .pending_actions
        .lock()
        .unwrap()
        .push(LocalBridgePendingAction::SendBundle(
            LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-execute".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: Some("device-a".to_string()),
                bundle_root: bundle_root.display().to_string(),
                bundle_type: BundleType::Skill,
                require_trusted_device: true,
                requested_at_ms: 1_500,
            },
        ));
    let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];

    let result = execute_next_local_bridge_bundle_send_with(&runtime, &trusted, 2_000, |action| {
        assert_eq!(action.request_id, "bridge-send-execute");
        assert_eq!(action.bundle_root, bundle_root.display().to_string());
        Ok(())
    })
    .unwrap()
    .expect("pending bundle.send action should be executed");
    let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

    assert_eq!(result.request_id, "bridge-send-execute");
    assert_eq!(result.action_kind, "bundle.send");
    assert_eq!(result.status, "completed");
    assert_eq!(result.lifecycle_status.as_deref(), Some("succeeded"));
    assert_eq!(result.bundle_id.as_deref(), Some("bundle_send_execution"));
    assert_eq!(result.bundle_type.as_deref(), Some("skill"));
    assert_eq!(result.target_device_id.as_deref(), Some("device-a"));
    assert_eq!(result.requested_at_ms, 1_500);
    assert_eq!(result.claimed_at_ms, 2_000);
    assert!(result.bundle_root.is_none());
    assert!(runtime.pending_actions.lock().unwrap().is_empty());
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].request_id, "bridge-send-execute");
    assert_eq!(results[0].status, "completed");
    assert_eq!(results[0].lifecycle_status.as_deref(), Some("succeeded"));
    assert!(results[0].bundle_root.is_none());
    let events = runtime.events.lock().unwrap();
    let statuses: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            nekolink_protocol::LocalBridgeEvent::ActionUpdated(event) => Some(event.status),
            _ => None,
        })
        .collect();
    assert_eq!(
        statuses,
        vec![
            nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
            nekolink_protocol::LocalBridgeActionLifecycleStatus::Succeeded,
        ]
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_send_execution_requires_target_device() {
    let dir = unique_bundle_temp_dir("local-bridge-send-execution-target-required");
    let bundle_root = create_desktop_test_bundle(&dir, "bundle", "bundle_send_no_target");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .pending_actions
        .lock()
        .unwrap()
        .push(LocalBridgePendingAction::SendBundle(
            LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-no-target".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: None,
                bundle_root: bundle_root.display().to_string(),
                bundle_type: BundleType::Skill,
                require_trusted_device: false,
                requested_at_ms: 1_500,
            },
        ));
    let mut called = false;

    let result = execute_next_local_bridge_bundle_send_with(&runtime, &[], 2_000, |_| {
        called = true;
        Ok(())
    })
    .unwrap()
    .expect("pending bundle.send action should be consumed");
    let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

    assert!(!called);
    assert_eq!(result.status, "failed");
    assert_eq!(result.lifecycle_status.as_deref(), Some("failed"));
    assert_eq!(
        result.reason.as_deref(),
        Some("sensitive_bundle_requires_trusted_device")
    );
    assert_eq!(result.bundle_id.as_deref(), Some("bundle_send_no_target"));
    assert_eq!(result.bundle_type.as_deref(), Some("skill"));
    assert!(result.bundle_root.is_none());
    assert!(runtime.pending_actions.lock().unwrap().is_empty());
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].request_id, "bridge-send-no-target");
    assert_eq!(results[0].status, "failed");
    assert_eq!(results[0].lifecycle_status.as_deref(), Some("failed"));
    assert_eq!(
        results[0].reason.as_deref(),
        Some("sensitive_bundle_requires_trusted_device")
    );
    assert!(results[0].bundle_root.is_none());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn authorized_local_bridge_client_can_poll_bundle_send_preflight_events() {
    let runtime = LocalBridgeRuntimeState::default();
    let dir = unique_bundle_temp_dir("local-bridge-send-preflight-event");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_000,
            5_000,
        ));
    runtime
        .pending_actions
        .lock()
        .unwrap()
        .push(LocalBridgePendingAction::SendBundle(
            LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-event".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: Some("device-a".to_string()),
                bundle_root: "/tmp/nekodrop-missing-bundle-event".to_string(),
                bundle_type: BundleType::Skill,
                require_trusted_device: false,
                requested_at_ms: 1_500,
            },
        ));
    preflight_next_local_bridge_bundle_send_at(&runtime, &[], 2_000).unwrap();
    let poll_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-send",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "after_event_id": null,
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &poll_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.events.len(), 1);
    assert_eq!(
        response.events[0]["kind"].as_str(),
        Some("bundle.send.preflight")
    );
    assert_eq!(
        response.events[0]["payload"]["request_id"].as_str(),
        Some("bridge-send-event")
    );
    assert_eq!(
        response.events[0]["payload"]["status"].as_str(),
        Some("failed_preflight")
    );
    assert!(response.events[0]["payload"].get("bundle_root").is_none());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn authorized_local_bridge_client_can_poll_action_updated_events_by_scope() {
    let dir = unique_bundle_temp_dir("local-bridge-action-updated-poll");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime.authorizations.lock().unwrap().extend([
        local_bridge_authorization(
            "sender-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_000,
            5_000,
        ),
        local_bridge_authorization(
            "importer-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            5_000,
        ),
    ]);
    push_local_bridge_runtime_event(
        &runtime,
        nekolink_protocol::LocalBridgeEvent::ActionUpdated(
            nekolink_protocol::LocalBridgeActionUpdatedEvent {
                event_id: "bridge-action-send-running".to_string(),
                request_id: "bridge-send-1".to_string(),
                action_kind: "bundle.send".to_string(),
                client_id: "sender-app".to_string(),
                client_app_kind: Some("agent".to_string()),
                status: nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                reason: None,
                message: "send running".to_string(),
                bundle_id: Some("bundle_send".to_string()),
                bundle_type: Some(BundleType::Skill),
                target_device_id: Some("device-a".to_string()),
                updated_at_ms: 2_000,
            },
        ),
    )
    .unwrap();
    push_local_bridge_runtime_event(
        &runtime,
        nekolink_protocol::LocalBridgeEvent::ActionUpdated(
            nekolink_protocol::LocalBridgeActionUpdatedEvent {
                event_id: "bridge-action-import-running".to_string(),
                request_id: "bridge-import-1".to_string(),
                action_kind: "bundle.import".to_string(),
                client_id: "importer-app".to_string(),
                client_app_kind: Some("agent".to_string()),
                status: nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                reason: None,
                message: "import running".to_string(),
                bundle_id: Some("bundle_1234567890".to_string()),
                bundle_type: Some(BundleType::Skill),
                target_device_id: None,
                updated_at_ms: 2_100,
            },
        ),
    )
    .unwrap();
    push_local_bridge_runtime_event(
        &runtime,
        nekolink_protocol::LocalBridgeEvent::ActionUpdated(
            nekolink_protocol::LocalBridgeActionUpdatedEvent {
                event_id: "bridge-action-send-automation-running".to_string(),
                request_id: "bridge-send-automation".to_string(),
                action_kind: "bundle.send".to_string(),
                client_id: "sender-app".to_string(),
                client_app_kind: Some("automation".to_string()),
                status: nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                reason: None,
                message: "automation send running".to_string(),
                bundle_id: Some("bundle_automation".to_string()),
                bundle_type: Some(BundleType::Skill),
                target_device_id: Some("device-a".to_string()),
                updated_at_ms: 2_200,
            },
        ),
    )
    .unwrap();
    let poll_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-action",
            "client": {
                "client_id": "sender-app",
                "display_name": "Sender App",
                "app_kind": "agent"
            },
            "after_event_id": null,
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &poll_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.events.len(), 1);
    assert_eq!(response.events[0]["kind"].as_str(), Some("action.updated"));
    assert_eq!(
        response.events[0]["payload"]["request_id"].as_str(),
        Some("bridge-send-1")
    );
    assert_eq!(
        response.events[0]["payload"]["status"].as_str(),
        Some("running")
    );
    assert!(response.events[0]["payload"].get("bundle_root").is_none());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_event_poll_treats_hidden_cursor_as_missing() {
    let dir = unique_bundle_temp_dir("local-bridge-hidden-event-cursor");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime.authorizations.lock().unwrap().extend([
        local_bridge_authorization(
            "sender-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_000,
            5_000,
        ),
        local_bridge_authorization(
            "importer-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            5_000,
        ),
    ]);
    push_local_bridge_runtime_event(
        &runtime,
        nekolink_protocol::LocalBridgeEvent::ActionUpdated(
            nekolink_protocol::LocalBridgeActionUpdatedEvent {
                event_id: "bridge-action-import-hidden".to_string(),
                request_id: "bridge-import-1".to_string(),
                action_kind: "bundle.import".to_string(),
                client_id: "importer-app".to_string(),
                client_app_kind: Some("agent".to_string()),
                status: nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                reason: None,
                message: "import running".to_string(),
                bundle_id: Some("bundle_1234567890".to_string()),
                bundle_type: Some(BundleType::Skill),
                target_device_id: None,
                updated_at_ms: 2_000,
            },
        ),
    )
    .unwrap();
    push_local_bridge_runtime_event(
        &runtime,
        nekolink_protocol::LocalBridgeEvent::ActionUpdated(
            nekolink_protocol::LocalBridgeActionUpdatedEvent {
                event_id: "bridge-action-send-visible".to_string(),
                request_id: "bridge-send-1".to_string(),
                action_kind: "bundle.send".to_string(),
                client_id: "sender-app".to_string(),
                client_app_kind: Some("agent".to_string()),
                status: nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                reason: None,
                message: "send running".to_string(),
                bundle_id: Some("bundle_send".to_string()),
                bundle_type: Some(BundleType::Skill),
                target_device_id: Some("device-a".to_string()),
                updated_at_ms: 2_100,
            },
        ),
    )
    .unwrap();
    let poll_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-hidden-cursor",
            "client": {
                "client_id": "sender-app",
                "display_name": "Sender App",
                "app_kind": "agent"
            },
            "after_event_id": "bridge-action-import-hidden",
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &poll_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert!(response.events.is_empty());
    assert_eq!(response.events_cursor_state, "missing");
    assert_eq!(response.events_last_id, None);
    assert_eq!(response.events_next_after_id, None);
    assert!(!response.events_has_more);
    assert_eq!(
        response.events_visible_first_id.as_deref(),
        Some("bridge-action-send-visible")
    );
    assert_eq!(
        response.events_visible_last_id.as_deref(),
        Some("bridge-action-send-visible")
    );
    assert_eq!(response.events_visible_count, 1);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_event_poll_can_filter_action_updates_by_request_id() {
    let dir = unique_bundle_temp_dir("local-bridge-action-event-filter");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "sender-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_000,
            5_000,
        ));
    push_local_bridge_runtime_event(
        &runtime,
        nekolink_protocol::LocalBridgeEvent::ActionUpdated(
            nekolink_protocol::LocalBridgeActionUpdatedEvent {
                event_id: "bridge-action-send-a-running".to_string(),
                request_id: "bridge-send-a".to_string(),
                action_kind: "bundle.send".to_string(),
                client_id: "sender-app".to_string(),
                client_app_kind: Some("agent".to_string()),
                status: nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                reason: None,
                message: "send a running".to_string(),
                bundle_id: Some("bundle_a".to_string()),
                bundle_type: Some(BundleType::Skill),
                target_device_id: Some("device-a".to_string()),
                updated_at_ms: 2_000,
            },
        ),
    )
    .unwrap();
    push_local_bridge_runtime_event(
        &runtime,
        nekolink_protocol::LocalBridgeEvent::TransferUpdated(
            nekolink_protocol::LocalBridgeTransferUpdatedEvent {
                event_id: "bridge-transfer-noise".to_string(),
                transfer_id: "transfer-1".to_string(),
                phase: nekolink_protocol::LocalBridgeTransferPhase::Sending,
                bytes_transferred: 10,
                total_bytes: 100,
            },
        ),
    )
    .unwrap();
    push_local_bridge_runtime_event(
        &runtime,
        nekolink_protocol::LocalBridgeEvent::ActionUpdated(
            nekolink_protocol::LocalBridgeActionUpdatedEvent {
                event_id: "bridge-action-send-b-running".to_string(),
                request_id: "bridge-send-b".to_string(),
                action_kind: "bundle.send".to_string(),
                client_id: "sender-app".to_string(),
                client_app_kind: Some("agent".to_string()),
                status: nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                reason: None,
                message: "send b running".to_string(),
                bundle_id: Some("bundle_b".to_string()),
                bundle_type: Some(BundleType::Skill),
                target_device_id: Some("device-b".to_string()),
                updated_at_ms: 2_100,
            },
        ),
    )
    .unwrap();
    let poll_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-filter-action",
            "client": {
                "client_id": "sender-app",
                "display_name": "Sender App",
                "app_kind": "agent"
            },
            "after_event_id": null,
            "action_request_id": "bridge-send-b",
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &poll_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.events.len(), 1);
    assert_eq!(response.events[0]["kind"].as_str(), Some("action.updated"));
    assert_eq!(
        response.events[0]["payload"]["request_id"].as_str(),
        Some("bridge-send-b")
    );
    assert_eq!(
        response.events_last_id.as_deref(),
        Some("bridge-action-send-b-running")
    );
    assert_eq!(response.events_cursor_state, "ok");
    assert_eq!(
        response.events_visible_first_id.as_deref(),
        Some("bridge-action-send-b-running")
    );
    assert_eq!(
        response.events_visible_last_id.as_deref(),
        Some("bridge-action-send-b-running")
    );
    assert_eq!(response.events_visible_count, 1);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn authorized_local_bridge_client_can_poll_runtime_events() {
    let dir = unique_bundle_temp_dir("local-bridge-events-poll");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleRead],
            1_000,
            5_000,
        ));
    push_local_bridge_runtime_event(
        &runtime,
        nekolink_protocol::LocalBridgeEvent::BundleReceived(
            nekolink_protocol::LocalBridgeBundleReceivedEvent {
                event_id: "bridge-event-1".to_string(),
                transfer_id: "transfer-1".to_string(),
                bundle_id: "bundle_1234567890".to_string(),
                bundle_type: BundleType::Skill,
                display_name: "voice_transcribe".to_string(),
                source_app: "Generic Agent App".to_string(),
                file_count: 2,
                total_bytes: 28,
                import_allowed: true,
            },
        ),
    )
    .unwrap();
    let poll_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-1",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "after_event_id": null,
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &poll_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.security_state, "authorized");
    assert_eq!(response.events.len(), 1);
    assert_eq!(
        response.events[0]["payload"]["event_id"].as_str(),
        Some("bridge-event-1")
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn authorized_local_bridge_client_can_poll_action_results() {
    let dir = unique_bundle_temp_dir("local-bridge-action-results-poll");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            5_000,
        ));
    runtime
        .pending_action_results
        .lock()
        .unwrap()
        .push(LocalBridgePendingActionResult {
            request_id: "bridge-import-1".to_string(),
            action_kind: "bundle.import".to_string(),
            client_id: "local-agent-app".to_string(),
            client_display_name: "Local Agent App".to_string(),
            client_app_kind: Some("agent".to_string()),
            status: "completed".to_string(),
            lifecycle_status: Some("succeeded".to_string()),
            reason: None,
            message: "local bridge staged bundle was imported".to_string(),
            bundle_id: Some("bundle_1234567890".to_string()),
            bundle_type: Some("skill".to_string()),
            bundle_root: None,
            target_device_id: None,
            require_trusted_device: None,
            conflict_strategy: None,
            skipped_file_count: 0,
            import_receipt_path: Some("/tmp/private/receipt.json".to_string()),
            rollback_file_count: 2,
            rollback_blocking_reason: None,
            rolled_back_file_count: 0,
            requested_at_ms: 1_500,
            claimed_at_ms: 2_000,
        });
    let request = serde_json::json!({
        "kind": "actions.results",
        "payload": {
            "request_id": "bridge-results-1",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "after_claimed_at_ms": null,
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.security_state, "authorized");
    assert_eq!(response.events.len(), 0);
    assert!(response.staged_bundles.is_empty());
    assert!(response.transfer_status.is_none());
    assert_eq!(response.action_results.len(), 1);
    assert_eq!(response.action_results[0].request_id, "bridge-import-1");
    assert_eq!(response.action_results[0].action_kind, "bundle.import");
    assert_eq!(response.action_results[0].status, "completed");
    assert_eq!(
        response.action_results[0].bundle_id.as_deref(),
        Some("bundle_1234567890")
    );
    assert!(response.action_results[0].bundle_root.is_none());
    assert!(response.action_results[0].import_receipt_path.is_none());
    assert!(response.action_results[0].has_import_receipt);
    assert_eq!(response.action_results[0].rollback_file_count, 2);
    assert!(response.action_results[0].can_request_rollback);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_action_results_are_scoped_to_client_and_permission() {
    let dir = unique_bundle_temp_dir("local-bridge-action-results-scope");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            5_000,
        ));
    runtime.pending_action_results.lock().unwrap().extend([
        LocalBridgePendingActionResult {
            request_id: "bridge-import-1".to_string(),
            action_kind: "bundle.import".to_string(),
            client_id: "local-agent-app".to_string(),
            client_display_name: "Local Agent App".to_string(),
            client_app_kind: Some("agent".to_string()),
            status: "completed".to_string(),
            lifecycle_status: Some("succeeded".to_string()),
            reason: None,
            message: "imported".to_string(),
            bundle_id: Some("bundle_1234567890".to_string()),
            bundle_type: Some("skill".to_string()),
            bundle_root: None,
            target_device_id: None,
            require_trusted_device: None,
            conflict_strategy: None,
            skipped_file_count: 0,
            import_receipt_path: None,
            rollback_file_count: 0,
            rollback_blocking_reason: None,
            rolled_back_file_count: 0,
            requested_at_ms: 1_500,
            claimed_at_ms: 2_000,
        },
        LocalBridgePendingActionResult {
            request_id: "bridge-send-1".to_string(),
            action_kind: "bundle.send".to_string(),
            client_id: "local-agent-app".to_string(),
            client_display_name: "Local Agent App".to_string(),
            client_app_kind: Some("agent".to_string()),
            status: "ready".to_string(),
            lifecycle_status: None,
            reason: None,
            message: "send ready".to_string(),
            bundle_id: Some("bundle_send".to_string()),
            bundle_type: Some("skill".to_string()),
            bundle_root: Some("/tmp/private/bundle".to_string()),
            target_device_id: Some("device-a".to_string()),
            require_trusted_device: Some(true),
            conflict_strategy: None,
            skipped_file_count: 0,
            import_receipt_path: None,
            rollback_file_count: 0,
            rollback_blocking_reason: None,
            rolled_back_file_count: 0,
            requested_at_ms: 1_600,
            claimed_at_ms: 2_100,
        },
        LocalBridgePendingActionResult {
            request_id: "bridge-import-automation".to_string(),
            action_kind: "bundle.import".to_string(),
            client_id: "local-agent-app".to_string(),
            client_display_name: "Local Automation App".to_string(),
            client_app_kind: Some("automation".to_string()),
            status: "completed".to_string(),
            lifecycle_status: Some("succeeded".to_string()),
            reason: None,
            message: "automation imported".to_string(),
            bundle_id: Some("bundle_automation".to_string()),
            bundle_type: Some("skill".to_string()),
            bundle_root: None,
            target_device_id: None,
            require_trusted_device: None,
            conflict_strategy: None,
            skipped_file_count: 0,
            import_receipt_path: None,
            rollback_file_count: 0,
            rollback_blocking_reason: None,
            rolled_back_file_count: 0,
            requested_at_ms: 1_650,
            claimed_at_ms: 2_150,
        },
        LocalBridgePendingActionResult {
            request_id: "bridge-import-other".to_string(),
            action_kind: "bundle.import".to_string(),
            client_id: "other-app".to_string(),
            client_display_name: "Other App".to_string(),
            client_app_kind: Some("agent".to_string()),
            status: "completed".to_string(),
            lifecycle_status: Some("succeeded".to_string()),
            reason: None,
            message: "other imported".to_string(),
            bundle_id: Some("bundle_other".to_string()),
            bundle_type: Some("skill".to_string()),
            bundle_root: None,
            target_device_id: None,
            require_trusted_device: None,
            conflict_strategy: None,
            skipped_file_count: 0,
            import_receipt_path: None,
            rollback_file_count: 0,
            rollback_blocking_reason: None,
            rolled_back_file_count: 0,
            requested_at_ms: 1_700,
            claimed_at_ms: 2_200,
        },
        LocalBridgePendingActionResult {
            request_id: "bridge-rollback-1".to_string(),
            action_kind: "bundle.rollback".to_string(),
            client_id: "local-agent-app".to_string(),
            client_display_name: "Local Agent App".to_string(),
            client_app_kind: Some("agent".to_string()),
            status: "completed".to_string(),
            lifecycle_status: Some("succeeded".to_string()),
            reason: None,
            message: "rollback completed".to_string(),
            bundle_id: Some("bundle_1234567890".to_string()),
            bundle_type: None,
            bundle_root: None,
            target_device_id: None,
            require_trusted_device: None,
            conflict_strategy: None,
            skipped_file_count: 0,
            import_receipt_path: None,
            rollback_file_count: 0,
            rollback_blocking_reason: None,
            rolled_back_file_count: 2,
            requested_at_ms: 1_800,
            claimed_at_ms: 2_300,
        },
    ]);
    let request = serde_json::json!({
        "kind": "actions.results",
        "payload": {
            "request_id": "bridge-results-scope",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "after_claimed_at_ms": 1_999,
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.action_results.len(), 2);
    assert_eq!(response.action_results[0].request_id, "bridge-import-1");
    assert!(response.action_results[0].bundle_root.is_none());
    assert_eq!(response.action_results[1].request_id, "bridge-rollback-1");
    assert_eq!(response.action_results[1].action_kind, "bundle.rollback");
    assert_eq!(response.action_results[1].rolled_back_file_count, 2);

    let exact_request = serde_json::json!({
        "kind": "actions.results",
        "payload": {
            "request_id": "bridge-results-exact",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "action_request_id": "bridge-rollback-1",
            "after_claimed_at_ms": null,
            "limit": 10
        }
    })
    .to_string();
    let exact_response = handle_local_bridge_request_with_runtime_at(
        &exact_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();

    assert_eq!(exact_response.status, "ok");
    assert_eq!(exact_response.action_results.len(), 1);
    assert_eq!(
        exact_response.action_results[0].request_id,
        "bridge-rollback-1"
    );
    assert_eq!(
        exact_response.action_results[0].action_kind,
        "bundle.rollback"
    );
    assert_eq!(exact_response.action_results[0].rolled_back_file_count, 2);

    let automation_exact_request = serde_json::json!({
        "kind": "actions.results",
        "payload": {
            "request_id": "bridge-results-automation",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "action_request_id": "bridge-import-automation",
            "after_claimed_at_ms": null,
            "limit": 10
        }
    })
    .to_string();
    let automation_exact_response = handle_local_bridge_request_with_runtime_at(
        &automation_exact_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();

    assert_eq!(automation_exact_response.status, "ok");
    assert!(automation_exact_response.action_results.is_empty());

    let send_without_scope_request = serde_json::json!({
        "kind": "actions.results",
        "payload": {
            "request_id": "bridge-results-send-without-scope",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "action_request_id": "bridge-send-1",
            "after_claimed_at_ms": null,
            "limit": 10
        }
    })
    .to_string();
    let send_without_scope_response = handle_local_bridge_request_with_runtime_at(
        &send_without_scope_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();

    assert_eq!(send_without_scope_response.status, "ok");
    assert!(send_without_scope_response.action_results.is_empty());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_action_results_returns_pending_queue_status_for_exact_lookup() {
    let dir = unique_bundle_temp_dir("local-bridge-action-results-pending-lookup");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime.authorizations.lock().unwrap().extend([
        local_bridge_authorization(
            "local-agent-app",
            &[
                LocalBridgePermissionScope::BundleSend,
                LocalBridgePermissionScope::BundleImportRequest,
            ],
            1_000,
            5_000,
        ),
        local_bridge_authorization(
            "other-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_000,
            5_000,
        ),
        local_bridge_authorization(
            "import-only-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            5_000,
        ),
    ]);
    runtime.pending_actions.lock().unwrap().extend([
        LocalBridgePendingAction::SendBundle(LocalBridgePendingSendBundleAction {
            request_id: "bridge-send-pending".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            target_device_id: Some("device-a".to_string()),
            bundle_root: "/tmp/private/bundle".to_string(),
            bundle_type: BundleType::Skill,
            require_trusted_device: true,
            requested_at_ms: 1_500,
        }),
        LocalBridgePendingAction::SendBundle(LocalBridgePendingSendBundleAction {
            request_id: "bridge-send-import-only".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "import-only-app".to_string(),
                display_name: "Import Only App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            target_device_id: Some("device-b".to_string()),
            bundle_root: "/tmp/private/import-only-bundle".to_string(),
            bundle_type: BundleType::Workspace,
            require_trusted_device: true,
            requested_at_ms: 1_600,
        }),
        LocalBridgePendingAction::ImportBundle(LocalBridgePendingImportBundleAction {
            request_id: "bridge-import-pending".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            staged_bundle_id: "bundle_pending_import".to_string(),
            expected_bundle_type: Some(BundleType::Workspace),
            conflict_strategy: "rename".to_string(),
            requested_at_ms: 1_700,
        }),
        LocalBridgePendingAction::RollbackBundleImport(
            LocalBridgePendingRollbackBundleImportAction {
                request_id: "bridge-rollback-pending".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                bundle_id: "bundle_pending_rollback".to_string(),
                requested_at_ms: 1_800,
            },
        ),
    ]);
    let request = serde_json::json!({
        "kind": "actions.results",
        "payload": {
            "request_id": "bridge-results-pending",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "action_request_id": "bridge-send-pending",
            "after_claimed_at_ms": null,
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.action_results.len(), 1);
    assert_eq!(response.action_results[0].request_id, "bridge-send-pending");
    assert_eq!(response.action_results[0].action_kind, "bundle.send");
    assert_eq!(response.action_results[0].status, "queued");
    assert_eq!(
        response.action_results[0].lifecycle_status.as_deref(),
        Some("queued")
    );
    assert_eq!(
        response.action_results[0].bundle_type.as_deref(),
        Some("skill")
    );
    assert_eq!(
        response.action_results[0].target_device_id.as_deref(),
        Some("device-a")
    );
    assert!(response.action_results[0].bundle_root.is_none());

    let import_request = serde_json::json!({
        "kind": "actions.results",
        "payload": {
            "request_id": "bridge-results-import-pending",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "action_request_id": "bridge-import-pending",
            "after_claimed_at_ms": null,
            "limit": 10
        }
    })
    .to_string();
    let import_response = handle_local_bridge_request_with_runtime_at(
        &import_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();
    assert_eq!(import_response.status, "ok");
    assert_eq!(import_response.action_results.len(), 1);
    assert_eq!(
        import_response.action_results[0].request_id,
        "bridge-import-pending"
    );
    assert_eq!(
        import_response.action_results[0].action_kind,
        "bundle.import"
    );
    assert_eq!(import_response.action_results[0].status, "queued");
    assert_eq!(
        import_response.action_results[0].bundle_id.as_deref(),
        Some("bundle_pending_import")
    );
    assert_eq!(
        import_response.action_results[0].bundle_type.as_deref(),
        Some("workspace")
    );
    assert_eq!(
        import_response.action_results[0]
            .conflict_strategy
            .as_deref(),
        Some("rename")
    );
    assert!(import_response.action_results[0]
        .import_receipt_path
        .is_none());

    let rollback_request = serde_json::json!({
        "kind": "actions.results",
        "payload": {
            "request_id": "bridge-results-rollback-pending",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "action_request_id": "bridge-rollback-pending",
            "after_claimed_at_ms": null,
            "limit": 10
        }
    })
    .to_string();
    let rollback_response = handle_local_bridge_request_with_runtime_at(
        &rollback_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();
    assert_eq!(rollback_response.status, "ok");
    assert_eq!(rollback_response.action_results.len(), 1);
    assert_eq!(
        rollback_response.action_results[0].request_id,
        "bridge-rollback-pending"
    );
    assert_eq!(
        rollback_response.action_results[0].action_kind,
        "bundle.rollback"
    );
    assert_eq!(rollback_response.action_results[0].status, "queued");
    assert_eq!(
        rollback_response.action_results[0].bundle_id.as_deref(),
        Some("bundle_pending_rollback")
    );

    let other_client_request = serde_json::json!({
        "kind": "actions.results",
        "payload": {
            "request_id": "bridge-results-other-client",
            "client": {
                "client_id": "other-app",
                "display_name": "Other App",
                "app_kind": "agent"
            },
            "action_request_id": "bridge-send-pending",
            "after_claimed_at_ms": null,
            "limit": 10
        }
    })
    .to_string();
    let other_client_response = handle_local_bridge_request_with_runtime_at(
        &other_client_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();
    assert_eq!(other_client_response.status, "ok");
    assert!(other_client_response.action_results.is_empty());

    let wrong_scope_request = serde_json::json!({
        "kind": "actions.results",
        "payload": {
            "request_id": "bridge-results-wrong-scope",
            "client": {
                "client_id": "import-only-app",
                "display_name": "Import Only App",
                "app_kind": "agent"
            },
            "action_request_id": "bridge-send-import-only",
            "after_claimed_at_ms": null,
            "limit": 10
        }
    })
    .to_string();
    let wrong_scope_response = handle_local_bridge_request_with_runtime_at(
        &wrong_scope_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();
    assert_eq!(wrong_scope_response.status, "ok");
    assert!(wrong_scope_response.action_results.is_empty());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_action_results_returns_running_lifecycle_status_for_exact_lookup() {
    let dir = unique_bundle_temp_dir("local-bridge-action-results-running-lookup");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            5_000,
        ));
    runtime
        .pending_action_results
        .lock()
        .unwrap()
        .push(LocalBridgePendingActionResult {
            request_id: "bridge-import-running".to_string(),
            action_kind: "bundle.import".to_string(),
            client_id: "local-agent-app".to_string(),
            client_display_name: "Local Agent App".to_string(),
            client_app_kind: Some("agent".to_string()),
            status: "running".to_string(),
            lifecycle_status: Some("running".to_string()),
            reason: None,
            message: "local bridge bundle import is running".to_string(),
            bundle_id: Some("bundle_1234567890".to_string()),
            bundle_type: Some("skill".to_string()),
            bundle_root: None,
            target_device_id: None,
            require_trusted_device: None,
            conflict_strategy: Some("reject".to_string()),
            skipped_file_count: 0,
            import_receipt_path: None,
            rollback_file_count: 0,
            rollback_blocking_reason: None,
            rolled_back_file_count: 0,
            requested_at_ms: 1_500,
            claimed_at_ms: 2_000,
        });
    let request = serde_json::json!({
        "kind": "actions.results",
        "payload": {
            "request_id": "bridge-results-running",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "action_request_id": "bridge-import-running",
            "after_claimed_at_ms": null,
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_500,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.action_results.len(), 1);
    assert_eq!(
        response.action_results[0].request_id,
        "bridge-import-running"
    );
    assert_eq!(response.action_results[0].status, "running");
    assert_eq!(
        response.action_results[0].lifecycle_status.as_deref(),
        Some("running")
    );
    assert!(response.action_results[0].import_receipt_path.is_none());
    assert!(!response.action_results[0].has_import_receipt);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_action_results_updates_only_scope_used_by_returned_results() {
    let dir = unique_bundle_temp_dir("local-bridge-action-results-last-used-scope");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime.authorizations.lock().unwrap().extend([
        local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_000,
            5_000,
        ),
        local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_100,
            5_000,
        ),
    ]);
    runtime
        .pending_action_results
        .lock()
        .unwrap()
        .push(LocalBridgePendingActionResult {
            request_id: "bridge-send-completed".to_string(),
            action_kind: "bundle.send".to_string(),
            client_id: "local-agent-app".to_string(),
            client_display_name: "Local Agent App".to_string(),
            client_app_kind: Some("agent".to_string()),
            status: "completed".to_string(),
            lifecycle_status: Some("succeeded".to_string()),
            reason: None,
            message: "send completed".to_string(),
            bundle_id: Some("bundle_1234567890".to_string()),
            bundle_type: Some("skill".to_string()),
            bundle_root: Some("/tmp/private/bundle".to_string()),
            target_device_id: Some("device-a".to_string()),
            require_trusted_device: Some(true),
            conflict_strategy: None,
            skipped_file_count: 0,
            import_receipt_path: None,
            rollback_file_count: 0,
            rollback_blocking_reason: None,
            rolled_back_file_count: 0,
            requested_at_ms: 1_500,
            claimed_at_ms: 1_900,
        });
    let request = serde_json::json!({
        "kind": "actions.results",
        "payload": {
            "request_id": "bridge-results-send",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "action_request_id": "bridge-send-completed",
            "after_claimed_at_ms": null,
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_000,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.action_results.len(), 1);
    let authorizations = runtime.authorizations.lock().unwrap();
    let last_used_for_scope = |scope| {
        authorizations
            .iter()
            .find(|record| record.scopes == vec![scope])
            .map(|record| record.last_used_at_ms)
            .unwrap()
    };
    assert_eq!(
        last_used_for_scope(LocalBridgePermissionScope::BundleSend),
        2_000
    );
    assert_eq!(
        last_used_for_scope(LocalBridgePermissionScope::BundleImportRequest),
        1_100
    );
    drop(authorizations);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_event_poll_returns_only_events_after_cursor() {
    let dir = unique_bundle_temp_dir("local-bridge-events-after");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::TransferStatusRead],
            1_000,
            5_000,
        ));
    push_local_bridge_runtime_event(
        &runtime,
        nekolink_protocol::LocalBridgeEvent::TransferUpdated(
            nekolink_protocol::LocalBridgeTransferUpdatedEvent {
                event_id: "bridge-event-1".to_string(),
                transfer_id: "transfer-1".to_string(),
                phase: nekolink_protocol::LocalBridgeTransferPhase::Sending,
                bytes_transferred: 10,
                total_bytes: 100,
            },
        ),
    )
    .unwrap();
    push_local_bridge_runtime_event(
        &runtime,
        nekolink_protocol::LocalBridgeEvent::TransferUpdated(
            nekolink_protocol::LocalBridgeTransferUpdatedEvent {
                event_id: "bridge-event-2".to_string(),
                transfer_id: "transfer-1".to_string(),
                phase: nekolink_protocol::LocalBridgeTransferPhase::Completed,
                bytes_transferred: 100,
                total_bytes: 100,
            },
        ),
    )
    .unwrap();
    let poll_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-2",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "after_event_id": "bridge-event-1",
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &poll_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();

    assert_eq!(response.events.len(), 1);
    assert_eq!(
        response.events[0]["payload"]["event_id"].as_str(),
        Some("bridge-event-2")
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_event_poll_returns_cursor_metadata_for_paging() {
    let dir = unique_bundle_temp_dir("local-bridge-events-cursor-page");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::TransferStatusRead],
            1_000,
            5_000,
        ));
    for (event_id, bytes_transferred) in [
        ("bridge-event-1", 10_u64),
        ("bridge-event-2", 20_u64),
        ("bridge-event-3", 30_u64),
    ] {
        push_local_bridge_runtime_event(
            &runtime,
            nekolink_protocol::LocalBridgeEvent::TransferUpdated(
                nekolink_protocol::LocalBridgeTransferUpdatedEvent {
                    event_id: event_id.to_string(),
                    transfer_id: "transfer-1".to_string(),
                    phase: nekolink_protocol::LocalBridgeTransferPhase::Sending,
                    bytes_transferred,
                    total_bytes: 100,
                },
            ),
        )
        .unwrap();
    }
    let first_page_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-page-1",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "after_event_id": null,
            "limit": 1
        }
    })
    .to_string();

    let first_page = handle_local_bridge_request_with_runtime_at(
        &first_page_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();
    let second_page_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-page-2",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "after_event_id": first_page.events_next_after_id,
            "limit": 2
        }
    })
    .to_string();
    let second_page = handle_local_bridge_request_with_runtime_at(
        &second_page_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_600,
    )
    .unwrap();

    assert_eq!(first_page.events.len(), 1);
    assert_eq!(first_page.events_last_id.as_deref(), Some("bridge-event-1"));
    assert_eq!(
        first_page.events_next_after_id.as_deref(),
        Some("bridge-event-1")
    );
    assert!(first_page.events_has_more);
    assert_eq!(first_page.events_cursor_state, "ok");
    assert_eq!(
        first_page.events_visible_first_id.as_deref(),
        Some("bridge-event-1")
    );
    assert_eq!(
        first_page.events_visible_last_id.as_deref(),
        Some("bridge-event-3")
    );
    assert_eq!(first_page.events_visible_count, 3);
    assert_eq!(second_page.events.len(), 2);
    assert_eq!(
        second_page.events[0]["payload"]["event_id"].as_str(),
        Some("bridge-event-2")
    );
    assert_eq!(
        second_page.events_last_id.as_deref(),
        Some("bridge-event-3")
    );
    assert!(!second_page.events_has_more);
    assert_eq!(second_page.events_cursor_state, "ok");
    assert_eq!(
        second_page.events_visible_first_id.as_deref(),
        Some("bridge-event-1")
    );
    assert_eq!(
        second_page.events_visible_last_id.as_deref(),
        Some("bridge-event-3")
    );
    assert_eq!(second_page.events_visible_count, 3);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_event_poll_reports_missing_cursor() {
    let dir = unique_bundle_temp_dir("local-bridge-events-missing-cursor");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::TransferStatusRead],
            1_000,
            5_000,
        ));
    push_local_bridge_runtime_event(
        &runtime,
        nekolink_protocol::LocalBridgeEvent::TransferUpdated(
            nekolink_protocol::LocalBridgeTransferUpdatedEvent {
                event_id: "bridge-event-current".to_string(),
                transfer_id: "transfer-1".to_string(),
                phase: nekolink_protocol::LocalBridgeTransferPhase::Sending,
                bytes_transferred: 10,
                total_bytes: 100,
            },
        ),
    )
    .unwrap();
    let poll_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-missing-cursor",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "after_event_id": "bridge-event-pruned",
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &poll_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.events.len(), 0);
    assert_eq!(response.events_cursor_state, "missing");
    assert_eq!(response.events_last_id, None);
    assert_eq!(response.events_next_after_id, None);
    assert!(!response.events_has_more);
    assert_eq!(
        response.events_visible_first_id.as_deref(),
        Some("bridge-event-current")
    );
    assert_eq!(
        response.events_visible_last_id.as_deref(),
        Some("bridge-event-current")
    );
    assert_eq!(response.events_visible_count, 1);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_event_poll_reports_empty_cursor_state() {
    let dir = unique_bundle_temp_dir("local-bridge-events-empty-cursor");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::TransferStatusRead],
            1_000,
            5_000,
        ));
    let poll_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-empty-cursor",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "after_event_id": null,
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &poll_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.events.len(), 0);
    assert_eq!(response.events_cursor_state, "empty");

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_event_poll_updates_only_scopes_used_by_returned_events() {
    let dir = unique_bundle_temp_dir("local-bridge-events-last-used-scope");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime.authorizations.lock().unwrap().extend([
        local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::TransferStatusRead],
            1_000,
            5_000,
        ),
        local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleRead],
            1_100,
            5_000,
        ),
        local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_200,
            5_000,
        ),
        local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_300,
            5_000,
        ),
    ]);
    push_local_bridge_runtime_event(
        &runtime,
        nekolink_protocol::LocalBridgeEvent::TransferUpdated(
            nekolink_protocol::LocalBridgeTransferUpdatedEvent {
                event_id: "bridge-event-transfer".to_string(),
                transfer_id: "transfer-1".to_string(),
                phase: nekolink_protocol::LocalBridgeTransferPhase::Sending,
                bytes_transferred: 10,
                total_bytes: 100,
            },
        ),
    )
    .unwrap();
    let poll_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-last-used",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "after_event_id": null,
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &poll_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        2_000,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.events.len(), 1);
    let authorizations = runtime.authorizations.lock().unwrap();
    let last_used_for_scope = |scope| {
        authorizations
            .iter()
            .find(|record| record.scopes == vec![scope])
            .map(|record| record.last_used_at_ms)
            .unwrap()
    };
    assert_eq!(
        last_used_for_scope(LocalBridgePermissionScope::TransferStatusRead),
        2_000
    );
    assert_eq!(
        last_used_for_scope(LocalBridgePermissionScope::BundleRead),
        1_100
    );
    assert_eq!(
        last_used_for_scope(LocalBridgePermissionScope::BundleSend),
        1_200
    );
    assert_eq!(
        last_used_for_scope(LocalBridgePermissionScope::BundleImportRequest),
        1_300
    );
    drop(authorizations);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_event_poll_can_wait_for_new_events() {
    let dir = unique_bundle_temp_dir("local-bridge-events-long-poll");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = Arc::new(LocalBridgeRuntimeState::default());
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::TransferStatusRead],
            1_000,
            5_000,
        ));
    let producer_runtime = runtime.clone();
    let producer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        push_local_bridge_runtime_event(
            &producer_runtime,
            nekolink_protocol::LocalBridgeEvent::TransferUpdated(
                nekolink_protocol::LocalBridgeTransferUpdatedEvent {
                    event_id: "bridge-event-waited".to_string(),
                    transfer_id: "transfer-1".to_string(),
                    phase: nekolink_protocol::LocalBridgeTransferPhase::Sending,
                    bytes_transferred: 10,
                    total_bytes: 100,
                },
            ),
        )
        .unwrap();
    });
    let poll_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-wait",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "after_event_id": null,
            "limit": 10,
            "timeout_ms": 500
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &poll_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        true,
        1_500,
    )
    .unwrap();
    producer.join().unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.events.len(), 1);
    assert_eq!(
        response.events[0]["payload"]["event_id"].as_str(),
        Some("bridge-event-waited")
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_event_poll_requires_authorized_client_scope() {
    let dir = unique_bundle_temp_dir("local-bridge-events-auth");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    push_local_bridge_runtime_event(
        &runtime,
        nekolink_protocol::LocalBridgeEvent::BundleReceived(
            nekolink_protocol::LocalBridgeBundleReceivedEvent {
                event_id: "bridge-event-1".to_string(),
                transfer_id: "transfer-1".to_string(),
                bundle_id: "bundle_1234567890".to_string(),
                bundle_type: BundleType::Skill,
                display_name: "voice_transcribe".to_string(),
                source_app: "Generic Agent App".to_string(),
                file_count: 2,
                total_bytes: 28,
                import_allowed: true,
            },
        ),
    )
    .unwrap();
    let poll_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-auth",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "after_event_id": null,
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &poll_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();

    assert_eq!(response.status, "pending_auth");
    assert_eq!(response.security_state, "requires_user_confirmation");
    assert!(response.events.is_empty());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_event_poll_timeout_does_not_delay_pending_auth() {
    let dir = unique_bundle_temp_dir("local-bridge-events-pending-auth-no-wait");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    let poll_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-auth-timeout",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "after_event_id": null,
            "limit": 10,
            "timeout_ms": 500
        }
    })
    .to_string();
    let started = Instant::now();

    let response = handle_local_bridge_request_with_runtime_at(
        &poll_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        true,
        1_500,
    )
    .unwrap();

    assert_eq!(response.status, "pending_auth");
    assert!(started.elapsed() < Duration::from_millis(100));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_transfer_status_producer_pushes_transfer_updated_event() {
    let runtime = LocalBridgeRuntimeState::default();
    let status = TransferStatusState {
        direction: "send".to_string(),
        phase: "completed".to_string(),
        root_name: Some("drop".to_string()),
        file_count: 2,
        file_index: 2,
        current_file: None,
        bytes_transferred: 200,
        total_bytes: 200,
        message: "发送完成".to_string(),
        updated_at_ms: 42,
    };

    push_local_bridge_transfer_status_event(&runtime, "transfer-1", &status).unwrap();

    let events = runtime.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    match &events[0] {
        nekolink_protocol::LocalBridgeEvent::TransferUpdated(event) => {
            assert_eq!(event.event_id, "transfer:transfer-1:completed:42");
            assert_eq!(event.transfer_id, "transfer-1");
            assert_eq!(
                event.phase,
                nekolink_protocol::LocalBridgeTransferPhase::Completed
            );
            assert_eq!(event.bytes_transferred, 200);
            assert_eq!(event.total_bytes, 200);
        }
        other => panic!("expected transfer.updated, got {other:?}"),
    }
}

#[test]
fn local_bridge_bundle_report_producer_pushes_bundle_received_event() {
    let runtime = LocalBridgeRuntimeState::default();
    let bundle = ReceivedBundleReport {
        bundle_id: "bundle_1234567890".to_string(),
        bundle_type: BundleType::Skill,
        display_name: "voice_transcribe".to_string(),
        source_app: "Generic Agent App".to_string(),
        file_count: 2,
        total_bytes: 28,
        staging_path: std::path::PathBuf::from("/tmp/staged/bundle_1234567890"),
        import_allowed: true,
    };

    push_local_bridge_bundle_received_event(&runtime, "transfer-1", &bundle).unwrap();

    let events = runtime.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    match &events[0] {
        nekolink_protocol::LocalBridgeEvent::BundleReceived(event) => {
            assert_eq!(event.event_id, "bundle:transfer-1:bundle_1234567890");
            assert_eq!(event.transfer_id, "transfer-1");
            assert_eq!(event.bundle_id, "bundle_1234567890");
            assert_eq!(event.bundle_type, BundleType::Skill);
            assert!(event.import_allowed);
        }
        other => panic!("expected bundle.received, got {other:?}"),
    }
}

#[test]
fn local_bridge_event_poll_reads_events_from_producers() {
    let dir = unique_bundle_temp_dir("local-bridge-produced-events-poll");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let runtime = LocalBridgeRuntimeState::default();
    runtime
        .authorizations
        .lock()
        .unwrap()
        .push(local_bridge_authorization(
            "local-agent-app",
            &[
                LocalBridgePermissionScope::BundleRead,
                LocalBridgePermissionScope::TransferStatusRead,
            ],
            1_000,
            5_000,
        ));
    let status = TransferStatusState {
        direction: "receive".to_string(),
        phase: "completed".to_string(),
        root_name: Some("drop".to_string()),
        file_count: 2,
        file_index: 2,
        current_file: None,
        bytes_transferred: 28,
        total_bytes: 28,
        message: "接收完成".to_string(),
        updated_at_ms: 42,
    };
    let bundle = ReceivedBundleReport {
        bundle_id: "bundle_1234567890".to_string(),
        bundle_type: BundleType::Skill,
        display_name: "voice_transcribe".to_string(),
        source_app: "Generic Agent App".to_string(),
        file_count: 2,
        total_bytes: 28,
        staging_path: std::path::PathBuf::from("/tmp/staged/bundle_1234567890"),
        import_allowed: true,
    };
    push_local_bridge_transfer_status_event(&runtime, "transfer-1", &status).unwrap();
    push_local_bridge_bundle_received_event(&runtime, "transfer-1", &bundle).unwrap();
    let poll_request = serde_json::json!({
        "kind": "events.poll",
        "payload": {
            "request_id": "bridge-events-produced",
            "client": {
                "client_id": "local-agent-app",
                "display_name": "Local Agent App",
                "app_kind": "agent"
            },
            "after_event_id": null,
            "limit": 10
        }
    })
    .to_string();

    let response = handle_local_bridge_request_with_runtime_at(
        &poll_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_500,
    )
    .unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.events.len(), 2);
    assert_eq!(response.events[0]["kind"], "transfer.updated");
    assert_eq!(response.events[1]["kind"], "bundle.received");

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn confirmed_runtime_authorization_is_saved_for_restart() {
    let dir = unique_bundle_temp_dir("local-bridge-runtime-persisted-auth");
    let staging_root = dir.join("bundle_staging");
    let import_root = dir.join("bundle_imports");
    let authorizations_path = dir.join("local_bridge_authorizations.json");
    let runtime = LocalBridgeRuntimeState::default();
    let auth_request = serde_json::json!({
        "kind": "authorization.request",
        "payload": {
            "request_id": "bridge-auth-persist",
            "client": {
                "client_id": "generic-local-app",
                "display_name": "Generic Local App",
                "app_kind": "generic"
            },
            "requested_scopes": [
                "bundle.send"
            ],
            "reason": "Send a local bundle",
            "ttl_seconds": 900
        }
    })
    .to_string();

    let auth_response = handle_local_bridge_request_with_runtime_at(
        &auth_request,
        &[],
        None,
        &staging_root,
        &import_root,
        &runtime,
        false,
        1_000,
    )
    .unwrap();
    let authorization = confirm_local_bridge_runtime_authorization_and_save_at(
        &runtime,
        auth_response.authorization_code.as_deref().unwrap(),
        1_500,
        &authorizations_path,
    )
    .unwrap();
    let saved = crate::local_bridge_authorizations::load_local_bridge_authorizations_at(
        &authorizations_path,
        1_600,
    )
    .unwrap();

    assert_eq!(authorization.client_id, "generic-local-app");
    assert_eq!(saved, vec![authorization]);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_authorization_management_lists_revokes_and_prunes() {
    let dir = unique_bundle_temp_dir("local-bridge-authorization-management");
    let authorizations_path = dir.join("local_bridge_authorizations.json");
    let runtime = LocalBridgeRuntimeState::default();
    runtime.authorizations.lock().unwrap().extend([
        local_bridge_authorization(
            "multi-scope-app",
            &[
                LocalBridgePermissionScope::BundleSend,
                LocalBridgePermissionScope::BundleImportRequest,
            ],
            3_000,
            40_000,
        ),
        local_bridge_authorization(
            "local-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_000,
            20_000,
        ),
        local_bridge_authorization(
            "local-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            2_000,
            30_000,
        ),
        local_bridge_authorization(
            "expired-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_000,
            2_000,
        ),
    ]);

    let listed = list_local_bridge_authorizations_at(&runtime, 3_000);
    let revoked = revoke_local_bridge_authorization_at(
        &runtime,
        "local-app",
        LocalBridgePermissionScope::BundleSend,
        3_500,
        &authorizations_path,
    )
    .unwrap();
    let pruned =
        prune_local_bridge_authorizations_at(&runtime, 5_000, &authorizations_path).unwrap();
    let multi_scope_revoked = revoke_local_bridge_authorization_at(
        &runtime,
        "multi-scope-app",
        LocalBridgePermissionScope::BundleSend,
        5_100,
        &authorizations_path,
    )
    .unwrap();
    let saved = crate::local_bridge_authorizations::load_local_bridge_authorizations_at(
        &authorizations_path,
        5_500,
    )
    .unwrap();

    assert_eq!(listed.len(), 3);
    assert!(revoked);
    assert!(multi_scope_revoked);
    assert_eq!(pruned, 1);
    assert_eq!(saved.len(), 2);
    assert_eq!(
        saved
            .iter()
            .find(|record| record.client_id == "multi-scope-app")
            .unwrap()
            .scopes,
        vec![LocalBridgePermissionScope::BundleImportRequest]
    );
    assert!(saved.iter().any(|record| {
        record.client_id == "local-app"
            && record.scopes == vec![LocalBridgePermissionScope::BundleImportRequest]
    }));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn confirmed_local_bridge_authorization_grants_requested_scopes() {
    let pending = PendingLocalBridgeAuthorization {
        request_id: "bridge-auth-1".to_string(),
        client: LocalBridgeClientIdentity {
            client_id: "local-agent-app".to_string(),
            display_name: "Local Agent App".to_string(),
            app_kind: Some("agent".to_string()),
        },
        requested_scopes: vec![LocalBridgePermissionScope::BundleImportRequest],
        reason: "Import staged bundle".to_string(),
        authorization_code: "ABC-123".to_string(),
        requested_at_ms: 1_000,
        expires_at_ms: 11_000,
    };

    let authorization =
        confirm_pending_local_bridge_authorization(&pending, "ABC-123", 2_000).unwrap();

    assert_eq!(authorization.client_id, "local-agent-app");
    assert_eq!(
        authorization.scopes,
        vec![LocalBridgePermissionScope::BundleImportRequest]
    );
    assert_eq!(authorization.granted_at_ms, 2_000);
    assert_eq!(authorization.last_used_at_ms, 2_000);
    assert_eq!(authorization.expires_at_ms, Some(11_000));
    assert!(local_bridge_client_has_scope(
        Some(&pending.client),
        &[authorization],
        LocalBridgePermissionScope::BundleImportRequest,
        3_000,
    ));
}

#[test]
fn confirmed_local_bridge_authorization_dedupes_requested_scopes() {
    let pending = PendingLocalBridgeAuthorization {
        request_id: "bridge-auth-1".to_string(),
        client: LocalBridgeClientIdentity {
            client_id: "local-agent-app".to_string(),
            display_name: "Local Agent App".to_string(),
            app_kind: Some("agent".to_string()),
        },
        requested_scopes: vec![
            LocalBridgePermissionScope::BundleRead,
            LocalBridgePermissionScope::BundleRead,
            LocalBridgePermissionScope::TransferStatusRead,
        ],
        reason: "Read local bridge state".to_string(),
        authorization_code: "ABC-123".to_string(),
        requested_at_ms: 1_000,
        expires_at_ms: 11_000,
    };

    let authorization =
        confirm_pending_local_bridge_authorization(&pending, "ABC-123", 2_000).unwrap();

    assert_eq!(
        authorization.scopes,
        vec![
            LocalBridgePermissionScope::BundleRead,
            LocalBridgePermissionScope::TransferStatusRead,
        ]
    );
}

#[test]
fn receive_policy_block_all_rejects_offer_without_pending_prompt() {
    let pending = Arc::new(Mutex::new(None));
    let status = Arc::new(Mutex::new(None));
    let trusted = Arc::new(Mutex::new(Vec::new()));
    let offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());

    let accepted = wait_for_receive_decision(
        &offer,
        &pending,
        &status,
        ReceivePolicy::BlockAll,
        &trusted,
        ReceiveTrustContext::Untrusted,
        None,
    );

    assert!(!accepted);
    assert!(pending.lock().unwrap().is_none());
    let status = status.lock().unwrap().clone().unwrap();
    assert_eq!(status.phase, "blocked");
    assert!(status.message.contains("阻止"));
}

#[test]
fn receive_policy_auto_accept_trusted_requires_authenticated_session() {
    let public_key = test_public_key("device-a");
    let trusted = Arc::new(Mutex::new(vec![TrustedDeviceRecord {
        schema_version: 1,
        device_id: "device-a".to_string(),
        device_name: "MacBook".to_string(),
        platform: "macos".to_string(),
        host: "192.168.1.20".to_string(),
        port: 45821,
        public_key: public_key.public_key.clone(),
        public_key_fingerprint: public_key.fingerprint.clone(),
        pairing_code: "AAA-BBB".to_string(),
        paired_at_ms: 1,
        last_seen_at_ms: 1,
    }]));
    let mut offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());
    offer.sender_device_id = Some("device-a".to_string());
    offer.sender_public_key_fingerprint = Some(public_key.fingerprint);

    let accepted = should_auto_accept_receive_offer(
        &offer,
        ReceivePolicy::AutoAcceptTrusted,
        &trusted,
        ReceiveTrustContext::Untrusted,
    );

    assert!(!accepted);
}

#[test]
fn receive_policy_auto_accept_trusted_accepts_authenticated_trusted_session() {
    let public_key = test_public_key("device-a");
    let trusted = Arc::new(Mutex::new(vec![trusted_record_with_public_key(
        "device-a",
        "MacBook",
        public_key.public_key.as_str(),
        public_key.fingerprint.as_str(),
    )]));
    let mut offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());
    offer.sender_device_id = Some("device-a".to_string());
    offer.sender_public_key_fingerprint = Some(public_key.fingerprint);

    let accepted = should_auto_accept_receive_offer(
        &offer,
        ReceivePolicy::AutoAcceptTrusted,
        &trusted,
        ReceiveTrustContext::AuthenticatedTrusted,
    );

    assert!(accepted);
}

#[test]
fn authenticated_trusted_receive_policy_auto_accepts_without_pending_prompt() {
    let public_key = test_public_key("device-a");
    let pending = Arc::new(Mutex::new(None));
    let status = Arc::new(Mutex::new(None));
    let trusted = Arc::new(Mutex::new(vec![trusted_record_with_public_key(
        "device-a",
        "MacBook",
        public_key.public_key.as_str(),
        public_key.fingerprint.as_str(),
    )]));
    let mut offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());
    offer.sender_device_id = Some("device-a".to_string());
    offer.sender_public_key_fingerprint = Some(public_key.fingerprint);

    let accepted = wait_for_receive_decision(
        &offer,
        &pending,
        &status,
        ReceivePolicy::AutoAcceptTrusted,
        &trusted,
        ReceiveTrustContext::AuthenticatedTrusted,
        None,
    );

    assert!(accepted);
    assert!(pending.lock().unwrap().is_none());
    let status = status.lock().unwrap().clone().unwrap();
    assert_eq!(status.phase, "auto_accepted");
}

#[test]
fn legacy_plain_offer_from_trusted_device_identity_is_rejected_before_prompt() {
    let public_key = test_public_key("device-a");
    let pending = Arc::new(Mutex::new(None));
    let status = Arc::new(Mutex::new(None));
    let trusted = Arc::new(Mutex::new(vec![trusted_record_with_public_key(
        "device-a",
        "MacBook",
        public_key.public_key.as_str(),
        public_key.fingerprint.as_str(),
    )]));
    let mut offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());
    offer.sender_device_id = Some("device-a".to_string());
    offer.sender_public_key_fingerprint = Some(public_key.fingerprint);

    let accepted = wait_for_receive_decision(
        &offer,
        &pending,
        &status,
        ReceivePolicy::AlwaysAsk,
        &trusted,
        ReceiveTrustContext::Untrusted,
        None,
    );

    assert!(!accepted);
    assert!(pending.lock().unwrap().is_none());
    let status = status.lock().unwrap().clone().unwrap();
    assert_eq!(status.phase, "blocked");
    assert!(status.message.contains("明文"));
}

#[test]
fn legacy_plain_offer_rejects_known_device_id_even_with_mismatched_fingerprint() {
    let public_key = test_public_key("device-a");
    let pending = Arc::new(Mutex::new(None));
    let status = Arc::new(Mutex::new(None));
    let trusted = Arc::new(Mutex::new(vec![trusted_record_with_public_key(
        "device-a",
        "MacBook",
        public_key.public_key.as_str(),
        public_key.fingerprint.as_str(),
    )]));
    let mut offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());
    offer.sender_device_id = Some("device-a".to_string());
    offer.sender_public_key_fingerprint = Some("sha256:rotated".to_string());

    let accepted = wait_for_receive_decision(
        &offer,
        &pending,
        &status,
        ReceivePolicy::AlwaysAsk,
        &trusted,
        ReceiveTrustContext::Untrusted,
        None,
    );

    assert!(!accepted);
    assert!(pending.lock().unwrap().is_none());
    let status = status.lock().unwrap().clone().unwrap();
    assert_eq!(status.phase, "blocked");
    assert!(status.message.contains("已认证加密"));
}

#[test]
fn legacy_plain_offer_from_unknown_identity_remains_manual_compatibility() {
    let trusted_public_key = test_public_key("device-a");
    let unknown_public_key = test_public_key("device-b");
    let trusted = Arc::new(Mutex::new(vec![trusted_record_with_public_key(
        "device-a",
        "MacBook",
        trusted_public_key.public_key.as_str(),
        trusted_public_key.fingerprint.as_str(),
    )]));
    let mut offer = TransferOffer::new("transfer-b", "example.txt", Vec::new());
    offer.sender_device_id = Some("device-b".to_string());
    offer.sender_public_key_fingerprint = Some(unknown_public_key.fingerprint);

    assert!(!legacy_plain_offer_matches_trusted_device(&offer, &trusted));
}

#[test]
fn legacy_plain_offer_matches_trusted_device_by_known_device_id() {
    let trusted_public_key = test_public_key("device-a");
    let trusted = Arc::new(Mutex::new(vec![trusted_record_with_public_key(
        "device-a",
        "MacBook",
        trusted_public_key.public_key.as_str(),
        trusted_public_key.fingerprint.as_str(),
    )]));
    let mut offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());
    offer.sender_device_id = Some("device-a".to_string());
    offer.sender_public_key_fingerprint = Some("sha256:rotated".to_string());

    assert!(legacy_plain_offer_matches_trusted_device(&offer, &trusted));
}

#[test]
fn receive_policy_input_rejects_unknown_values() {
    assert_eq!(
        receive_policy_from_input("always_ask").unwrap(),
        ReceivePolicy::AlwaysAsk
    );
    assert_eq!(
        receive_policy_from_input("auto_accept_trusted").unwrap(),
        ReceivePolicy::AutoAcceptTrusted
    );
    assert_eq!(
        receive_policy_from_input("block_all").unwrap(),
        ReceivePolicy::BlockAll
    );
    assert!(receive_policy_from_input("unknown").is_err());
}

#[test]
fn current_transfer_progress_uses_last_status_bytes() {
    let status = Arc::new(Mutex::new(Some(TransferStatusState {
        direction: "send".to_string(),
        phase: "sending".to_string(),
        root_name: Some("drop".to_string()),
        file_count: 2,
        file_index: 1,
        current_file: Some("drop/a.txt".to_string()),
        bytes_transferred: 42,
        total_bytes: 100,
        message: "发送中".to_string(),
        updated_at_ms: 1,
    })));

    let (file_index, current_file, bytes_transferred, total_bytes) =
        current_transfer_progress(&status);

    assert_eq!(file_index, 1);
    assert_eq!(current_file.as_deref(), Some("drop/a.txt"));
    assert_eq!(bytes_transferred, 42);
    assert_eq!(total_bytes, 100);
}

fn nearby_device(device_id: &str, fingerprint: &str) -> Device {
    let public_key = test_public_key(device_id);
    let mut device = Device::new(
        nekodrop_core::DeviceId::new(device_id).unwrap(),
        "MacBook",
        nekodrop_core::DevicePlatform::MacOS,
        "192.168.1.20",
        45821,
    )
    .unwrap();
    device.public_key = Some(public_key.public_key);
    device.public_key_fingerprint = Some(fingerprint.to_string());
    device
}

fn trusted_record(
    device_id: &str,
    device_name: &str,
    public_key_fingerprint: &str,
) -> TrustedDeviceRecord {
    let public_key = test_public_key(device_id);
    trusted_record_with_public_key(
        device_id,
        device_name,
        public_key.public_key.as_str(),
        public_key_fingerprint,
    )
}

fn trusted_record_with_public_key(
    device_id: &str,
    device_name: &str,
    public_key: &str,
    public_key_fingerprint: &str,
) -> TrustedDeviceRecord {
    TrustedDeviceRecord {
        schema_version: 1,
        device_id: device_id.to_string(),
        device_name: device_name.to_string(),
        platform: "macos".to_string(),
        host: "192.168.1.20".to_string(),
        port: 45821,
        public_key: public_key.to_string(),
        public_key_fingerprint: public_key_fingerprint.to_string(),
        pairing_code: "AAA-BBB".to_string(),
        paired_at_ms: 1,
        last_seen_at_ms: 1,
    }
}

fn local_bridge_authorization(
    client_id: &str,
    scopes: &[LocalBridgePermissionScope],
    granted_at_ms: u128,
    expires_at_ms: u128,
) -> LocalBridgeAuthorizationRecord {
    LocalBridgeAuthorizationRecord {
        client_id: client_id.to_string(),
        display_name: "Local Agent App".to_string(),
        app_kind: Some("agent".to_string()),
        scopes: scopes.to_vec(),
        granted_at_ms,
        last_used_at_ms: granted_at_ms,
        expires_at_ms: Some(expires_at_ms),
    }
}

fn test_public_key(label: &str) -> nekolink_protocol::DeviceIdentityPublicKey {
    let mut seed = [0_u8; nekolink_protocol::DEVICE_IDENTITY_SIGNING_KEY_LEN];
    for (index, byte) in label.as_bytes().iter().enumerate() {
        seed[index % seed.len()] ^= *byte;
    }
    nekolink_protocol::DeviceIdentitySigningKey::from_seed(seed).public_key()
}

fn test_identity(device_id: &str) -> DeviceIdentity {
    DeviceIdentity::new(
        device_id,
        "This Mac",
        nekolink_protocol::DeviceKind::Desktop,
        nekolink_protocol::PlatformKind::Macos,
        "sha256:self",
        [],
    )
}

fn test_identity_signing_key(device_id: &str) -> nekolink_protocol::DeviceIdentitySigningKey {
    let mut seed = [0_u8; nekolink_protocol::DEVICE_IDENTITY_SIGNING_KEY_LEN];
    for (index, byte) in device_id.as_bytes().iter().enumerate() {
        seed[index % seed.len()] ^= byte.rotate_left((index % 8) as u32);
    }
    nekolink_protocol::DeviceIdentitySigningKey::from_seed(seed)
}

fn test_identity_with_signing_key(
    device_id: &str,
    signing_key: &nekolink_protocol::DeviceIdentitySigningKey,
) -> DeviceIdentity {
    let public_key = signing_key.public_key();
    DeviceIdentity::new(
        device_id,
        "This Mac",
        nekolink_protocol::DeviceKind::Desktop,
        nekolink_protocol::PlatformKind::Macos,
        public_key.fingerprint,
        [nekolink_protocol::Capability::EncryptedSession],
    )
}

fn create_desktop_test_bundle(
    dir: &std::path::Path,
    directory_name: &str,
    bundle_id: &str,
) -> PathBuf {
    create_desktop_test_bundle_with_type(dir, directory_name, bundle_id, BundleType::Skill)
}

fn create_desktop_test_bundle_with_type(
    dir: &std::path::Path,
    directory_name: &str,
    bundle_id: &str,
    bundle_type: BundleType,
) -> PathBuf {
    let root = dir.join(directory_name);
    fs::create_dir_all(root.join("files")).unwrap();
    fs::write(
        root.join("files").join("manifest.json"),
        b"{\"kind\":\"skill\"}",
    )
    .unwrap();
    fs::write(root.join("files").join("content.bin"), b"hello bundle").unwrap();
    let mut manifest = desktop_test_bundle_manifest();
    manifest.bundle_id = bundle_id.to_string();
    manifest.bundle_type = bundle_type;
    write_json(root.join("bundle.json"), &manifest);
    write_json(
        root.join("checksums.json"),
        &desktop_test_bundle_checksums(),
    );
    write_json(
        root.join("permissions.json"),
        &desktop_test_bundle_permissions(),
    );
    root
}

fn desktop_test_bundle_manifest() -> BundleManifest {
    BundleManifest {
        schema: BUNDLE_SCHEMA_V1.to_string(),
        bundle_id: "bundle_1234567890".to_string(),
        bundle_type: BundleType::Skill,
        display_name: "voice_transcribe".to_string(),
        source_app: "Generic Agent App".to_string(),
        created_at: "2026-06-14T10:30:00Z".to_string(),
        sender: BundleSender {
            device_id: "neko-device-1234567890".to_string(),
            device_name: "MacBook".to_string(),
            fingerprint: "sha256:0123456789abcdef".to_string(),
        },
        compatibility: BundleCompatibility {
            min_nekolink_version: PROTOCOL_VERSION,
            required_capabilities: vec![Capability::BundleTransfer],
        },
        summary: BundleSummary {
            file_count: 2,
            total_bytes: 28,
        },
        files: vec![
            BundleFile {
                path: "files/manifest.json".to_string(),
                size: 16,
                sha256: "0bc3f835203da0c2bbb44658e66c6bc0449e7f00bd9bd8fecd5d12283baaf5c9"
                    .to_string(),
                role: "manifest".to_string(),
            },
            BundleFile {
                path: "files/content.bin".to_string(),
                size: 12,
                sha256: "04cfecf64270c52b81da10bf6890b24fa73ee79715c44d1bc443dd9dd1de04d0"
                    .to_string(),
                role: "payload".to_string(),
            },
        ],
    }
}

fn desktop_test_bundle_checksums() -> BundleChecksums {
    let mut files = BTreeMap::new();
    files.insert(
        "files/manifest.json".to_string(),
        "0bc3f835203da0c2bbb44658e66c6bc0449e7f00bd9bd8fecd5d12283baaf5c9".to_string(),
    );
    files.insert(
        "files/content.bin".to_string(),
        "04cfecf64270c52b81da10bf6890b24fa73ee79715c44d1bc443dd9dd1de04d0".to_string(),
    );
    BundleChecksums {
        algorithm: BUNDLE_CHECKSUM_SHA256.to_string(),
        files,
    }
}

fn desktop_test_bundle_permissions() -> BundlePermissions {
    BundlePermissions {
        requested_scopes: vec![BundlePermissionScope::SkillInstall],
        writes: vec![BundleWritePermission {
            target: "agent.skills".to_string(),
            mode: BundleWriteMode::CreateOnly,
        }],
        secrets: BundleSecretsPolicy {
            contains_secrets: false,
            redacted_fields: Vec::new(),
        },
    }
}

fn write_json(path: impl AsRef<std::path::Path>, value: &impl serde::Serialize) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn unique_bundle_temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "nekodrop-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
