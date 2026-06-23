use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use nekodrop_core::{NekoDropError, NekoDropResult};
use nekodrop_network::{
    connect_endpoint, read_incoming_control_frame, read_pairing_decision,
    read_session_identity_binding, read_session_transfer_decision_once,
    read_session_transfer_offer_once, read_transfer_decision, read_verified_session_ready,
    receive_encrypted_file_frames_with_expected_count, receive_file_frames_with_expected_count,
    send_encrypted_file_frames_with_resume_and_cancel, send_file_frames_with_resume_and_cancel,
    write_pairing_decision, write_pairing_request, write_session_hello,
    write_session_identity_binding, write_session_ready, write_session_transfer_decision,
    write_session_transfer_offer, write_transfer_decision_for_transfer, write_transfer_offer,
    ConnectionTicket, Endpoint, IncomingControlFrame, OutgoingFileFrame, PairingDecisionPayload,
    PairingRequestPayload, SentFileFrame, SignedSessionIdentityBinding, TransferDecision,
    TransferOffer, TransferOfferFile, TransferProgress, TransferResumeFile,
};
use nekodrop_storage::{
    build_resume_plan_for_files, check_receive_space, create_source_plan_from_paths,
    create_source_plan_from_paths_with_progress, safe_join_receive_path, stage_bundle_directory,
    write_received_file_with_resume_and_cancel, BundleImportPolicy, ReceivedFile,
    ResumeExpectedFile, ResumePlan, StagedBundle,
};
use nekolink_protocol::{
    default_session_cipher_preference, BundleType, Capability, DeviceIdentity, ProtocolError,
    SessionEphemeralKeyPair, SessionFrameKind, SessionHelloPayload, SessionIdentityBinding,
    SessionKeyMaterial, SessionReadyPayload, SessionReplayWindow, SessionTrafficCounters,
    SessionTrafficFrameHeader, VerifiedSessionHandshake,
};

pub use nekodrop_storage::{
    TransferPlanScanPhase, TransferPlanScanProgress, TransferSourceFile, TransferSourcePlan,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSendReport {
    pub plan: TransferSourcePlan,
    pub sent_files: Vec<SentFileFrame>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferReceiveReport {
    pub transfer_id: String,
    pub root_name: String,
    pub security_mode: TransferSecurityMode,
    pub sender_device_id: Option<String>,
    pub sender_device_name: Option<String>,
    pub sender_public_key_fingerprint: Option<String>,
    pub files: Vec<ReceivedFile>,
    pub bundle: Option<ReceivedBundleReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferSecurityMode {
    LegacyPlain,
    EncryptedSession,
    AuthenticatedEncryptedSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedBundleReport {
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub display_name: String,
    pub source_app: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub staging_path: PathBuf,
    pub import_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferProgressEvent {
    AwaitingApproval {
        root_name: String,
        file_count: usize,
        total_bytes: u64,
    },
    Sending(TransferProgress),
    Receiving(TransferProgress),
    Verifying {
        manifest_path: String,
        bytes_transferred: u64,
        total_bytes: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncomingSessionReport {
    Transfer(TransferReceiveReport),
    Pairing(PairingDecisionPayload),
}

pub fn create_transfer_plan(paths: &[PathBuf]) -> NekoDropResult<TransferSourcePlan> {
    create_source_plan_from_paths(paths)
}

pub fn create_transfer_plan_with_scan_progress<F>(
    paths: &[PathBuf],
    on_progress: F,
) -> NekoDropResult<TransferSourcePlan>
where
    F: FnMut(TransferPlanScanProgress),
{
    create_source_plan_from_paths_with_progress(paths, on_progress)
}

pub fn connection_code_for_endpoint(
    endpoint: Endpoint,
    device_name: Option<&str>,
) -> NekoDropResult<String> {
    let mut ticket = ConnectionTicket::new(endpoint)?;
    if let Some(device_name) = device_name {
        ticket = ticket.with_device_name(device_name);
    }
    ticket.to_code()
}

pub fn endpoint_from_connection_code(code: &str) -> NekoDropResult<Endpoint> {
    Ok(ConnectionTicket::parse(code)?.endpoint)
}

pub fn send_paths(endpoint: &Endpoint, paths: &[PathBuf]) -> NekoDropResult<TransferSendReport> {
    send_paths_with_progress(endpoint, paths, |_| {})
}

pub fn send_paths_with_progress<F>(
    endpoint: &Endpoint,
    paths: &[PathBuf],
    on_progress: F,
) -> NekoDropResult<TransferSendReport>
where
    F: FnMut(TransferProgressEvent),
{
    let plan = create_source_plan_from_paths(paths)?;
    send_plan_with_progress(endpoint, plan, on_progress)
}

pub fn send_plan_with_progress<F>(
    endpoint: &Endpoint,
    plan: TransferSourcePlan,
    on_progress: F,
) -> NekoDropResult<TransferSendReport>
where
    F: FnMut(TransferProgressEvent),
{
    send_plan_with_progress_and_cancel(endpoint, plan, on_progress, || false)
}

pub fn send_plan_with_progress_and_cancel<F, C>(
    endpoint: &Endpoint,
    plan: TransferSourcePlan,
    on_progress: F,
    should_cancel: C,
) -> NekoDropResult<TransferSendReport>
where
    F: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
{
    send_plan_with_sender_identity_and_cancel(endpoint, plan, None, on_progress, should_cancel)
}

pub fn send_plan_with_sender_identity_and_cancel<F, C>(
    endpoint: &Endpoint,
    plan: TransferSourcePlan,
    sender_identity: Option<&DeviceIdentity>,
    on_progress: F,
    mut should_cancel: C,
) -> NekoDropResult<TransferSendReport>
where
    F: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
{
    let outgoing = outgoing_frames_from_plan(&plan);
    let offer = offer_from_plan_with_sender_identity(&plan, sender_identity);
    let mut stream = connect_endpoint(endpoint)?;
    write_transfer_offer(&mut stream, &offer)?;
    let mut on_progress = on_progress;
    on_progress(TransferProgressEvent::AwaitingApproval {
        root_name: plan.manifest.root_name.clone(),
        file_count: plan.file_count(),
        total_bytes: plan.total_bytes(),
    });
    let decision = read_transfer_decision(&mut stream)?;
    if should_cancel() {
        return Err(NekoDropError::Network("transfer cancelled".into()));
    }
    if !decision.accepted {
        return Err(NekoDropError::Network(format!(
            "receiver declined transfer: {}",
            decision
                .reason
                .unwrap_or_else(|| "no reason provided".to_string())
        )));
    }

    let sent_files = send_file_frames_with_resume_and_cancel(
        &mut stream,
        &outgoing,
        plan.total_bytes(),
        &decision.resume_files,
        |progress| on_progress(TransferProgressEvent::Sending(progress)),
        || should_cancel(),
    )?;

    Ok(TransferSendReport { plan, sent_files })
}

pub fn send_plan_with_encrypted_control_and_cancel<F, C>(
    endpoint: &Endpoint,
    plan: TransferSourcePlan,
    sender_identity: &DeviceIdentity,
    on_progress: F,
    mut should_cancel: C,
) -> NekoDropResult<TransferSendReport>
where
    F: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
{
    let outgoing = outgoing_frames_from_plan(&plan);
    let offer = offer_from_plan_with_sender_identity(&plan, Some(sender_identity));
    let mut stream = connect_endpoint(endpoint)?;
    let mut session = start_initiator_session(&mut stream, sender_identity, &offer.transfer_id)?;
    let offer_message_id = session.next_message_id("offer");
    let offer_header = session.next_send_control_header()?;
    write_session_transfer_offer(
        &mut stream,
        session.session_id.clone(),
        offer_message_id,
        &session.keys,
        offer_header,
        &offer,
    )?;
    let mut on_progress = on_progress;
    on_progress(TransferProgressEvent::AwaitingApproval {
        root_name: plan.manifest.root_name.clone(),
        file_count: plan.file_count(),
        total_bytes: plan.total_bytes(),
    });
    let decision = session.read_transfer_decision(&mut stream)?;
    if should_cancel() {
        return Err(NekoDropError::Network("transfer cancelled".into()));
    }
    if !decision.accepted {
        return Err(NekoDropError::Network(format!(
            "receiver declined transfer: {}",
            decision
                .reason
                .unwrap_or_else(|| "no reason provided".to_string())
        )));
    }

    let sent_files = send_encrypted_file_frames_with_resume_and_cancel(
        &mut stream,
        &offer.transfer_id,
        &outgoing,
        plan.total_bytes(),
        &decision.resume_files,
        &session.keys,
        &mut session.counters,
        &session.cipher,
        |progress| on_progress(TransferProgressEvent::Sending(progress)),
        || should_cancel(),
    )?;

    Ok(TransferSendReport { plan, sent_files })
}

pub fn send_plan_with_authenticated_session_and_cancel<S, F, C>(
    endpoint: &Endpoint,
    plan: TransferSourcePlan,
    sender_identity: &DeviceIdentity,
    sign_identity_binding: S,
    on_progress: F,
    should_cancel: C,
) -> NekoDropResult<TransferSendReport>
where
    S: FnMut(SessionIdentityBinding) -> NekoDropResult<SignedSessionIdentityBinding>,
    F: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
{
    send_plan_with_authenticated_session_peer_verifier_and_cancel(
        endpoint,
        plan,
        sender_identity,
        sign_identity_binding,
        |_, _| Ok(()),
        on_progress,
        should_cancel,
    )
}

pub fn send_plan_with_authenticated_session_peer_verifier_and_cancel<S, V, F, C>(
    endpoint: &Endpoint,
    plan: TransferSourcePlan,
    sender_identity: &DeviceIdentity,
    sign_identity_binding: S,
    verify_peer_identity: V,
    on_progress: F,
    mut should_cancel: C,
) -> NekoDropResult<TransferSendReport>
where
    S: FnMut(SessionIdentityBinding) -> NekoDropResult<SignedSessionIdentityBinding>,
    V: FnMut(&DeviceIdentity, &SignedSessionIdentityBinding) -> NekoDropResult<()>,
    F: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
{
    let outgoing = outgoing_frames_from_plan(&plan);
    let offer = offer_from_plan_with_sender_identity(&plan, Some(sender_identity));
    let mut stream = connect_endpoint(endpoint)?;
    let mut session = start_authenticated_initiator_session_with_peer_verifier(
        &mut stream,
        sender_identity,
        &offer.transfer_id,
        sign_identity_binding,
        verify_peer_identity,
    )?;
    let offer_message_id = session.next_message_id("offer");
    let offer_header = session.next_send_control_header()?;
    write_session_transfer_offer(
        &mut stream,
        session.session_id.clone(),
        offer_message_id,
        &session.keys,
        offer_header,
        &offer,
    )?;
    let mut on_progress = on_progress;
    on_progress(TransferProgressEvent::AwaitingApproval {
        root_name: plan.manifest.root_name.clone(),
        file_count: plan.file_count(),
        total_bytes: plan.total_bytes(),
    });
    let decision = session.read_transfer_decision(&mut stream)?;
    if should_cancel() {
        return Err(NekoDropError::Network("transfer cancelled".into()));
    }
    if !decision.accepted {
        return Err(NekoDropError::Network(format!(
            "receiver declined transfer: {}",
            decision
                .reason
                .unwrap_or_else(|| "no reason provided".to_string())
        )));
    }

    let sent_files = send_encrypted_file_frames_with_resume_and_cancel(
        &mut stream,
        &offer.transfer_id,
        &outgoing,
        plan.total_bytes(),
        &decision.resume_files,
        &session.keys,
        &mut session.counters,
        &session.cipher,
        |progress| on_progress(TransferProgressEvent::Sending(progress)),
        || should_cancel(),
    )?;

    Ok(TransferSendReport { plan, sent_files })
}

pub fn send_pairing_request(
    endpoint: &Endpoint,
    request: PairingRequestPayload,
) -> NekoDropResult<PairingDecisionPayload> {
    let mut stream = connect_endpoint(endpoint)?;
    write_pairing_request(&mut stream, &request)?;
    read_pairing_decision(&mut stream)
}

pub fn accept_transfer(
    listener: &TcpListener,
    receive_dir: &Path,
) -> NekoDropResult<TransferReceiveReport> {
    accept_transfer_with_decision(listener, receive_dir, |_| true, |_| {})
}

pub fn accept_transfer_with_bundle_staging(
    listener: &TcpListener,
    receive_dir: &Path,
    bundle_staging_root: &Path,
) -> NekoDropResult<TransferReceiveReport> {
    accept_transfer_with_decision_and_bundle_staging(
        listener,
        receive_dir,
        bundle_staging_root,
        |_| true,
        |_| {},
    )
}

pub fn accept_transfer_with_decision<D, P>(
    listener: &TcpListener,
    receive_dir: &Path,
    decide: D,
    on_progress: P,
) -> NekoDropResult<TransferReceiveReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    P: FnMut(TransferProgressEvent),
{
    let (mut stream, _) = listener.accept().map_err(|error| {
        NekoDropError::Network(format!("failed to accept TCP connection: {error}"))
    })?;
    accept_transfer_stream_with_decision(&mut stream, receive_dir, decide, on_progress)
}

pub fn accept_transfer_with_decision_and_bundle_staging<D, P>(
    listener: &TcpListener,
    receive_dir: &Path,
    bundle_staging_root: &Path,
    decide: D,
    on_progress: P,
) -> NekoDropResult<TransferReceiveReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    P: FnMut(TransferProgressEvent),
{
    let (mut stream, _) = listener.accept().map_err(|error| {
        NekoDropError::Network(format!("failed to accept TCP connection: {error}"))
    })?;
    accept_transfer_stream_with_decision_and_bundle_staging(
        &mut stream,
        receive_dir,
        bundle_staging_root,
        decide,
        on_progress,
    )
}

pub fn accept_transfer_stream_with_decision<D, P>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    decide: D,
    on_progress: P,
) -> NekoDropResult<TransferReceiveReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    P: FnMut(TransferProgressEvent),
{
    match read_incoming_control_frame(stream)? {
        IncomingControlFrame::FileOffer(offer) => accept_transfer_offer_stream_with_decision(
            stream,
            receive_dir,
            offer,
            decide,
            on_progress,
        ),
        IncomingControlFrame::SessionHello(_) => Err(NekoDropError::Network(
            "session hello requires encrypted control receive entry".into(),
        )),
        IncomingControlFrame::DeviceHello(_) => Err(NekoDropError::Network(
            "device hello is not a transfer offer".into(),
        )),
        IncomingControlFrame::PairingRequest(_) => Err(NekoDropError::Network(
            "pairing request is not a transfer offer".into(),
        )),
    }
}

pub fn accept_transfer_stream_with_decision_and_bundle_staging<D, P>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    _bundle_staging_root: &Path,
    decide: D,
    on_progress: P,
) -> NekoDropResult<TransferReceiveReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    P: FnMut(TransferProgressEvent),
{
    match read_incoming_control_frame(stream)? {
        IncomingControlFrame::FileOffer(offer) => accept_transfer_offer_stream_with_decision(
            stream,
            receive_dir,
            offer,
            decide,
            on_progress,
        ),
        IncomingControlFrame::SessionHello(_) => Err(NekoDropError::Network(
            "session hello requires encrypted control receive entry".into(),
        )),
        IncomingControlFrame::DeviceHello(_) => Err(NekoDropError::Network(
            "device hello is not a transfer offer".into(),
        )),
        IncomingControlFrame::PairingRequest(_) => Err(NekoDropError::Network(
            "pairing request is not a transfer offer".into(),
        )),
    }
}

pub fn accept_incoming_stream<D, H, P>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    decide: D,
    handle_pairing: H,
    on_progress: P,
) -> NekoDropResult<IncomingSessionReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    H: FnOnce(&PairingRequestPayload) -> PairingDecisionPayload,
    P: FnMut(TransferProgressEvent),
{
    accept_incoming_stream_with_cancel(
        stream,
        receive_dir,
        decide,
        handle_pairing,
        on_progress,
        || false,
    )
}

pub fn accept_incoming_stream_with_cancel<D, H, P, C>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    decide: D,
    handle_pairing: H,
    on_progress: P,
    mut should_cancel: C,
) -> NekoDropResult<IncomingSessionReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    H: FnOnce(&PairingRequestPayload) -> PairingDecisionPayload,
    P: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
{
    let frame = read_incoming_control_frame(stream)?;
    accept_plain_incoming_frame_with_cancel(
        stream,
        receive_dir,
        None,
        frame,
        decide,
        handle_pairing,
        on_progress,
        || should_cancel(),
    )
}

pub fn accept_incoming_stream_with_encrypted_control_and_cancel<D, H, P, C>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    receiver_identity: &DeviceIdentity,
    decide: D,
    handle_pairing: H,
    on_progress: P,
    mut should_cancel: C,
) -> NekoDropResult<IncomingSessionReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    H: FnOnce(&PairingRequestPayload) -> PairingDecisionPayload,
    P: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
{
    match read_incoming_control_frame(stream)? {
        IncomingControlFrame::SessionHello(hello) => {
            let mut session = accept_responder_session(stream, receiver_identity, hello)?;
            let offer = session.read_transfer_offer(stream)?;
            accept_transfer_offer_stream_with_encrypted_decision_and_cancel(
                stream,
                receive_dir,
                None,
                offer,
                TransferSecurityMode::EncryptedSession,
                session,
                decide,
                on_progress,
                || should_cancel(),
            )
            .map(IncomingSessionReport::Transfer)
        }
        frame => accept_plain_incoming_frame_with_cancel(
            stream,
            receive_dir,
            None,
            frame,
            decide,
            handle_pairing,
            on_progress,
            || should_cancel(),
        ),
    }
}

pub fn accept_incoming_stream_with_encrypted_control_bundle_staging_and_cancel<D, H, P, C>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    bundle_staging_root: &Path,
    receiver_identity: &DeviceIdentity,
    decide: D,
    handle_pairing: H,
    on_progress: P,
    mut should_cancel: C,
) -> NekoDropResult<IncomingSessionReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    H: FnOnce(&PairingRequestPayload) -> PairingDecisionPayload,
    P: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
{
    match read_incoming_control_frame(stream)? {
        IncomingControlFrame::SessionHello(hello) => {
            let mut session = accept_responder_session(stream, receiver_identity, hello)?;
            let offer = session.read_transfer_offer(stream)?;
            accept_transfer_offer_stream_with_encrypted_decision_and_cancel(
                stream,
                receive_dir,
                Some(bundle_staging_root),
                offer,
                TransferSecurityMode::EncryptedSession,
                session,
                decide,
                on_progress,
                || should_cancel(),
            )
            .map(IncomingSessionReport::Transfer)
        }
        frame => accept_plain_incoming_frame_with_cancel(
            stream,
            receive_dir,
            None,
            frame,
            decide,
            handle_pairing,
            on_progress,
            || should_cancel(),
        ),
    }
}

pub fn accept_incoming_stream_with_authenticated_control_bundle_staging_and_cancel<D, H, P, S, C>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    bundle_staging_root: &Path,
    receiver_identity: &DeviceIdentity,
    sign_identity_binding: S,
    decide: D,
    handle_pairing: H,
    on_progress: P,
    should_cancel: C,
) -> NekoDropResult<IncomingSessionReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    H: FnOnce(&PairingRequestPayload) -> PairingDecisionPayload,
    P: FnMut(TransferProgressEvent),
    S: FnMut(SessionIdentityBinding) -> NekoDropResult<SignedSessionIdentityBinding>,
    C: FnMut() -> bool,
{
    accept_incoming_stream_with_authenticated_control_bundle_staging_peer_verifier_and_cancel(
        stream,
        receive_dir,
        bundle_staging_root,
        receiver_identity,
        sign_identity_binding,
        |_, _| Ok(()),
        decide,
        handle_pairing,
        on_progress,
        should_cancel,
    )
}

