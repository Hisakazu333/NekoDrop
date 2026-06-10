use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use nekodrop_core::{NekoDropError, NekoDropResult};
use nekodrop_network::{
    read_incoming_control_frame, read_pairing_decision, read_transfer_decision,
    read_transfer_offer, receive_file_frames, send_file_frames_with_progress_and_cancel,
    write_pairing_decision, write_pairing_request, write_transfer_decision_for_transfer,
    write_transfer_offer, ConnectionTicket, Endpoint, IncomingControlFrame, OutgoingFileFrame,
    PairingDecisionPayload, PairingRequestPayload, SentFileFrame, TransferDecision, TransferOffer,
    TransferOfferFile, TransferProgress, TransportKind,
};
use nekodrop_storage::{
    create_source_plan_from_paths, write_received_file_with_progress, ReceivedFile,
};

pub use nekodrop_storage::{TransferSourceFile, TransferSourcePlan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSendReport {
    pub plan: TransferSourcePlan,
    pub sent_files: Vec<SentFileFrame>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferReceiveReport {
    pub files: Vec<ReceivedFile>,
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
    mut should_cancel: C,
) -> NekoDropResult<TransferSendReport>
where
    F: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
{
    if endpoint.transport != TransportKind::Tcp {
        return Err(NekoDropError::Network(format!(
            "unsupported transport for file send: {:?}",
            endpoint.transport
        )));
    }

    let outgoing = outgoing_frames_from_plan(&plan);
    let offer = offer_from_plan(&plan);
    let mut stream =
        TcpStream::connect((endpoint.host.as_str(), endpoint.port)).map_err(|error| {
            NekoDropError::Network(format!(
                "failed to connect to {}:{}: {error}",
                endpoint.host, endpoint.port
            ))
        })?;
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

    let sent_files = send_file_frames_with_progress_and_cancel(
        &mut stream,
        &outgoing,
        plan.total_bytes(),
        |progress| on_progress(TransferProgressEvent::Sending(progress)),
        || should_cancel(),
    )?;

    Ok(TransferSendReport { plan, sent_files })
}

pub fn send_pairing_request(
    endpoint: &Endpoint,
    request: PairingRequestPayload,
) -> NekoDropResult<PairingDecisionPayload> {
    if endpoint.transport != TransportKind::Tcp {
        return Err(NekoDropError::Network(format!(
            "unsupported transport for pairing: {:?}",
            endpoint.transport
        )));
    }

    let mut stream =
        TcpStream::connect((endpoint.host.as_str(), endpoint.port)).map_err(|error| {
            NekoDropError::Network(format!(
                "failed to connect to {}:{}: {error}",
                endpoint.host, endpoint.port
            ))
        })?;
    write_pairing_request(&mut stream, &request)?;
    read_pairing_decision(&mut stream)
}

pub fn accept_transfer(
    listener: &TcpListener,
    receive_dir: &Path,
) -> NekoDropResult<TransferReceiveReport> {
    accept_transfer_with_decision(listener, receive_dir, |_| true, |_| {})
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
    let offer = read_transfer_offer(stream)?;
    accept_transfer_offer_stream_with_decision(stream, receive_dir, offer, decide, on_progress)
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
    match read_incoming_control_frame(stream)? {
        IncomingControlFrame::FileOffer(offer) => accept_transfer_offer_stream_with_decision(
            stream,
            receive_dir,
            offer,
            decide,
            on_progress,
        )
        .map(IncomingSessionReport::Transfer),
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
    if !decide(&offer) {
        write_transfer_decision_for_transfer(
            stream,
            &offer.transfer_id,
            &TransferDecision::decline("receiver declined this transfer"),
        )?;
        return Err(NekoDropError::Network(
            "transfer declined by receiver".into(),
        ));
    }
    write_transfer_decision_for_transfer(stream, &offer.transfer_id, &TransferDecision::accept())?;

    let mut bytes_transferred = 0_u64;
    let mut file_index = 0_usize;
    let mut on_progress = on_progress;
    let files = receive_file_frames(stream, |header, stream| {
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
        file_index += 1;
        on_progress(TransferProgressEvent::Receiving(TransferProgress {
            manifest_path: header.manifest_path.clone(),
            file_index,
            file_count: offer.file_count,
            file_bytes_transferred: 0,
            file_size: header.size,
            bytes_transferred,
            total_bytes: offer.total_bytes,
        }));
        let received = write_received_file_with_progress(
            receive_dir,
            &header.manifest_path,
            header.size,
            &header.sha256,
            stream,
            |file_bytes| {
                on_progress(TransferProgressEvent::Receiving(TransferProgress {
                    manifest_path: header.manifest_path.clone(),
                    file_index,
                    file_count: offer.file_count,
                    file_bytes_transferred: file_bytes,
                    file_size: header.size,
                    bytes_transferred: bytes_transferred.saturating_add(file_bytes),
                    total_bytes: offer.total_bytes,
                }));
            },
        )?;
        bytes_transferred = bytes_transferred.saturating_add(received.bytes_written);
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

    Ok(TransferReceiveReport { files })
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
    TransferOffer::new(
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
    )
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
        assert_eq!(receive_report.files.len(), 2);
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

    fn unique_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nekodrop-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