pub fn accept_incoming_stream_with_authenticated_control_bundle_staging_peer_verifier_and_cancel<
    D,
    H,
    P,
    S,
    V,
    C,
>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    bundle_staging_root: &Path,
    receiver_identity: &DeviceIdentity,
    sign_identity_binding: S,
    verify_peer_identity: V,
    decide: D,
    handle_pairing: H,
    on_progress: P,
    mut should_cancel: C,
) -> NekoDropResult<IncomingSessionReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    H: FnOnce(&PairingRequestPayload) -> PairingDecisionPayload,
    P: FnMut(TransferProgressEvent),
    S: FnMut(SessionIdentityBinding) -> NekoDropResult<SignedSessionIdentityBinding>,
    V: FnMut(&DeviceIdentity, &SignedSessionIdentityBinding) -> NekoDropResult<()>,
    C: FnMut() -> bool,
{
    match read_incoming_control_frame(stream)? {
        IncomingControlFrame::SessionHello(hello) => {
            let mut session = accept_authenticated_responder_session_with_peer_verifier(
                stream,
                receiver_identity,
                hello,
                sign_identity_binding,
                verify_peer_identity,
            )?;
            let offer = session.read_transfer_offer(stream)?;
            accept_transfer_offer_stream_with_encrypted_decision_and_cancel(
                stream,
                receive_dir,
                Some(bundle_staging_root),
                offer,
                TransferSecurityMode::AuthenticatedEncryptedSession,
                session,
                decide,
                on_progress,
                || should_cancel(),
            )
            .map(IncomingSessionReport::Transfer)
        }
        frame => accept_plain_incoming_frame_with_cancel(
            stream,
            receive_dir,
            None,
            frame,
            decide,
            handle_pairing,
            on_progress,
            || should_cancel(),
        ),
    }
}

fn accept_plain_incoming_frame_with_cancel<D, H, P, C>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    bundle_staging_root: Option<&Path>,
    frame: IncomingControlFrame,
    decide: D,
    handle_pairing: H,
    on_progress: P,
    mut should_cancel: C,
) -> NekoDropResult<IncomingSessionReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    H: FnOnce(&PairingRequestPayload) -> PairingDecisionPayload,
    P: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
{
    match frame {
        IncomingControlFrame::DeviceHello(_) => Err(NekoDropError::Network(
            "device hello is not a transfer or pairing request".into(),
        )),
        IncomingControlFrame::SessionHello(_) => Err(NekoDropError::Network(
            "session hello requires encrypted control handling".into(),
        )),
        IncomingControlFrame::FileOffer(offer) => {
            accept_transfer_offer_stream_with_decision_and_cancel(
                stream,
                receive_dir,
                bundle_staging_root,
                offer,
                decide,
                on_progress,
                || should_cancel(),
            )
            .map(IncomingSessionReport::Transfer)
        }
        IncomingControlFrame::PairingRequest(request) => {
            let decision = handle_pairing(&request);
            write_pairing_decision(stream, &request.request_id, &decision)?;
            Ok(IncomingSessionReport::Pairing(decision))
        }
    }
}

fn accept_transfer_offer_stream_with_decision<D, P>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    offer: TransferOffer,
    decide: D,
    on_progress: P,
) -> NekoDropResult<TransferReceiveReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    P: FnMut(TransferProgressEvent),
{
    accept_transfer_offer_stream_with_decision_and_cancel(
        stream,
        receive_dir,
        None,
        offer,
        decide,
        on_progress,
        || false,
    )
}

fn accept_transfer_offer_stream_with_decision_and_cancel<D, P, C>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    bundle_staging_root: Option<&Path>,
    offer: TransferOffer,
    decide: D,
    on_progress: P,
    mut should_cancel: C,
) -> NekoDropResult<TransferReceiveReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    P: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
{
    accept_transfer_offer_stream_with_decision_writer_and_cancel(
        stream,
        receive_dir,
        bundle_staging_root,
        offer,
        decide,
        on_progress,
        || should_cancel(),
        |stream, offer, decision| {
            write_transfer_decision_for_transfer(stream, &offer.transfer_id, decision)
        },
    )
}

fn accept_transfer_offer_stream_with_encrypted_decision_and_cancel<D, P, C>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    bundle_staging_root: Option<&Path>,
    offer: TransferOffer,
    security_mode: TransferSecurityMode,
    mut session: ActiveSessionControl,
    decide: D,
    on_progress: P,
    mut should_cancel: C,
) -> NekoDropResult<TransferReceiveReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    P: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
{
    validate_offer_sender_identity_matches_session(&offer, &session.peer_identity)?;
    let keys = session.keys.clone();
    accept_encrypted_transfer_offer_stream_with_decision_writer_and_cancel(
        stream,
        receive_dir,
        bundle_staging_root,
        offer,
        security_mode,
        &keys,
        decide,
        on_progress,
        || should_cancel(),
        |stream, _offer, decision| {
            let message_id = session.next_message_id("decision");
            let header = session.next_send_control_header()?;
            write_session_transfer_decision(
                stream,
                session.session_id.clone(),
                message_id,
                &session.keys,
                header,
                decision,
            )
        },
    )
}

fn accept_transfer_offer_stream_with_decision_writer_and_cancel<D, P, C, W>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    bundle_staging_root: Option<&Path>,
    offer: TransferOffer,
    decide: D,
    on_progress: P,
    mut should_cancel: C,
    mut write_decision: W,
) -> NekoDropResult<TransferReceiveReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    P: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
    W: FnMut(&mut TcpStream, &TransferOffer, &TransferDecision) -> NekoDropResult<()>,
{
    if !decide(&offer) {
        write_decision(
            stream,
            &offer,
            &TransferDecision::decline("receiver declined this transfer"),
        )?;
        return Err(NekoDropError::Network(
            "transfer declined by receiver".into(),
        ));
    }
    let resume_plan = match resume_plan_from_offer(receive_dir, &offer) {
        Ok(plan) => plan,
        Err(error) => {
            let _ = write_decision(
                stream,
                &offer,
                &TransferDecision::decline("receiver resume state is not usable"),
            );
            return Err(error);
        }
    };
    if let Err(error) = check_receive_space(receive_dir, offer.total_bytes, &resume_plan) {
        let _ = write_decision(
            stream,
            &offer,
            &TransferDecision::decline("insufficient receive space"),
        );
        return Err(error);
    }
    let decision = TransferDecision::accept_with_resume(resume_files_from_plan(&resume_plan)?);
    write_decision(stream, &offer, &decision)?;
    let resume_offsets = resume_offsets_by_path(&decision.resume_files)?;

    let mut bytes_transferred = resume_plan.total_received_bytes();
    let mut file_index = 0_usize;
    let mut on_progress = on_progress;
    let files =
        receive_file_frames_with_expected_count(stream, offer.file_count, |header, stream| {
            if should_cancel() {
                return Err(NekoDropError::Network("transfer cancelled".into()));
            }
            let expected = offer.files.get(file_index).ok_or_else(|| {
                NekoDropError::Network(format!(
                    "received unexpected extra file frame: {}",
                    header.manifest_path
                ))
            })?;
            if header.manifest_path != expected.manifest_path
                || header.size != expected.size
                || !header.sha256.eq_ignore_ascii_case(&expected.sha256)
            {
                return Err(NekoDropError::Network(format!(
                    "incoming file does not match accepted offer: {}",
                    header.manifest_path
                )));
            }
            let expected_offset = resume_offsets
                .get(header.manifest_path.as_str())
                .copied()
                .unwrap_or(0);
            if header.offset != expected_offset {
                return Err(NekoDropError::Network(format!(
                    "incoming file resume offset does not match accepted decision for {}: {} != {}",
                    header.manifest_path, header.offset, expected_offset
                )));
            }
            file_index += 1;
            on_progress(TransferProgressEvent::Receiving(TransferProgress {
                manifest_path: header.manifest_path.clone(),
                file_index,
                file_count: offer.file_count,
                file_bytes_transferred: header.offset,
                file_size: header.size,
                bytes_transferred,
                total_bytes: offer.total_bytes,
            }));
            let mut last_file_bytes = header.offset;
            let received = write_received_file_with_resume_and_cancel(
                receive_dir,
                &header.manifest_path,
                header.size,
                &header.sha256,
                header.offset,
                stream,
                |file_bytes| {
                    let delta = file_bytes.saturating_sub(last_file_bytes);
                    last_file_bytes = file_bytes;
                    on_progress(TransferProgressEvent::Receiving(TransferProgress {
                        manifest_path: header.manifest_path.clone(),
                        file_index,
                        file_count: offer.file_count,
                        file_bytes_transferred: file_bytes,
                        file_size: header.size,
                        bytes_transferred: bytes_transferred.saturating_add(delta),
                        total_bytes: offer.total_bytes,
                    }));
                },
                || should_cancel(),
            )?;
            bytes_transferred = bytes_transferred
                .saturating_add(received.bytes_written.saturating_sub(header.offset));
            on_progress(TransferProgressEvent::Verifying {
                manifest_path: received.manifest_path.clone(),
                bytes_transferred,
                total_bytes: offer.total_bytes,
            });
            Ok(received)
        })?;
    if files.len() != offer.file_count {
        return Err(NekoDropError::Network(format!(
            "received file count does not match accepted offer: {} != {}",
            files.len(),
            offer.file_count
        )));
    }
    let bundle = maybe_stage_received_bundle(
        receive_dir,
        &offer.root_name,
        bundle_staging_root,
        TransferSecurityMode::LegacyPlain,
    )?;

    Ok(TransferReceiveReport {
        transfer_id: offer.transfer_id,
        root_name: offer.root_name,
        security_mode: TransferSecurityMode::LegacyPlain,
        sender_device_id: offer.sender_device_id,
        sender_device_name: offer.sender_device_name,
        sender_public_key_fingerprint: offer.sender_public_key_fingerprint,
        files,
        bundle,
    })
}

fn accept_encrypted_transfer_offer_stream_with_decision_writer_and_cancel<D, P, C, W>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    bundle_staging_root: Option<&Path>,
    offer: TransferOffer,
    security_mode: TransferSecurityMode,
    keys: &SessionKeyMaterial,
    decide: D,
    on_progress: P,
    mut should_cancel: C,
    mut write_decision: W,
) -> NekoDropResult<TransferReceiveReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    P: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
    W: FnMut(&mut TcpStream, &TransferOffer, &TransferDecision) -> NekoDropResult<()>,
{
    if !decide(&offer) {
        write_decision(
            stream,
            &offer,
            &TransferDecision::decline("receiver declined this transfer"),
        )?;
        return Err(NekoDropError::Network(
            "transfer declined by receiver".into(),
        ));
    }
    let resume_plan = match resume_plan_from_offer(receive_dir, &offer) {
        Ok(plan) => plan,
        Err(error) => {
            let _ = write_decision(
                stream,
                &offer,
                &TransferDecision::decline("receiver resume state is not usable"),
            );
            return Err(error);
        }
    };
    if let Err(error) = check_receive_space(receive_dir, offer.total_bytes, &resume_plan) {
        let _ = write_decision(
            stream,
            &offer,
            &TransferDecision::decline("insufficient receive space"),
        );
        return Err(error);
    }
    let decision = TransferDecision::accept_with_resume(resume_files_from_plan(&resume_plan)?);
    write_decision(stream, &offer, &decision)?;
    let resume_offsets = resume_offsets_by_path(&decision.resume_files)?;

    let mut bytes_transferred = resume_plan.total_received_bytes();
    let mut file_index = 0_usize;
    let mut on_progress = on_progress;
    let files = receive_encrypted_file_frames_with_expected_count(
        stream,
        offer.file_count,
        keys,
        |header, stream| {
            if should_cancel() {
                return Err(NekoDropError::Network("transfer cancelled".into()));
            }
            let expected = offer.files.get(file_index).ok_or_else(|| {
                NekoDropError::Network(format!(
                    "received unexpected extra file frame: {}",
                    header.manifest_path
                ))
            })?;
            if header.manifest_path != expected.manifest_path
                || header.size != expected.size
                || !header.sha256.eq_ignore_ascii_case(&expected.sha256)
            {
                return Err(NekoDropError::Network(format!(
                    "incoming file does not match accepted offer: {}",
                    header.manifest_path
                )));
            }
            let expected_offset = resume_offsets
                .get(header.manifest_path.as_str())
                .copied()
                .unwrap_or(0);
            if header.offset != expected_offset {
                return Err(NekoDropError::Network(format!(
                    "incoming file resume offset does not match accepted decision for {}: {} != {}",
                    header.manifest_path, header.offset, expected_offset
                )));
            }
            file_index += 1;
            on_progress(TransferProgressEvent::Receiving(TransferProgress {
                manifest_path: header.manifest_path.clone(),
                file_index,
                file_count: offer.file_count,
                file_bytes_transferred: header.offset,
                file_size: header.size,
                bytes_transferred,
                total_bytes: offer.total_bytes,
            }));
            let mut last_file_bytes = header.offset;
            let received = write_received_file_with_resume_and_cancel(
                receive_dir,
                &header.manifest_path,
                header.size,
                &header.sha256,
                header.offset,
                stream,
                |file_bytes| {
                    let delta = file_bytes.saturating_sub(last_file_bytes);
                    last_file_bytes = file_bytes;
                    on_progress(TransferProgressEvent::Receiving(TransferProgress {
                        manifest_path: header.manifest_path.clone(),
                        file_index,
                        file_count: offer.file_count,
                        file_bytes_transferred: file_bytes,
                        file_size: header.size,
                        bytes_transferred: bytes_transferred.saturating_add(delta),
                        total_bytes: offer.total_bytes,
                    }));
                },
                || should_cancel(),
            )?;
            bytes_transferred = bytes_transferred
                .saturating_add(received.bytes_written.saturating_sub(header.offset));
            on_progress(TransferProgressEvent::Verifying {
                manifest_path: received.manifest_path.clone(),
                bytes_transferred,
                total_bytes: offer.total_bytes,
            });
            Ok(received)
        },
    )?;
    if files.len() != offer.file_count {
        return Err(NekoDropError::Network(format!(
            "received file count does not match accepted offer: {} != {}",
            files.len(),
            offer.file_count
        )));
    }
    let bundle = maybe_stage_received_bundle(
        receive_dir,
        &offer.root_name,
        bundle_staging_root,
        security_mode,
    )?;

    Ok(TransferReceiveReport {
        transfer_id: offer.transfer_id,
        root_name: offer.root_name,
        security_mode,
        sender_device_id: offer.sender_device_id,
        sender_device_name: offer.sender_device_name,
        sender_public_key_fingerprint: offer.sender_public_key_fingerprint,
        files,
        bundle,
    })
}

fn maybe_stage_received_bundle(
    receive_dir: &Path,
    root_name: &str,
    bundle_staging_root: Option<&Path>,
    security_mode: TransferSecurityMode,
) -> NekoDropResult<Option<ReceivedBundleReport>> {
    let Some(bundle_staging_root) = bundle_staging_root else {
        return Ok(None);
    };
    let received_root = safe_join_receive_path(receive_dir, root_name)?;
    if !received_root.join("bundle.json").exists() {
        return Ok(None);
    }
    let detected = match nekodrop_storage::detect_bundle_directory(&received_root)? {
        Some(detected) => detected,
        None => return Ok(None),
    };
    if !can_stage_received_bundle_for_import(detected.manifest.bundle_type, security_mode) {
        return Ok(None);
    }
    stage_bundle_directory(&received_root, bundle_staging_root)
        .map(staged_bundle_to_report)
        .map(Some)
}

fn can_stage_received_bundle_for_import(
    bundle_type: BundleType,
    security_mode: TransferSecurityMode,
) -> bool {
    if bundle_type.requires_authenticated_encrypted_session() {
        return security_mode == TransferSecurityMode::AuthenticatedEncryptedSession;
    }
    security_mode != TransferSecurityMode::LegacyPlain
}

fn staged_bundle_to_report(staged: StagedBundle) -> ReceivedBundleReport {
    let manifest = staged.detected.manifest;
    ReceivedBundleReport {
        bundle_id: manifest.bundle_id,
        bundle_type: manifest.bundle_type,
        display_name: manifest.display_name,
        source_app: manifest.source_app,
        file_count: manifest.summary.file_count,
        total_bytes: manifest.summary.total_bytes,
        staging_path: staged.staging_path,
        import_allowed: staged.detected.import_policy == BundleImportPolicy::ImportAllowed,
    }
}

fn validate_offer_sender_identity_matches_session(
    offer: &TransferOffer,
    expected_identity: &DeviceIdentity,
) -> NekoDropResult<()> {
    let sender_device_id = offer.sender_device_id.as_deref();
    let sender_device_name = offer.sender_device_name.as_deref();
    let sender_public_key_fingerprint = offer.sender_public_key_fingerprint.as_deref();

    if sender_device_id != Some(expected_identity.device_id.as_str())
        || sender_device_name != Some(expected_identity.device_name.as_str())
        || sender_public_key_fingerprint != Some(expected_identity.public_key_fingerprint.as_str())
    {
        return Err(NekoDropError::Network(
            "offer sender identity does not match encrypted session".into(),
        ));
    }

    Ok(())
}

#[derive(Debug)]
struct ActiveSessionControl {
    session_id: String,
    cipher: String,
    keys: SessionKeyMaterial,
    counters: SessionTrafficCounters,
    receive_window: SessionReplayWindow,
    message_counter: u64,
    peer_identity: DeviceIdentity,
}

impl ActiveSessionControl {
    fn next_message_id(&mut self, label: &str) -> String {
        let counter = self.message_counter;
        self.message_counter = self.message_counter.saturating_add(1);
        format!("{}:{label}-{counter}", self.session_id)
    }

    fn next_send_control_header(&mut self) -> NekoDropResult<SessionTrafficFrameHeader> {
        self.counters
            .next_send_header(&self.cipher, SessionFrameKind::Control)
            .map_err(protocol_error_to_service)
    }

    fn read_transfer_decision<S>(&mut self, stream: &mut S) -> NekoDropResult<TransferDecision>
    where
        S: Read,
    {
        read_session_transfer_decision_once(stream, &self.keys, &mut self.receive_window)
    }

    fn read_transfer_offer<S>(&mut self, stream: &mut S) -> NekoDropResult<TransferOffer>
    where
        S: Read,
    {
        read_session_transfer_offer_once(stream, &self.keys, &mut self.receive_window)
    }
}

fn start_initiator_session<S>(
    stream: &mut S,
    sender_identity: &DeviceIdentity,
    transfer_id: &str,
) -> NekoDropResult<ActiveSessionControl>
where
    S: Read + Write,
{
    require_encrypted_session_identity(sender_identity)?;
    let key_pair = SessionEphemeralKeyPair::generate().map_err(protocol_error_to_service)?;
    let hello = SessionHelloPayload::default_crypto(
        format!("session-{transfer_id}"),
        sender_identity.clone(),
        key_pair.public_key.clone(),
    );
    write_session_hello(stream, &hello)?;
    let ready = read_verified_session_ready(stream, &hello)?;
    active_session_from_handshake(
        &hello,
        &ready,
        sender_identity,
        &key_pair,
        &ready.ephemeral_public_key,
        ready.identity.clone(),
    )
}

fn start_authenticated_initiator_session_with_peer_verifier<S, F, V>(
    stream: &mut S,
    sender_identity: &DeviceIdentity,
    transfer_id: &str,
    sign_identity_binding: F,
    verify_peer_identity: V,
) -> NekoDropResult<ActiveSessionControl>
where
    S: Read + Write,
    F: FnMut(SessionIdentityBinding) -> NekoDropResult<SignedSessionIdentityBinding>,
    V: FnMut(&DeviceIdentity, &SignedSessionIdentityBinding) -> NekoDropResult<()>,
{
    require_encrypted_session_identity(sender_identity)?;
    let key_pair = SessionEphemeralKeyPair::generate().map_err(protocol_error_to_service)?;
    let hello = SessionHelloPayload::default_crypto(
        format!("session-{transfer_id}"),
        sender_identity.clone(),
        key_pair.public_key.clone(),
    );
    write_session_hello(stream, &hello)?;
    let ready = read_verified_session_ready(stream, &hello)?;
    let handshake =
        VerifiedSessionHandshake::from_ready(&hello, &ready).map_err(protocol_error_to_service)?;
    exchange_initiator_identity_binding(
        stream,
        &handshake,
        sign_identity_binding,
        verify_peer_identity,
    )?;
    active_session_from_verified_handshake(
        handshake,
        sender_identity,
        &key_pair,
        &ready.ephemeral_public_key,
        ready.identity.clone(),
    )
}

fn accept_responder_session(
    stream: &mut TcpStream,
    receiver_identity: &DeviceIdentity,
    hello: SessionHelloPayload,
) -> NekoDropResult<ActiveSessionControl> {
    require_encrypted_session_identity(receiver_identity)?;
    let key_pair = SessionEphemeralKeyPair::generate().map_err(protocol_error_to_service)?;
    let ready = SessionReadyPayload::for_hello_with_cipher_preference(
        &hello,
        receiver_identity.clone(),
        key_pair.public_key.clone(),
        &default_session_cipher_preference(),
    )
    .map_err(protocol_error_to_service)?;
    write_session_ready(stream, &ready)?;
    active_session_from_handshake(
        &hello,
        &ready,
        receiver_identity,
        &key_pair,
        &hello.ephemeral_public_key,
        hello.identity.clone(),
    )
}

fn accept_authenticated_responder_session_with_peer_verifier<F, V>(
    stream: &mut TcpStream,
    receiver_identity: &DeviceIdentity,
    hello: SessionHelloPayload,
    sign_identity_binding: F,
    verify_peer_identity: V,
) -> NekoDropResult<ActiveSessionControl>
where
    F: FnMut(SessionIdentityBinding) -> NekoDropResult<SignedSessionIdentityBinding>,
    V: FnMut(&DeviceIdentity, &SignedSessionIdentityBinding) -> NekoDropResult<()>,
{
    require_encrypted_session_identity(receiver_identity)?;
    let key_pair = SessionEphemeralKeyPair::generate().map_err(protocol_error_to_service)?;
    let ready = SessionReadyPayload::for_hello_with_cipher_preference(
        &hello,
        receiver_identity.clone(),
        key_pair.public_key.clone(),
        &default_session_cipher_preference(),
    )
    .map_err(protocol_error_to_service)?;
    write_session_ready(stream, &ready)?;
    let handshake =
        VerifiedSessionHandshake::from_ready(&hello, &ready).map_err(protocol_error_to_service)?;
    exchange_responder_identity_binding(
        stream,
        &handshake,
        sign_identity_binding,
        verify_peer_identity,
    )?;
    active_session_from_verified_handshake(
        handshake,
        receiver_identity,
        &key_pair,
        &hello.ephemeral_public_key,
        hello.identity.clone(),
    )
}

fn active_session_from_handshake(
    hello: &SessionHelloPayload,
    ready: &SessionReadyPayload,
    local_identity: &DeviceIdentity,
    key_pair: &SessionEphemeralKeyPair,
    peer_ephemeral_public_key: &str,
    peer_identity: DeviceIdentity,
) -> NekoDropResult<ActiveSessionControl> {
    let handshake =
        VerifiedSessionHandshake::from_ready(hello, ready).map_err(protocol_error_to_service)?;
    active_session_from_verified_handshake(
        handshake,
        local_identity,
        key_pair,
        peer_ephemeral_public_key,
        peer_identity,
    )
}

fn active_session_from_verified_handshake(
    handshake: VerifiedSessionHandshake,
    local_identity: &DeviceIdentity,
    key_pair: &SessionEphemeralKeyPair,
    peer_ephemeral_public_key: &str,
    peer_identity: DeviceIdentity,
) -> NekoDropResult<ActiveSessionControl> {
    let shared_secret = key_pair
        .shared_secret_from_peer_public_key(peer_ephemeral_public_key)
        .map_err(protocol_error_to_service)?;
    let key_context = handshake
        .key_derivation_context_for_local_device(&local_identity.device_id)
        .map_err(protocol_error_to_service)?;
    let keys = key_context
        .derive_key_material(&shared_secret)
        .map_err(protocol_error_to_service)?;

    Ok(ActiveSessionControl {
        session_id: handshake.session_id,
        cipher: handshake.cipher,
        keys,
        counters: SessionTrafficCounters::default(),
        receive_window: SessionReplayWindow::default(),
        message_counter: 0,
        peer_identity,
    })
}

fn exchange_initiator_identity_binding<S, F>(
    stream: &mut S,
    handshake: &VerifiedSessionHandshake,
    mut sign_identity_binding: F,
    mut verify_peer_identity: impl FnMut(
        &DeviceIdentity,
        &SignedSessionIdentityBinding,
    ) -> NekoDropResult<()>,
) -> NekoDropResult<()>
where
    S: Read + Write,
    F: FnMut(SessionIdentityBinding) -> NekoDropResult<SignedSessionIdentityBinding>,
{
    let local_binding =
        SessionIdentityBinding::for_initiator(handshake).map_err(protocol_error_to_service)?;
    let signed_local = sign_identity_binding(local_binding)?;
    write_session_identity_binding(stream, &signed_local)?;

    let signed_peer = read_session_identity_binding(stream)?;
    let expected_peer =
        SessionIdentityBinding::for_responder(handshake).map_err(protocol_error_to_service)?;
    verify_signed_session_identity_binding(&signed_peer, &expected_peer, &handshake.responder)?;
    verify_peer_identity(&handshake.responder, &signed_peer)
}

fn exchange_responder_identity_binding<F>(
    stream: &mut TcpStream,
    handshake: &VerifiedSessionHandshake,
    mut sign_identity_binding: F,
    mut verify_peer_identity: impl FnMut(
        &DeviceIdentity,
        &SignedSessionIdentityBinding,
    ) -> NekoDropResult<()>,
) -> NekoDropResult<()>
where
    F: FnMut(SessionIdentityBinding) -> NekoDropResult<SignedSessionIdentityBinding>,
{
    let signed_peer = read_session_identity_binding(stream)?;
    let expected_peer =
        SessionIdentityBinding::for_initiator(handshake).map_err(protocol_error_to_service)?;
    verify_signed_session_identity_binding(&signed_peer, &expected_peer, &handshake.initiator)?;
    verify_peer_identity(&handshake.initiator, &signed_peer)?;

    let local_binding =
        SessionIdentityBinding::for_responder(handshake).map_err(protocol_error_to_service)?;
    let signed_local = sign_identity_binding(local_binding)?;
    write_session_identity_binding(stream, &signed_local)
}

fn verify_signed_session_identity_binding(
    signed_binding: &SignedSessionIdentityBinding,
    expected_binding: &SessionIdentityBinding,
    expected_identity: &DeviceIdentity,
) -> NekoDropResult<()> {
    expected_binding
        .verify_identity(expected_identity)
        .map_err(protocol_error_to_service)?;
    if signed_binding.public_key_fingerprint != expected_identity.public_key_fingerprint {
        return Err(protocol_error_to_service(ProtocolError::new(
            nekolink_protocol::ErrorCode::InvalidPayload,
            "session identity signature public key mismatch",
        )));
    }
    signed_binding
        .verify(expected_binding)
        .map_err(protocol_error_to_service)
}

fn require_encrypted_session_identity(identity: &DeviceIdentity) -> NekoDropResult<()> {
    identity
        .require_capability(Capability::EncryptedSession)
        .map_err(protocol_error_to_service)
}

fn protocol_error_to_service(error: ProtocolError) -> NekoDropError {
    NekoDropError::Network(format!("{:?}: {}", error.code, error.message))
}

pub fn outgoing_frames_from_plan(plan: &TransferSourcePlan) -> Vec<OutgoingFileFrame> {
    plan.files
        .iter()
        .map(|file| {
            OutgoingFileFrame::new(
                file.manifest_path.clone(),
                file.source_path.clone(),
                file.sha256.clone(),
            )
        })
        .collect()
}

pub fn offer_from_plan(plan: &TransferSourcePlan) -> TransferOffer {
    offer_from_plan_with_sender_identity(plan, None)
}

pub fn offer_from_plan_with_sender_identity(
    plan: &TransferSourcePlan,
    sender_identity: Option<&DeviceIdentity>,
) -> TransferOffer {
    let offer = TransferOffer::new(
        next_transfer_id(),
        plan.manifest.root_name.clone(),
        plan.files
            .iter()
            .map(|file| TransferOfferFile {
                manifest_path: file.manifest_path.clone(),
                size: file.size,
                sha256: file.sha256.clone(),
            })
            .collect(),
    );

    if let Some(identity) = sender_identity {
        return offer.with_sender_identity(identity);
    }

    offer
}

fn resume_plan_from_offer(receive_dir: &Path, offer: &TransferOffer) -> NekoDropResult<ResumePlan> {
    let expected_files = offer
        .files
        .iter()
        .map(|file| {
            ResumeExpectedFile::new(
                file.manifest_path.clone(),
                file.size,
                Some(file.sha256.clone()),
            )
        })
        .collect::<NekoDropResult<Vec<_>>>()?;
    build_resume_plan_for_files(receive_dir, &offer.transfer_id, &expected_files)
}

fn resume_files_from_plan(plan: &ResumePlan) -> NekoDropResult<Vec<TransferResumeFile>> {
    plan.files
        .iter()
        .map(|file| {
            TransferResumeFile::new(file.path.clone(), file.received_bytes).map_err(|error| {
                NekoDropError::Network(format!("{:?}: {}", error.code, error.message))
            })
        })
        .collect()
}

fn resume_offsets_by_path(
    resume_files: &[TransferResumeFile],
) -> NekoDropResult<HashMap<&str, u64>> {
    let mut offsets = HashMap::new();
    for file in resume_files {
        if offsets
            .insert(file.manifest_path.as_str(), file.received_bytes)
            .is_some()
        {
            return Err(NekoDropError::Network(format!(
                "duplicate resume path: {}",
                file.manifest_path
            )));
        }
    }
    Ok(offsets)
}

fn next_transfer_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("transfer-{millis}")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::net::TcpListener;
    use std::thread;

    use nekolink_protocol::{BundleType, DeviceIdentitySigningKey, DeviceKind, PlatformKind};

    use super::*;

    #[test]
    fn creates_and_parses_connection_code() {
        let code =
            connection_code_for_endpoint(Endpoint::tcp("127.0.0.1", 45821), Some("Desktop PC"))
                .unwrap();
        let endpoint = endpoint_from_connection_code(&code).unwrap();

        assert_eq!(endpoint, Endpoint::tcp("127.0.0.1", 45821));
        assert!(code.contains("name=Desktop%20PC"));
    }

    #[test]
    fn service_sends_selected_directory_and_receiver_writes_verified_files() {
        let dir = unique_temp_dir("service-loopback");
        let source_root = dir.join("source").join("drop");
        let receive_dir = dir.join("receive");
        fs::create_dir_all(source_root.join("nested")).unwrap();
        fs::create_dir_all(&receive_dir).unwrap();
        fs::write(source_root.join("nested").join("one.txt"), b"one").unwrap();
        fs::write(source_root.join("two.txt"), b"two").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            move || accept_transfer(&listener, &receive_dir)
        });

        let send_report = send_paths(&endpoint, &[source_root]).unwrap();
        let receive_report = receiver.join().unwrap().unwrap();

        assert_eq!(send_report.plan.file_count(), 2);
        assert_eq!(send_report.sent_files.len(), 2);
        assert_eq!(
            receive_report.security_mode,
            TransferSecurityMode::LegacyPlain
        );
        assert_eq!(receive_report.files.len(), 2);
        assert_eq!(receive_report.bundle, None);
        assert!(receive_report.files.iter().all(|file| file.verified));
        assert_eq!(
            fs::read_to_string(receive_dir.join("drop/nested/one.txt")).unwrap(),
            "one"
        );
        assert_eq!(
            fs::read_to_string(receive_dir.join("drop/two.txt")).unwrap(),
            "two"
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn legacy_plain_receive_keeps_bundle_as_plain_files_only() {
        let dir = unique_temp_dir("service-plain-bundle-not-staged");
        let source_root = create_valid_bundle_source(&dir);
        let receive_dir = dir.join("receive");
        let staging_root = dir.join("staging");
        fs::create_dir_all(&receive_dir).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            let staging_root = staging_root.clone();
            move || accept_transfer_with_bundle_staging(&listener, &receive_dir, &staging_root)
        });

        send_paths(&endpoint, &[source_root]).unwrap();
        let receive_report = receiver.join().unwrap().unwrap();

        assert_eq!(
            receive_report.security_mode,
            TransferSecurityMode::LegacyPlain
        );
        assert_eq!(receive_report.bundle, None);
        assert!(receive_dir.join("bundle").join("bundle.json").is_file());
        assert!(!staging_root.join("bundle_1234567890").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn encrypted_control_plain_fallback_keeps_bundle_as_plain_files_only() {
        let dir = unique_temp_dir("service-encrypted-control-plain-bundle-not-staged");
        let source_root = create_valid_bundle_source(&dir);
        let receive_dir = dir.join("receive");
        let staging_root = dir.join("staging");
        fs::create_dir_all(&receive_dir).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
        let receiver_identity = test_identity("neko-device-receiver", "Receiver Windows");

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            let staging_root = staging_root.clone();
            let receiver_identity = receiver_identity.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                accept_incoming_stream_with_encrypted_control_bundle_staging_and_cancel(
                    &mut stream,
                    &receive_dir,
                    &staging_root,
                    &receiver_identity,
                    |_| true,
                    |_| panic!("pairing should not be handled on transfer path"),
                    |_| {},
                    || false,
                )
            }
        });

        send_paths(&endpoint, &[source_root]).unwrap();
        let receive_report = match receiver.join().unwrap().unwrap() {
            IncomingSessionReport::Transfer(report) => report,
            IncomingSessionReport::Pairing(_) => panic!("expected transfer report"),
        };

        assert_eq!(
            receive_report.security_mode,
            TransferSecurityMode::LegacyPlain
        );
        assert_eq!(receive_report.bundle, None);
        assert!(receive_dir.join("bundle").join("bundle.json").is_file());
        assert!(!staging_root.join("bundle_1234567890").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn encrypted_session_without_authenticated_identity_keeps_sensitive_bundle_as_plain_files_only()
    {
        let dir = unique_temp_dir("service-encrypted-sensitive-bundle-not-staged");
        let source_root = create_valid_bundle_source(&dir);
        let receive_dir = dir.join("receive");
        let staging_root = dir.join("staging");
        fs::create_dir_all(&receive_dir).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
        let sender = test_identity("neko-device-sender", "Sender Mac");
        let receiver_identity = test_identity("neko-device-receiver", "Receiver Windows");

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            let staging_root = staging_root.clone();
            let receiver_identity = receiver_identity.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                accept_incoming_stream_with_encrypted_control_bundle_staging_and_cancel(
                    &mut stream,
                    &receive_dir,
                    &staging_root,
                    &receiver_identity,
                    |_| true,
                    |_| panic!("pairing should not be handled on encrypted transfer path"),
                    |_| {},
                    || false,
                )
            }
        });

        let plan = create_transfer_plan(&[source_root]).unwrap();
        send_plan_with_encrypted_control_and_cancel(&endpoint, plan, &sender, |_| {}, || false)
            .unwrap();
        let receive_report = match receiver.join().unwrap().unwrap() {
            IncomingSessionReport::Transfer(report) => report,
            IncomingSessionReport::Pairing(_) => panic!("expected transfer report"),
        };

        assert_eq!(
            receive_report.security_mode,
            TransferSecurityMode::EncryptedSession
        );
        assert_eq!(receive_report.bundle, None);
        assert!(receive_dir.join("bundle").join("bundle.json").is_file());
        assert!(!staging_root.join("bundle_1234567890").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn encrypted_session_transfer_sends_control_and_file_payload_through_session() {
        let dir = unique_temp_dir("service-encrypted-session-loopback");
        let source_root = dir.join("source").join("drop");
        let receive_dir = dir.join("receive");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&receive_dir).unwrap();
        fs::write(source_root.join("sample.txt"), b"encrypted file payload").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
        let sender = test_identity("neko-device-sender", "Sender Mac");
        let receiver_identity = test_identity("neko-device-receiver", "Receiver Windows");

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            let receiver_identity = receiver_identity.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                accept_incoming_stream_with_encrypted_control_and_cancel(
                    &mut stream,
                    &receive_dir,
                    &receiver_identity,
                    |_| true,
                    |_| panic!("pairing should not be handled on encrypted transfer path"),
                    |_| {},
                    || false,
                )
            }
        });

        let plan = create_transfer_plan(&[source_root]).unwrap();
        let send_report =
            send_plan_with_encrypted_control_and_cancel(&endpoint, plan, &sender, |_| {}, || false)
                .unwrap();
        let receive_report = match receiver.join().unwrap().unwrap() {
            IncomingSessionReport::Transfer(report) => report,
            IncomingSessionReport::Pairing(_) => panic!("expected transfer report"),
        };

        assert_eq!(send_report.sent_files.len(), 1);
        assert_eq!(
            receive_report.security_mode,
            TransferSecurityMode::EncryptedSession
        );
        assert_eq!(receive_report.files.len(), 1);
        assert_eq!(
            fs::read_to_string(receive_dir.join("drop/sample.txt")).unwrap(),
            "encrypted file payload"
        );
        assert_eq!(
            receive_report.sender_device_id.as_deref(),
            Some("neko-device-sender")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn encrypted_session_transfer_does_not_write_plain_file_payload_on_wire() {
        let dir = unique_temp_dir("service-encrypted-session-wire");
        let source_root = dir.join("source").join("drop");
        fs::create_dir_all(&source_root).unwrap();
        fs::write(source_root.join("sample.txt"), b"secret-payload-marker").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
        let sender = test_identity("neko-device-sender", "Sender Mac");
        let receiver_identity = test_identity("neko-device-receiver", "Receiver Windows");

        let receiver = thread::spawn({
            let receiver_identity = receiver_identity.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                let frame = read_incoming_control_frame(&mut stream).unwrap();
                let IncomingControlFrame::SessionHello(hello) = frame else {
                    panic!("expected encrypted session hello");
                };
                let mut session = accept_responder_session(&mut stream, &receiver_identity, hello)
                    .expect("session should be established");
                let offer = session.read_transfer_offer(&mut stream).unwrap();
                let message_id = session.next_message_id("decision");
                let header = session.next_send_control_header().unwrap();
                write_session_transfer_decision(
                    &mut stream,
                    session.session_id.clone(),
                    message_id,
                    &session.keys,
                    header,
                    &TransferDecision::accept(),
                )
                .unwrap();
                let mut raw_payload = Vec::new();
                stream.read_to_end(&mut raw_payload).unwrap();
                assert_eq!(offer.file_count, 1);
                raw_payload
            }
        });

        let plan = create_transfer_plan(&[source_root]).unwrap();
        send_plan_with_encrypted_control_and_cancel(&endpoint, plan, &sender, |_| {}, || false)
            .unwrap();
        let raw_payload = receiver.join().unwrap();

        assert!(!String::from_utf8_lossy(&raw_payload).contains("secret-payload-marker"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn encrypted_session_transfer_resumes_from_existing_partial_file() {
        let dir = unique_temp_dir("service-encrypted-session-resume");
        let source_root = dir.join("source").join("drop");
        let receive_dir = dir.join("receive");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(receive_dir.join("drop")).unwrap();
        fs::write(source_root.join("sample.txt"), b"hello encrypted resume").unwrap();
        fs::write(
            receive_dir.join("drop/sample.txt.nekodrop-part"),
            b"hello encrypted ",
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
        let sender = test_identity("neko-device-sender", "Sender Mac");
        let receiver_identity = test_identity("neko-device-receiver", "Receiver Windows");

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            let receiver_identity = receiver_identity.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                accept_incoming_stream_with_encrypted_control_and_cancel(
                    &mut stream,
                    &receive_dir,
                    &receiver_identity,
                    |_| true,
                    |_| panic!("pairing should not be handled on encrypted transfer path"),
                    |_| {},
                    || false,
                )
            }
        });

        let plan = create_transfer_plan(&[source_root]).unwrap();
        let send_report =
            send_plan_with_encrypted_control_and_cancel(&endpoint, plan, &sender, |_| {}, || false)
                .unwrap();
        let receive_report = match receiver.join().unwrap().unwrap() {
            IncomingSessionReport::Transfer(report) => report,
            IncomingSessionReport::Pairing(_) => panic!("expected transfer report"),
        };

        assert_eq!(send_report.sent_files.len(), 1);
        assert_eq!(send_report.sent_files[0].bytes_sent, 6);
        assert_eq!(receive_report.files.len(), 1);
        assert_eq!(
            fs::read_to_string(receive_dir.join("drop/sample.txt")).unwrap(),
            "hello encrypted resume"
        );
        assert!(!receive_dir.join("drop/sample.txt.nekodrop-part").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn authenticated_session_rejects_initiator_signature_for_wrong_identity() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
        let sender_key = DeviceIdentitySigningKey::from_seed([7_u8; 32]);
        let receiver_key = DeviceIdentitySigningKey::from_seed([8_u8; 32]);
        let wrong_sender_key = DeviceIdentitySigningKey::from_seed([9_u8; 32]);
        let sender =
            test_identity_with_signing_key("neko-device-sender", "Sender Mac", &sender_key);
        let receiver_identity = test_identity_with_signing_key(
            "neko-device-receiver",
            "Receiver Windows",
            &receiver_key,
        );

        let receiver = thread::spawn({
            let receiver_identity = receiver_identity.clone();
            let receiver_key = receiver_key.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                let frame = read_incoming_control_frame(&mut stream).unwrap();
                let IncomingControlFrame::SessionHello(hello) = frame else {
                    panic!("expected encrypted session hello");
                };
                accept_authenticated_responder_session_with_peer_verifier(
                    &mut stream,
                    &receiver_identity,
                    hello,
                    |binding| {
                        nekolink_protocol::SignedSessionIdentityBinding::sign(
                            binding,
                            &receiver_key,
                        )
                        .map_err(protocol_error_to_service)
                    },
                    |_, _| Ok(()),
                )
            }
        });

        let sender_result = {
            let mut stream = connect_endpoint(&endpoint).unwrap();
            start_authenticated_initiator_session_with_peer_verifier(
                &mut stream,
                &sender,
                "transfer-auth-fail",
                |binding| {
                    nekolink_protocol::SignedSessionIdentityBinding::sign(
                        binding,
                        &wrong_sender_key,
                    )
                    .map_err(protocol_error_to_service)
                },
                |_, _| Ok(()),
            )
        };
        let receiver_result = receiver.join().unwrap();

        assert!(sender_result.is_err());
        assert!(receiver_result.is_err());
        assert!(receiver_result
            .unwrap_err()
            .to_string()
            .contains("public key mismatch"));
    }

    #[test]
    fn authenticated_responder_session_applies_peer_policy_after_signature_verification() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
        let sender_key = DeviceIdentitySigningKey::from_seed([21_u8; 32]);
        let receiver_key = DeviceIdentitySigningKey::from_seed([22_u8; 32]);
        let sender =
            test_identity_with_signing_key("neko-device-sender", "Sender Mac", &sender_key);
        let receiver_identity = test_identity_with_signing_key(
            "neko-device-receiver",
            "Receiver Windows",
            &receiver_key,
        );

        let receiver = thread::spawn({
            let receiver_identity = receiver_identity.clone();
            let receiver_key = receiver_key.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                let frame = read_incoming_control_frame(&mut stream).unwrap();
                let IncomingControlFrame::SessionHello(hello) = frame else {
                    panic!("expected encrypted session hello");
                };
                accept_authenticated_responder_session_with_peer_verifier(
                    &mut stream,
                    &receiver_identity,
                    hello,
                    |binding| {
                        nekolink_protocol::SignedSessionIdentityBinding::sign(
                            binding,
                            &receiver_key,
                        )
                        .map_err(protocol_error_to_service)
                    },
                    |_identity, _signed_binding| {
                        Err(NekoDropError::Network("peer key is not trusted".into()))
                    },
                )
            }
        });

        let sender_result = {
            let mut stream = connect_endpoint(&endpoint).unwrap();
            start_authenticated_initiator_session_with_peer_verifier(
                &mut stream,
                &sender,
                "transfer-peer-policy-fail",
                |binding| {
                    nekolink_protocol::SignedSessionIdentityBinding::sign(binding, &sender_key)
                        .map_err(protocol_error_to_service)
                },
                |_, _| Ok(()),
            )
        };
        let receiver_result = receiver.join().unwrap();

        assert!(sender_result.is_err());
        assert!(receiver_result.is_err());
        assert!(receiver_result
            .unwrap_err()
            .to_string()
            .contains("peer key is not trusted"));
    }

    #[test]
    fn authenticated_responder_session_peer_policy_can_check_signed_public_key() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
        let sender_key = DeviceIdentitySigningKey::from_seed([23_u8; 32]);
        let receiver_key = DeviceIdentitySigningKey::from_seed([24_u8; 32]);
        let sender =
            test_identity_with_signing_key("neko-device-sender", "Sender Mac", &sender_key);
        let receiver_identity = test_identity_with_signing_key(
            "neko-device-receiver",
            "Receiver Windows",
            &receiver_key,
        );
        let sender_public_key = sender_key.public_key().public_key;

        let receiver = thread::spawn({
            let receiver_identity = receiver_identity.clone();
            let receiver_key = receiver_key.clone();
            let expected_public_key = sender_public_key.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                let frame = read_incoming_control_frame(&mut stream).unwrap();
                let IncomingControlFrame::SessionHello(hello) = frame else {
                    panic!("expected encrypted session hello");
                };
                accept_authenticated_responder_session_with_peer_verifier(
                    &mut stream,
                    &receiver_identity,
                    hello,
                    |binding| {
                        nekolink_protocol::SignedSessionIdentityBinding::sign(
                            binding,
                            &receiver_key,
                        )
                        .map_err(protocol_error_to_service)
                    },
                    |_identity, signed_binding| {
                        if signed_binding.public_key == expected_public_key {
                            Ok(())
                        } else {
                            Err(NekoDropError::Network(
                                "peer public key does not match trusted record".into(),
                            ))
                        }
                    },
                )
            }
        });

        let sender_result = {
            let mut stream = connect_endpoint(&endpoint).unwrap();
            start_authenticated_initiator_session_with_peer_verifier(
                &mut stream,
                &sender,
                "transfer-peer-public-key-policy",
                |binding| {
                    nekolink_protocol::SignedSessionIdentityBinding::sign(binding, &sender_key)
                        .map_err(protocol_error_to_service)
                },
                |_, _| Ok(()),
            )
        };
        let receiver_result = receiver.join().unwrap();

        assert!(sender_result.is_ok());
        assert!(receiver_result.is_ok());
    }

    #[test]
    fn authenticated_session_transfer_exchanges_signed_identities_before_files() {
        let dir = unique_temp_dir("service-authenticated-session-loopback");
        let source_root = dir.join("source").join("drop");
        let receive_dir = dir.join("receive");
        let staging_root = dir.join("staging");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&receive_dir).unwrap();
        fs::write(
            source_root.join("sample.txt"),
            b"authenticated file payload",
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
        let sender_key = DeviceIdentitySigningKey::from_seed([10_u8; 32]);
        let receiver_key = DeviceIdentitySigningKey::from_seed([11_u8; 32]);
        let sender =
            test_identity_with_signing_key("neko-device-sender", "Sender Mac", &sender_key);
        let receiver_identity = test_identity_with_signing_key(
            "neko-device-receiver",
            "Receiver Windows",
            &receiver_key,
        );

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            let staging_root = staging_root.clone();
            let receiver_identity = receiver_identity.clone();
            let receiver_key = receiver_key.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                accept_incoming_stream_with_authenticated_control_bundle_staging_and_cancel(
                    &mut stream,
                    &receive_dir,
                    &staging_root,
                    &receiver_identity,
                    |binding| {
                        nekolink_protocol::SignedSessionIdentityBinding::sign(
                            binding,
                            &receiver_key,
                        )
                        .map_err(protocol_error_to_service)
                    },
                    |_| true,
                    |_| panic!("pairing should not be handled on authenticated transfer path"),
                    |_| {},
                    || false,
                )
            }
        });

        let plan = create_transfer_plan(&[source_root]).unwrap();
        let send_report = send_plan_with_authenticated_session_and_cancel(
            &endpoint,
            plan,
            &sender,
            |binding| {
                nekolink_protocol::SignedSessionIdentityBinding::sign(binding, &sender_key)
                    .map_err(protocol_error_to_service)
            },
            |_| {},
            || false,
        )
        .unwrap();
        let receive_report = match receiver.join().unwrap().unwrap() {
            IncomingSessionReport::Transfer(report) => report,
            IncomingSessionReport::Pairing(_) => panic!("expected transfer report"),
        };

        assert_eq!(send_report.sent_files.len(), 1);
        assert_eq!(
            receive_report.security_mode,
            TransferSecurityMode::AuthenticatedEncryptedSession
        );
        assert_eq!(receive_report.files.len(), 1);
        assert_eq!(
            fs::read_to_string(receive_dir.join("drop/sample.txt")).unwrap(),
            "authenticated file payload"
        );
        assert_eq!(
            receive_report.sender_device_id.as_deref(),
            Some("neko-device-sender")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn authenticated_session_receive_stages_bundle_for_import() {
        let dir = unique_temp_dir("service-authenticated-bundle-staging");
        let source_root = create_valid_bundle_source(&dir);
        let receive_dir = dir.join("receive");
        let staging_root = dir.join("staging");
        fs::create_dir_all(&receive_dir).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
        let sender_key = DeviceIdentitySigningKey::from_seed([12_u8; 32]);
        let receiver_key = DeviceIdentitySigningKey::from_seed([13_u8; 32]);
        let sender =
            test_identity_with_signing_key("neko-device-sender", "Sender Mac", &sender_key);
        let receiver_identity = test_identity_with_signing_key(
            "neko-device-receiver",
            "Receiver Windows",
            &receiver_key,
        );

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            let staging_root = staging_root.clone();
            let receiver_identity = receiver_identity.clone();
            let receiver_key = receiver_key.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                accept_incoming_stream_with_authenticated_control_bundle_staging_and_cancel(
                    &mut stream,
                    &receive_dir,
                    &staging_root,
                    &receiver_identity,
                    |binding| {
                        nekolink_protocol::SignedSessionIdentityBinding::sign(
                            binding,
                            &receiver_key,
                        )
                        .map_err(protocol_error_to_service)
                    },
                    |_| true,
                    |_| panic!("pairing should not be handled on authenticated transfer path"),
                    |_| {},
                    || false,
                )
            }
        });

        let plan = create_transfer_plan(&[source_root]).unwrap();
        send_plan_with_authenticated_session_and_cancel(
            &endpoint,
            plan,
            &sender,
            |binding| {
                nekolink_protocol::SignedSessionIdentityBinding::sign(binding, &sender_key)
                    .map_err(protocol_error_to_service)
            },
            |_| {},
            || false,
        )
        .unwrap();
        let receive_report = match receiver.join().unwrap().unwrap() {
            IncomingSessionReport::Transfer(report) => report,
            IncomingSessionReport::Pairing(_) => panic!("expected transfer report"),
        };
        let bundle = receive_report.bundle.expect("bundle should be staged");

        assert_eq!(
            receive_report.security_mode,
            TransferSecurityMode::AuthenticatedEncryptedSession
        );
        assert_eq!(bundle.bundle_id, "bundle_1234567890");
        assert_eq!(bundle.bundle_type, BundleType::Skill);
        assert_eq!(bundle.display_name, "voice_transcribe");
        assert_eq!(bundle.source_app, "OpenNeko");
        assert_eq!(bundle.file_count, 2);
        assert_eq!(bundle.total_bytes, 28);
        assert_eq!(bundle.staging_path, staging_root.join("bundle_1234567890"));
        assert!(bundle.import_allowed);
        assert!(bundle.staging_path.join("bundle.json").is_file());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn encrypted_control_receiver_declines_before_files_are_sent() {
        let dir = unique_temp_dir("service-encrypted-control-decline");
        let source_root = dir.join("source").join("drop");
        let receive_dir = dir.join("receive");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&receive_dir).unwrap();
        fs::write(source_root.join("sample.txt"), b"declined").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
        let sender = test_identity("neko-device-sender", "Sender Mac");
        let receiver_identity = test_identity("neko-device-receiver", "Receiver Windows");

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                accept_incoming_stream_with_encrypted_control_and_cancel(
                    &mut stream,
                    &receive_dir,
                    &receiver_identity,
                    |_| false,
                    |_| panic!("pairing should not be handled on encrypted transfer path"),
                    |_| {},
                    || false,
                )
            }
        });

        let plan = create_transfer_plan(&[source_root]).unwrap();
        let send_result =
            send_plan_with_encrypted_control_and_cancel(&endpoint, plan, &sender, |_| {}, || false);
        let receive_result = receiver.join().unwrap();

        assert!(send_result.is_err());
        assert!(receive_result.is_err());
        assert!(!receive_dir.join("drop/sample.txt").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn active_session_control_rejects_replayed_encrypted_decision() {
        let mut session = ActiveSessionControl {
            session_id: "session-1".to_string(),
            cipher: nekolink_protocol::SESSION_CIPHER_XCHACHA20POLY1305.to_string(),
            keys: SessionKeyMaterial {
                send_key: [23_u8; nekolink_protocol::SESSION_TRAFFIC_KEY_LEN],
                receive_key: [23_u8; nekolink_protocol::SESSION_TRAFFIC_KEY_LEN],
            },
            counters: SessionTrafficCounters::default(),
            receive_window: SessionReplayWindow::default(),
            message_counter: 0,
            peer_identity: test_identity("neko-device-peer", "Peer"),
        };
        let header = nekolink_protocol::SessionTrafficFrameHeader::new(
            nekolink_protocol::SESSION_CIPHER_XCHACHA20POLY1305,
            nekolink_protocol::SessionFrameKind::Control,
            nekolink_protocol::SessionFrameDirection::Send,
            7,
        )
        .unwrap();
        let envelope = nekolink_protocol::EncryptedSessionPayload::seal_control(
            "session-1",
            "session-1:decision-1",
            &session.keys,
            header,
            nekolink_protocol::MessageKind::FileAccept,
            &TransferDecision::accept(),
        )
        .unwrap();
        let mut first_buffer = Vec::new();
        let mut second_buffer = Vec::new();
        nekodrop_network::write_session_control_envelope(&mut first_buffer, &envelope).unwrap();
        nekodrop_network::write_session_control_envelope(&mut second_buffer, &envelope).unwrap();

        assert_eq!(
            session
                .read_transfer_decision(&mut std::io::Cursor::new(first_buffer))
                .unwrap(),
            TransferDecision::accept()
        );
        let error = session
            .read_transfer_decision(&mut std::io::Cursor::new(second_buffer))
            .unwrap_err();

        assert!(error.to_string().contains("replayed session frame"));
    }

    #[test]
    fn active_session_control_rejects_replayed_encrypted_offer() {
        let mut session = ActiveSessionControl {
            session_id: "session-1".to_string(),
            cipher: nekolink_protocol::SESSION_CIPHER_XCHACHA20POLY1305.to_string(),
            keys: SessionKeyMaterial {
                send_key: [23_u8; nekolink_protocol::SESSION_TRAFFIC_KEY_LEN],
                receive_key: [23_u8; nekolink_protocol::SESSION_TRAFFIC_KEY_LEN],
            },
            counters: SessionTrafficCounters::default(),
            receive_window: SessionReplayWindow::default(),
            message_counter: 0,
            peer_identity: test_identity("neko-device-peer", "Peer"),
        };
        let offer = TransferOffer::new(
            "transfer-1",
            "drop",
            vec![TransferOfferFile {
                manifest_path: "drop/sample.txt".to_string(),
                size: 5,
                sha256: "abc123".to_string(),
            }],
        );
        let header = nekolink_protocol::SessionTrafficFrameHeader::new(
            nekolink_protocol::SESSION_CIPHER_XCHACHA20POLY1305,
            nekolink_protocol::SessionFrameKind::Control,
            nekolink_protocol::SessionFrameDirection::Send,
            6,
        )
        .unwrap();
        let envelope = nekolink_protocol::EncryptedSessionPayload::seal_control(
            "session-1",
            "session-1:offer-1",
            &session.keys,
            header,
            nekolink_protocol::MessageKind::FileOffer,
            &offer,
        )
        .unwrap();
        let mut first_buffer = Vec::new();
        let mut second_buffer = Vec::new();
        nekodrop_network::write_session_control_envelope(&mut first_buffer, &envelope).unwrap();
        nekodrop_network::write_session_control_envelope(&mut second_buffer, &envelope).unwrap();

        assert_eq!(
            session
                .read_transfer_offer(&mut std::io::Cursor::new(first_buffer))
                .unwrap(),
            offer
        );
        let error = session
            .read_transfer_offer(&mut std::io::Cursor::new(second_buffer))
            .unwrap_err();

        assert!(error.to_string().contains("replayed session frame"));
    }

    #[test]
    fn active_session_control_rejects_unexpected_encrypted_decision_kind() {
        let mut session = ActiveSessionControl {
            session_id: "session-1".to_string(),
            cipher: nekolink_protocol::SESSION_CIPHER_XCHACHA20POLY1305.to_string(),
            keys: SessionKeyMaterial {
                send_key: [23_u8; nekolink_protocol::SESSION_TRAFFIC_KEY_LEN],
                receive_key: [23_u8; nekolink_protocol::SESSION_TRAFFIC_KEY_LEN],
            },
            counters: SessionTrafficCounters::default(),
            receive_window: SessionReplayWindow::default(),
            message_counter: 0,
            peer_identity: test_identity("neko-device-peer", "Peer"),
        };
        let header = nekolink_protocol::SessionTrafficFrameHeader::new(
            nekolink_protocol::SESSION_CIPHER_XCHACHA20POLY1305,
            nekolink_protocol::SessionFrameKind::Control,
            nekolink_protocol::SessionFrameDirection::Send,
            8,
        )
        .unwrap();
        let envelope = nekolink_protocol::EncryptedSessionPayload::seal_control(
            "session-1",
            "session-1:decision-2",
            &session.keys,
            header,
            nekolink_protocol::MessageKind::FileOffer,
            &TransferDecision::accept(),
        )
        .unwrap();
        let mut buffer = Vec::new();
        nekodrop_network::write_session_control_envelope(&mut buffer, &envelope).unwrap();

        let error = session
            .read_transfer_decision(&mut std::io::Cursor::new(buffer))
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("unexpected encrypted transfer decision kind"));
    }

    #[test]
    fn encrypted_control_receive_entry_accepts_plaintext_offer_for_compatibility() {
        let dir = unique_temp_dir("service-encrypted-control-plaintext-compat");
        let source_root = dir.join("source").join("drop");
        let receive_dir = dir.join("receive");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&receive_dir).unwrap();
        fs::write(source_root.join("sample.txt"), b"plaintext compat").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
        let receiver_identity = test_identity("neko-device-receiver", "Receiver Windows");

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                accept_incoming_stream_with_encrypted_control_and_cancel(
                    &mut stream,
                    &receive_dir,
                    &receiver_identity,
                    |_| true,
                    |_| panic!("pairing should not be handled on transfer path"),
                    |_| {},
                    || false,
                )
            }
        });

        let send_report = send_paths(&endpoint, &[source_root]).unwrap();
        let receive_report = match receiver.join().unwrap().unwrap() {
            IncomingSessionReport::Transfer(report) => report,
            IncomingSessionReport::Pairing(_) => panic!("expected transfer report"),
        };

        assert_eq!(send_report.sent_files.len(), 1);
        assert_eq!(
            fs::read_to_string(receive_dir.join("drop/sample.txt")).unwrap(),
            "plaintext compat"
        );
        assert_eq!(receive_report.files.len(), 1);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn encrypted_control_receiver_rejects_offer_sender_that_differs_from_session_initiator() {
        let dir = unique_temp_dir("service-encrypted-control-sender-mismatch");
        let receive_dir = dir.join("receive");
        fs::create_dir_all(&receive_dir).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
        let sender = test_identity("neko-device-sender", "Sender Mac");
        let impersonated = test_identity("neko-device-other", "Other Mac");
        let receiver_identity = test_identity("neko-device-receiver", "Receiver Windows");

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                accept_incoming_stream_with_encrypted_control_and_cancel(
                    &mut stream,
                    &receive_dir,
                    &receiver_identity,
                    |_| true,
                    |_| panic!("pairing should not be handled on encrypted transfer path"),
                    |_| {},
                    || false,
                )
            }
        });

        let mut stream = connect_endpoint(&endpoint).unwrap();
        let mut session = start_initiator_session(&mut stream, &sender, "transfer-mismatch")
            .expect("session should be established");
        let offer = TransferOffer::new(
            "transfer-mismatch",
            "drop",
            vec![TransferOfferFile {
                manifest_path: "drop/sample.txt".to_string(),
                size: 4,
                sha256: "abc123".to_string(),
            }],
        )
        .with_sender_identity(&impersonated);
        let message_id = session.next_message_id("offer");
        let header = session.next_send_control_header().unwrap();
        write_session_transfer_offer(
            &mut stream,
            session.session_id.clone(),
            message_id,
            &session.keys,
            header,
            &offer,
        )
        .unwrap();
        drop(stream);
        let receive_result = receiver.join().unwrap();

        assert!(receive_result.is_err());
        assert!(receive_result
            .unwrap_err()
            .to_string()
            .contains("offer sender identity does not match encrypted session"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn plaintext_receive_entry_rejects_session_hello_first_frame() {
        let dir = unique_temp_dir("service-plaintext-rejects-session");
        let receive_dir = dir.join("receive");
        fs::create_dir_all(&receive_dir).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
        let sender = test_identity("neko-device-sender", "Sender Mac");

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            move || accept_transfer_with_decision(&listener, &receive_dir, |_| true, |_| {})
        });

        let mut stream = connect_endpoint(&endpoint).unwrap();
        let key_pair = SessionEphemeralKeyPair::from_secret([7_u8; 32]).unwrap();
        let hello =
            SessionHelloPayload::default_crypto("session-rejected", sender, key_pair.public_key);
        write_session_hello(&mut stream, &hello).unwrap();
        drop(stream);
        let receive_result = receiver.join().unwrap();

        assert!(receive_result.is_err());
        assert!(receive_result
            .unwrap_err()
            .to_string()
            .contains("session hello"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn service_resumes_transfer_from_existing_partial_file() {
        let dir = unique_temp_dir("service-resume");
        let source_root = dir.join("source").join("drop");
        let receive_dir = dir.join("receive");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(receive_dir.join("drop")).unwrap();
        fs::write(source_root.join("sample.txt"), b"hello world").unwrap();
        fs::write(receive_dir.join("drop/sample.txt.nekodrop-part"), b"hello ").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            move || accept_transfer(&listener, &receive_dir)
        });

        let send_report = send_paths(&endpoint, &[source_root]).unwrap();
        let receive_report = receiver.join().unwrap().unwrap();

        assert_eq!(send_report.sent_files.len(), 1);
        assert_eq!(send_report.sent_files[0].bytes_sent, 5);
        assert_eq!(receive_report.files.len(), 1);
        assert_eq!(
            fs::read_to_string(receive_dir.join("drop/sample.txt")).unwrap(),
            "hello world"
        );
        assert!(!receive_dir.join("drop/sample.txt.nekodrop-part").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn receiver_can_decline_transfer_offer_before_files_are_sent() {
        let dir = unique_temp_dir("service-decline");
        let source_root = dir.join("source").join("drop");
        let receive_dir = dir.join("receive");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&receive_dir).unwrap();
        fs::write(source_root.join("sample.txt"), b"declined").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            move || accept_transfer_with_decision(&listener, &receive_dir, |_| false, |_| {})
        });

        let send_result = send_paths(&endpoint, &[source_root]);
        let receive_result = receiver.join().unwrap();

        assert!(send_result.is_err());
        assert!(receive_result.is_err());
        assert!(!receive_dir.join("drop/sample.txt").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn receiver_declines_transfer_when_receive_space_is_insufficient() {
        let dir = unique_temp_dir("service-space-preflight");
        let receive_dir = dir.join("receive");
        fs::create_dir_all(&receive_dir).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            move || accept_transfer_with_decision(&listener, &receive_dir, |_| true, |_| {})
        });

        let mut stream = connect_endpoint(&endpoint).unwrap();
        let offer = TransferOffer::new(
            "transfer-huge",
            "huge",
            vec![TransferOfferFile {
                manifest_path: "huge/video.bin".to_string(),
                size: u64::MAX,
                sha256: "0".repeat(64),
            }],
        );
        write_transfer_offer(&mut stream, &offer).unwrap();
        let decision = read_transfer_decision(&mut stream).unwrap();
        drop(stream);
        let receive_result = receiver.join().unwrap();

        assert!(!decision.accepted);
        assert!(decision
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("insufficient receive space"));
        assert!(receive_result
            .unwrap_err()
            .to_string()
            .contains("insufficient receive space"));

        fs::remove_dir_all(dir).unwrap();
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nekodrop-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_identity(device_id: &str, device_name: &str) -> DeviceIdentity {
        DeviceIdentity::new(
            device_id,
            device_name,
            DeviceKind::Desktop,
            PlatformKind::Macos,
            format!("sha256:{:0>64}", device_id.len()),
            [
                Capability::FileTransfer,
                Capability::FileSend,
                Capability::FileReceive,
                Capability::FileSha256,
                Capability::EncryptedSession,
            ],
        )
    }

    fn test_identity_with_signing_key(
        device_id: &str,
        device_name: &str,
        signing_key: &DeviceIdentitySigningKey,
    ) -> DeviceIdentity {
        DeviceIdentity::new(
            device_id,
            device_name,
            DeviceKind::Desktop,
            PlatformKind::Macos,
            signing_key.public_key_fingerprint(),
            [
                Capability::FileTransfer,
                Capability::FileSend,
                Capability::FileReceive,
                Capability::FileSha256,
                Capability::EncryptedSession,
            ],
        )
    }

    fn create_valid_bundle_source(dir: &Path) -> PathBuf {
        let root = dir.join("source").join("bundle");
        fs::create_dir_all(root.join("files")).unwrap();
        fs::write(
            root.join("files").join("manifest.json"),
            b"{\"kind\":\"skill\"}",
        )
        .unwrap();
        fs::write(root.join("files").join("content.bin"), b"hello bundle").unwrap();
        fs::write(
            root.join("bundle.json"),
            r#"{
  "schema": "nekolink.bundle.v1",
  "bundle_id": "bundle_1234567890",
  "bundle_type": "skill",
  "display_name": "voice_transcribe",
  "source_app": "OpenNeko",
  "created_at": "2026-06-14T10:30:00Z",
  "sender": {
    "device_id": "neko-device-1234567890",
    "device_name": "MacBook",
    "fingerprint": "sha256:0123456789abcdef"
  },
  "compatibility": {
    "min_nekolink_version": 1,
    "required_capabilities": ["bundle_transfer"]
  },
  "summary": {
    "file_count": 2,
    "total_bytes": 28
  },
  "files": [
    {
      "path": "files/manifest.json",
      "size": 16,
      "sha256": "0bc3f835203da0c2bbb44658e66c6bc0449e7f00bd9bd8fecd5d12283baaf5c9",
      "role": "manifest"
    },
    {
      "path": "files/content.bin",
      "size": 12,
      "sha256": "04cfecf64270c52b81da10bf6890b24fa73ee79715c44d1bc443dd9dd1de04d0",
      "role": "payload"
    }
  ]
}"#,
        )
        .unwrap();
        fs::write(
            root.join("checksums.json"),
            r#"{
  "algorithm": "sha256",
  "files": {
    "files/manifest.json": "0bc3f835203da0c2bbb44658e66c6bc0449e7f00bd9bd8fecd5d12283baaf5c9",
    "files/content.bin": "04cfecf64270c52b81da10bf6890b24fa73ee79715c44d1bc443dd9dd1de04d0"
  }
}"#,
        )
        .unwrap();
        fs::write(
            root.join("permissions.json"),
            r#"{
  "requested_scopes": ["skill.install"],
  "writes": [
    {
      "target": "openneko.skills",
      "mode": "create_only"
    }
  ],
  "secrets": {
    "contains_secrets": false,
    "redacted_fields": []
  }
}"#,
        )
        .unwrap();
        root
    }
}
