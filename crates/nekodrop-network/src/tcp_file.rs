use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

pub use nekolink_protocol::{
    DeviceHello, PairingDecisionPayload, PairingRequestPayload, SessionHelloPayload,
    SessionReadyPayload, TransferDecision, TransferOffer, TransferOfferFile, TransferResumeFile,
};

use nekodrop_core::{NekoDropError, NekoDropResult};
use nekolink_protocol::{Capability, Envelope, ErrorCode, MessageKind, ProtocolError};
use serde::{Deserialize, Serialize};

const COPY_BUFFER_SIZE: usize = 64 * 1024;
const MAX_JSON_FRAME_SIZE: usize = 256 * 1024;
const MAX_FILE_FRAME_COUNT: u32 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFrameHeader {
    pub manifest_path: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferProgress {
    pub manifest_path: String,
    pub file_index: usize,
    pub file_count: usize,
    pub file_bytes_transferred: u64,
    pub file_size: u64,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentFileFrame {
    pub manifest_path: String,
    pub bytes_sent: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingFileFrame {
    pub manifest_path: String,
    pub file_path: std::path::PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncomingControlFrame {
    DeviceHello(DeviceHello),
    FileOffer(TransferOffer),
    PairingRequest(PairingRequestPayload),
}

impl OutgoingFileFrame {
    pub fn new(
        manifest_path: impl Into<String>,
        file_path: impl Into<std::path::PathBuf>,
        sha256: impl Into<String>,
    ) -> Self {
        Self {
            manifest_path: manifest_path.into(),
            file_path: file_path.into(),
            sha256: sha256.into(),
        }
    }
}

pub fn send_single_file_frame(
    stream: &mut impl Write,
    manifest_path: impl Into<String>,
    file_path: &Path,
    sha256: impl Into<String>,
) -> NekoDropResult<SentFileFrame> {
    send_single_file_frame_with_progress(stream, manifest_path, file_path, sha256, |_| {})
}

pub fn send_single_file_frame_with_progress<F>(
    stream: &mut impl Write,
    manifest_path: impl Into<String>,
    file_path: &Path,
    sha256: impl Into<String>,
    mut on_progress: F,
) -> NekoDropResult<SentFileFrame>
where
    F: FnMut(u64),
{
    send_single_file_frame_with_progress_and_cancel(
        stream,
        manifest_path,
        file_path,
        sha256,
        &mut on_progress,
        || false,
    )
}

pub fn send_single_file_frame_with_progress_and_cancel<W, F, C>(
    stream: &mut W,
    manifest_path: impl Into<String>,
    file_path: &Path,
    sha256: impl Into<String>,
    mut on_progress: F,
    mut should_cancel: C,
) -> NekoDropResult<SentFileFrame>
where
    W: Write,
    F: FnMut(u64),
    C: FnMut() -> bool,
{
    send_single_file_frame_from_offset_with_progress_and_cancel(
        stream,
        manifest_path,
        file_path,
        sha256,
        0,
        &mut on_progress,
        &mut should_cancel,
    )
}

pub fn send_single_file_frame_from_offset_with_progress_and_cancel<W, F, C>(
    stream: &mut W,
    manifest_path: impl Into<String>,
    file_path: &Path,
    sha256: impl Into<String>,
    offset: u64,
    mut on_progress: F,
    mut should_cancel: C,
) -> NekoDropResult<SentFileFrame>
where
    W: Write,
    F: FnMut(u64),
    C: FnMut() -> bool,
{
    let manifest_path = manifest_path.into();
    let metadata = file_path.metadata().map_err(|error| {
        NekoDropError::Network(format!(
            "failed to read metadata for {}: {error}",
            file_path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(NekoDropError::Network(format!(
            "path is not a file: {}",
            file_path.display()
        )));
    }
    if offset > metadata.len() {
        return Err(NekoDropError::Network(format!(
            "resume offset is larger than file size for {}: {} > {}",
            file_path.display(),
            offset,
            metadata.len()
        )));
    }

    let header = FileFrameHeader {
        manifest_path: manifest_path.clone(),
        size: metadata.len(),
        sha256: sha256.into(),
        offset,
    };
    write_header(stream, &header)?;

    let mut file = File::open(file_path).map_err(|error| {
        NekoDropError::Network(format!("failed to open {}: {error}", file_path.display()))
    })?;
    if offset > 0 {
        file.seek(SeekFrom::Start(offset)).map_err(|error| {
            NekoDropError::Network(format!(
                "failed to seek {} to resume offset {offset}: {error}",
                file_path.display()
            ))
        })?;
        on_progress(offset);
    }
    let mut buffer = [0_u8; COPY_BUFFER_SIZE];
    let mut file_bytes_transferred = offset;
    let mut bytes_sent = 0_u64;
    loop {
        if should_cancel() {
            return Err(NekoDropError::Network("transfer cancelled".into()));
        }

        let read = file.read(&mut buffer).map_err(|error| {
            NekoDropError::Network(format!(
                "failed to read {} while sending: {error}",
                file_path.display()
            ))
        })?;
        if read == 0 {
            break;
        }

        stream.write_all(&buffer[..read]).map_err(|error| {
            NekoDropError::Network(format!(
                "failed to send file {} over TCP: {error}",
                file_path.display()
            ))
        })?;
        bytes_sent += read as u64;
        file_bytes_transferred += read as u64;
        on_progress(file_bytes_transferred);
    }
    stream.flush().map_err(|error| {
        NekoDropError::Network(format!(
            "failed to flush TCP stream after file send: {error}"
        ))
    })?;

    Ok(SentFileFrame {
        manifest_path,
        bytes_sent,
    })
}

pub fn send_file_frames(
    stream: &mut impl Write,
    files: &[OutgoingFileFrame],
) -> NekoDropResult<Vec<SentFileFrame>> {
    send_file_frames_with_progress(stream, files, 0, |_| {})
}

pub fn send_file_frames_with_progress<F>(
    stream: &mut impl Write,
    files: &[OutgoingFileFrame],
    total_bytes: u64,
    mut on_progress: F,
) -> NekoDropResult<Vec<SentFileFrame>>
where
    F: FnMut(TransferProgress),
{
    send_file_frames_with_progress_and_cancel(stream, files, total_bytes, &mut on_progress, || {
        false
    })
}

pub fn send_file_frames_with_progress_and_cancel<W, F, C>(
    stream: &mut W,
    files: &[OutgoingFileFrame],
    total_bytes: u64,
    mut on_progress: F,
    mut should_cancel: C,
) -> NekoDropResult<Vec<SentFileFrame>>
where
    W: Write,
    F: FnMut(TransferProgress),
    C: FnMut() -> bool,
{
    send_file_frames_with_resume_and_cancel(
        stream,
        files,
        total_bytes,
        &[],
        &mut on_progress,
        &mut should_cancel,
    )
}

pub fn send_file_frames_with_resume_and_cancel<W, F, C>(
    stream: &mut W,
    files: &[OutgoingFileFrame],
    total_bytes: u64,
    resume_files: &[TransferResumeFile],
    mut on_progress: F,
    mut should_cancel: C,
) -> NekoDropResult<Vec<SentFileFrame>>
where
    W: Write,
    F: FnMut(TransferProgress),
    C: FnMut() -> bool,
{
    let count = u32::try_from(files.len())
        .map_err(|_| NekoDropError::Network("too many files in one transfer".into()))?;
    stream
        .write_all(&count.to_be_bytes())
        .map_err(|error| NekoDropError::Network(format!("failed to write file count: {error}")))?;

    let mut sent = Vec::with_capacity(files.len());
    let resolved_total_bytes = if total_bytes == 0 {
        files
            .iter()
            .filter_map(|file| {
                file.file_path
                    .metadata()
                    .ok()
                    .map(|metadata| metadata.len())
            })
            .sum()
    } else {
        total_bytes
    };
    let resume_offsets = resume_offsets_by_path(resume_files)?;
    let mut bytes_transferred = initial_resumed_bytes(files, &resume_offsets)?;

    for (index, file) in files.iter().enumerate() {
        if should_cancel() {
            return Err(NekoDropError::Network("transfer cancelled".into()));
        }

        let file_size = file
            .file_path
            .metadata()
            .map_err(|error| {
                NekoDropError::Network(format!(
                    "failed to read metadata for {}: {error}",
                    file.file_path.display()
                ))
            })?
            .len();
        let offset = resume_offsets
            .get(file.manifest_path.as_str())
            .copied()
            .unwrap_or(0);
        if offset > file_size {
            return Err(NekoDropError::Network(format!(
                "resume offset is larger than file size for {}: {} > {}",
                file.manifest_path, offset, file_size
            )));
        }
        let mut last_file_bytes = offset;
        let sent_frame = send_single_file_frame_from_offset_with_progress_and_cancel(
            stream,
            file.manifest_path.clone(),
            &file.file_path,
            file.sha256.clone(),
            offset,
            |file_bytes| {
                let delta = file_bytes.saturating_sub(last_file_bytes);
                last_file_bytes = file_bytes;
                bytes_transferred = bytes_transferred.saturating_add(delta);
                on_progress(TransferProgress {
                    manifest_path: file.manifest_path.clone(),
                    file_index: index + 1,
                    file_count: files.len(),
                    file_bytes_transferred: file_bytes,
                    file_size,
                    bytes_transferred,
                    total_bytes: resolved_total_bytes,
                });
            },
            || should_cancel(),
        )?;
        if offset == file_size {
            on_progress(TransferProgress {
                manifest_path: file.manifest_path.clone(),
                file_index: index + 1,
                file_count: files.len(),
                file_bytes_transferred: file_size,
                file_size,
                bytes_transferred,
                total_bytes: resolved_total_bytes,
            });
        }
        sent.push(sent_frame);
    }

    Ok(sent)
}

pub fn write_transfer_offer(stream: &mut impl Write, offer: &TransferOffer) -> NekoDropResult<()> {
    offer.validate().map_err(protocol_error_to_network)?;
    let envelope = Envelope::new(
        offer.transfer_id.clone(),
        format!("{}:offer", offer.transfer_id),
        MessageKind::FileOffer,
        offer.clone(),
    )
    .with_capabilities([Capability::FileTransfer, Capability::FileSha256]);
    write_json_frame(stream, &envelope)
}

pub fn write_device_hello(stream: &mut impl Write, hello: &DeviceHello) -> NekoDropResult<()> {
    hello.validate().map_err(protocol_error_to_network)?;
    let envelope = Envelope::new(
        hello.identity.device_id.clone(),
        format!("{}:hello", hello.identity.device_id),
        MessageKind::DeviceHello,
        hello.clone(),
    )
    .with_capabilities(hello.identity.capabilities.clone());
    write_json_frame(stream, &envelope)
}

pub fn read_device_hello(stream: &mut impl Read) -> NekoDropResult<DeviceHello> {
    let envelope: Envelope<DeviceHello> = read_json_frame(stream)?;
    envelope
        .validate_kind(MessageKind::DeviceHello)
        .map_err(protocol_error_to_network)?;
    let hello = envelope.payload;
    hello.validate().map_err(protocol_error_to_network)?;
    Ok(hello)
}

pub fn write_session_hello(
    stream: &mut impl Write,
    hello: &SessionHelloPayload,
) -> NekoDropResult<()> {
    hello.validate().map_err(protocol_error_to_network)?;
    let envelope = Envelope::new(
        hello.session_id.clone(),
        format!("{}:session-hello", hello.session_id),
        MessageKind::SessionHello,
        hello.clone(),
    )
    .with_capabilities([Capability::EncryptedSession]);
    write_json_frame(stream, &envelope)
}

pub fn read_session_hello(stream: &mut impl Read) -> NekoDropResult<SessionHelloPayload> {
    let envelope: Envelope<SessionHelloPayload> = read_json_frame(stream)?;
    envelope
        .validate_kind(MessageKind::SessionHello)
        .map_err(protocol_error_to_network)?;
    let hello = envelope.payload;
    hello.validate().map_err(protocol_error_to_network)?;
    Ok(hello)
}

pub fn write_session_ready(
    stream: &mut impl Write,
    ready: &SessionReadyPayload,
) -> NekoDropResult<()> {
    ready.validate().map_err(protocol_error_to_network)?;
    let envelope = Envelope::new(
        ready.session_id.clone(),
        format!("{}:session-ready", ready.session_id),
        MessageKind::SessionReady,
        ready.clone(),
    )
    .with_capabilities([Capability::EncryptedSession]);
    write_json_frame(stream, &envelope)
}

pub fn read_session_ready(stream: &mut impl Read) -> NekoDropResult<SessionReadyPayload> {
    let envelope: Envelope<SessionReadyPayload> = read_json_frame(stream)?;
    envelope
        .validate_kind(MessageKind::SessionReady)
        .map_err(protocol_error_to_network)?;
    let ready = envelope.payload;
    ready.validate().map_err(protocol_error_to_network)?;
    Ok(ready)
}

pub fn read_verified_session_ready(
    stream: &mut impl Read,
    hello: &SessionHelloPayload,
) -> NekoDropResult<SessionReadyPayload> {
    let ready = read_session_ready(stream)?;
    ready
        .verify_for_hello(hello)
        .map_err(protocol_error_to_network)?;
    Ok(ready)
}

pub fn read_transfer_offer(stream: &mut impl Read) -> NekoDropResult<TransferOffer> {
    let envelope: Envelope<TransferOffer> = read_json_frame(stream)?;
    envelope
        .validate_kind(MessageKind::FileOffer)
        .map_err(protocol_error_to_network)?;
    let offer = envelope.payload;
    offer.validate().map_err(protocol_error_to_network)?;
    Ok(offer)
}

pub fn write_pairing_request(
    stream: &mut impl Write,
    request: &PairingRequestPayload,
) -> NekoDropResult<()> {
    request.validate().map_err(protocol_error_to_network)?;
    let envelope = Envelope::new(
        request.request_id.clone(),
        format!("{}:pairing-request", request.request_id),
        MessageKind::PairingRequest,
        request.clone(),
    )
    .with_capabilities([Capability::DevicePairing]);
    write_json_frame(stream, &envelope)
}

pub fn read_incoming_control_frame(stream: &mut impl Read) -> NekoDropResult<IncomingControlFrame> {
    let envelope: Envelope<serde_json::Value> = read_json_frame(stream)?;
    envelope.validate().map_err(protocol_error_to_network)?;
    match envelope.kind {
        MessageKind::DeviceHello => {
            let hello =
                serde_json::from_value::<DeviceHello>(envelope.payload).map_err(|error| {
                    NekoDropError::Network(format!("failed to decode device hello: {error}"))
                })?;
            hello.validate().map_err(protocol_error_to_network)?;
            Ok(IncomingControlFrame::DeviceHello(hello))
        }
        MessageKind::FileOffer => {
            let offer =
                serde_json::from_value::<TransferOffer>(envelope.payload).map_err(|error| {
                    NekoDropError::Network(format!("failed to decode transfer offer: {error}"))
                })?;
            offer.validate().map_err(protocol_error_to_network)?;
            Ok(IncomingControlFrame::FileOffer(offer))
        }
        MessageKind::PairingRequest => {
            let request = serde_json::from_value::<PairingRequestPayload>(envelope.payload)
                .map_err(|error| {
                    NekoDropError::Network(format!("failed to decode pairing request: {error}"))
                })?;
            request.validate().map_err(protocol_error_to_network)?;
            Ok(IncomingControlFrame::PairingRequest(request))
        }
        _ => Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::UnexpectedMessageKind,
            format!("unexpected first frame kind: {}", envelope.kind.as_str()),
        ))),
    }
}

pub fn write_pairing_decision(
    stream: &mut impl Write,
    request_id: &str,
    decision: &PairingDecisionPayload,
) -> NekoDropResult<()> {
    let kind = if decision.accepted {
        MessageKind::PairingAccept
    } else {
        MessageKind::PairingReject
    };
    let envelope = Envelope::new(
        request_id,
        format!("{request_id}:pairing-decision"),
        kind,
        decision.clone(),
    )
    .with_capabilities([Capability::DevicePairing]);
    write_json_frame(stream, &envelope)
}

pub fn read_pairing_decision(stream: &mut impl Read) -> NekoDropResult<PairingDecisionPayload> {
    let envelope: Envelope<PairingDecisionPayload> = read_json_frame(stream)?;
    envelope.validate().map_err(protocol_error_to_network)?;
    if !matches!(
        envelope.kind,
        MessageKind::PairingAccept | MessageKind::PairingReject
    ) {
        return Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::UnexpectedMessageKind,
            format!(
                "unexpected pairing decision kind: {}",
                envelope.kind.as_str()
            ),
        )));
    }
    let decision = envelope.payload;
    if decision.accepted && envelope.kind != MessageKind::PairingAccept {
        return Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "accepted pairing decision must use pairing.accept",
        )));
    }
    if !decision.accepted && envelope.kind != MessageKind::PairingReject {
        return Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "rejected pairing decision must use pairing.reject",
        )));
    }
    Ok(decision)
}

pub fn write_transfer_decision(
    stream: &mut impl Write,
    decision: &TransferDecision,
) -> NekoDropResult<()> {
    write_transfer_decision_for_transfer(stream, "transfer-decision", decision)
}

pub fn write_transfer_decision_for_transfer(
    stream: &mut impl Write,
    transfer_id: &str,
    decision: &TransferDecision,
) -> NekoDropResult<()> {
    decision.validate().map_err(protocol_error_to_network)?;
    let kind = if decision.accepted {
        MessageKind::FileAccept
    } else {
        MessageKind::FileDecline
    };
    let envelope = Envelope::new(
        transfer_id,
        format!("{transfer_id}:decision"),
        kind,
        decision.clone(),
    )
    .with_capabilities([Capability::FileTransfer]);
    write_json_frame(stream, &envelope)
}

pub fn read_transfer_decision(stream: &mut impl Read) -> NekoDropResult<TransferDecision> {
    let envelope: Envelope<TransferDecision> = read_json_frame(stream)?;
    envelope.validate().map_err(protocol_error_to_network)?;
    if !matches!(
        envelope.kind,
        MessageKind::FileAccept | MessageKind::FileDecline
    ) {
        return Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::UnexpectedMessageKind,
            format!("unexpected decision kind: {}", envelope.kind.as_str()),
        )));
    }
    let decision = envelope.payload;
    decision.validate().map_err(protocol_error_to_network)?;
    if decision.accepted && envelope.kind != MessageKind::FileAccept {
        return Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "accepted decision must use file.accept",
        )));
    }
    if !decision.accepted && envelope.kind != MessageKind::FileDecline {
        return Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "declined decision must use file.decline",
        )));
    }
    Ok(decision)
}

pub fn receive_single_file_frame<R, F, T>(stream: &mut R, receive_file: F) -> NekoDropResult<T>
where
    R: Read,
    F: FnOnce(&FileFrameHeader, &mut R) -> NekoDropResult<T>,
{
    let header = read_header(stream)?;
    receive_file(&header, stream)
}

pub fn accept_file_frames<F, T>(
    listener: &TcpListener,
    mut receive_file: F,
) -> NekoDropResult<Vec<T>>
where
    F: FnMut(&FileFrameHeader, &mut TcpStream) -> NekoDropResult<T>,
{
    let (mut stream, _) = listener.accept().map_err(|error| {
        NekoDropError::Network(format!("failed to accept TCP connection: {error}"))
    })?;
    receive_file_frames(&mut stream, &mut receive_file)
}

pub fn receive_file_frames<R, F, T>(stream: &mut R, mut receive_file: F) -> NekoDropResult<Vec<T>>
where
    R: Read,
    F: FnMut(&FileFrameHeader, &mut R) -> NekoDropResult<T>,
{
    receive_file_frames_checked(stream, None, &mut receive_file)
}

pub fn receive_file_frames_with_expected_count<R, F, T>(
    stream: &mut R,
    expected_count: usize,
    mut receive_file: F,
) -> NekoDropResult<Vec<T>>
where
    R: Read,
    F: FnMut(&FileFrameHeader, &mut R) -> NekoDropResult<T>,
{
    let expected_count = u32::try_from(expected_count).map_err(|_| {
        NekoDropError::Network(format!(
            "expected file frame count exceeds maximum: {expected_count}"
        ))
    })?;
    receive_file_frames_checked(stream, Some(expected_count), &mut receive_file)
}

fn receive_file_frames_checked<R, F, T>(
    stream: &mut R,
    expected_count: Option<u32>,
    receive_file: &mut F,
) -> NekoDropResult<Vec<T>>
where
    R: Read,
    F: FnMut(&FileFrameHeader, &mut R) -> NekoDropResult<T>,
{
    let count = read_file_count(stream)?;
    if let Some(expected_count) = expected_count {
        if count != expected_count {
            return Err(NekoDropError::Network(format!(
                "file frame count mismatch: {count} != {expected_count}"
            )));
        }
    }
    let mut received = Vec::with_capacity(count as usize);

    for _ in 0..count {
        let header = read_header(stream)?;
        received.push(receive_file(&header, stream)?);
    }

    Ok(received)
}

pub fn accept_one_file_frame<F, T>(listener: &TcpListener, receive_file: F) -> NekoDropResult<T>
where
    F: FnOnce(&FileFrameHeader, &mut TcpStream) -> NekoDropResult<T>,
{
    let (mut stream, _) = listener.accept().map_err(|error| {
        NekoDropError::Network(format!("failed to accept TCP connection: {error}"))
    })?;
    let header = read_header(&mut stream)?;
    receive_file(&header, &mut stream)
}

fn write_header(stream: &mut impl Write, header: &FileFrameHeader) -> NekoDropResult<()> {
    let payload = serde_json::to_vec(header).map_err(|error| {
        NekoDropError::Network(format!("failed to encode file header: {error}"))
    })?;
    let len = u32::try_from(payload.len()).map_err(|_| {
        NekoDropError::Network("file header is too large for TCP frame".to_string())
    })?;
    stream.write_all(&len.to_be_bytes()).map_err(|error| {
        NekoDropError::Network(format!("failed to write file header length: {error}"))
    })?;
    stream.write_all(&payload).map_err(|error| {
        NekoDropError::Network(format!("failed to write file header payload: {error}"))
    })?;
    Ok(())
}

fn read_header(stream: &mut impl Read) -> NekoDropResult<FileFrameHeader> {
    let mut len_bytes = [0_u8; 4];
    stream.read_exact(&mut len_bytes).map_err(|error| {
        NekoDropError::Network(format!("failed to read file header length: {error}"))
    })?;
    let len = u32::from_be_bytes(len_bytes) as usize;
    if len == 0 || len > 64 * 1024 {
        return Err(NekoDropError::Network(format!(
            "invalid file header length: {len}"
        )));
    }

    let mut payload = vec![0_u8; len];
    stream.read_exact(&mut payload).map_err(|error| {
        NekoDropError::Network(format!("failed to read file header payload: {error}"))
    })?;

    serde_json::from_slice(&payload)
        .map_err(|error| NekoDropError::Network(format!("failed to decode file header: {error}")))
}

fn read_file_count(stream: &mut impl Read) -> NekoDropResult<u32> {
    let mut count_bytes = [0_u8; 4];
    stream
        .read_exact(&mut count_bytes)
        .map_err(|error| NekoDropError::Network(format!("failed to read file count: {error}")))?;
    let count = u32::from_be_bytes(count_bytes);
    if count > MAX_FILE_FRAME_COUNT {
        return Err(NekoDropError::Network(format!(
            "file frame count exceeds maximum: {count} > {MAX_FILE_FRAME_COUNT}"
        )));
    }
    Ok(count)
}

fn write_json_frame<T: Serialize>(stream: &mut impl Write, value: &T) -> NekoDropResult<()> {
    let payload = serde_json::to_vec(value)
        .map_err(|error| NekoDropError::Network(format!("failed to encode JSON frame: {error}")))?;
    let len = u32::try_from(payload.len())
        .map_err(|_| NekoDropError::Network("JSON frame is too large".into()))?;
    if payload.len() > MAX_JSON_FRAME_SIZE {
        return Err(NekoDropError::Network(format!(
            "JSON frame exceeds maximum size: {}",
            payload.len()
        )));
    }
    stream.write_all(&len.to_be_bytes()).map_err(|error| {
        NekoDropError::Network(format!("failed to write JSON frame length: {error}"))
    })?;
    stream.write_all(&payload).map_err(|error| {
        NekoDropError::Network(format!("failed to write JSON frame payload: {error}"))
    })?;
    Ok(())
}

fn read_json_frame<T: for<'de> Deserialize<'de>>(stream: &mut impl Read) -> NekoDropResult<T> {
    let mut len_bytes = [0_u8; 4];
    stream.read_exact(&mut len_bytes).map_err(|error| {
        NekoDropError::Network(format!("failed to read JSON frame length: {error}"))
    })?;
    let len = u32::from_be_bytes(len_bytes) as usize;
    if len == 0 || len > MAX_JSON_FRAME_SIZE {
        return Err(NekoDropError::Network(format!(
            "invalid JSON frame length: {len}"
        )));
    }

    let mut payload = vec![0_u8; len];
    stream.read_exact(&mut payload).map_err(|error| {
        NekoDropError::Network(format!("failed to read JSON frame payload: {error}"))
    })?;

    serde_json::from_slice(&payload)
        .map_err(|error| NekoDropError::Network(format!("failed to decode JSON frame: {error}")))
}

fn resume_offsets_by_path<'a>(
    resume_files: &'a [TransferResumeFile],
) -> NekoDropResult<HashMap<&'a str, u64>> {
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

fn initial_resumed_bytes(
    files: &[OutgoingFileFrame],
    resume_offsets: &HashMap<&str, u64>,
) -> NekoDropResult<u64> {
    let mut total = 0_u64;
    for file in files {
        let Some(offset) = resume_offsets.get(file.manifest_path.as_str()).copied() else {
            continue;
        };
        let size = file
            .file_path
            .metadata()
            .map_err(|error| {
                NekoDropError::Network(format!(
                    "failed to read metadata for {}: {error}",
                    file.file_path.display()
                ))
            })?
            .len();
        if offset > size {
            return Err(NekoDropError::Network(format!(
                "resume offset is larger than file size for {}: {} > {}",
                file.manifest_path, offset, size
            )));
        }
        total = total.saturating_add(offset);
    }
    Ok(total)
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

fn protocol_error_to_network(error: ProtocolError) -> NekoDropError {
    NekoDropError::Network(format!("{:?}: {}", error.code, error.message))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Cursor, Read};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::thread;

    use nekodrop_storage::{create_source_plan_from_paths, sha256_file, write_received_file};
    use nekolink_protocol::{
        Capability, DeviceIdentity, DeviceKind, PlatformKind, SessionHelloPayload,
        SessionReadyPayload,
    };

    use super::*;

    #[test]
    fn device_hello_round_trips_without_tcp_specific_stream() {
        let hello = DeviceHello::new(
            DeviceIdentity::new(
                "neko-device-abc123",
                "Hisakazu Mac",
                DeviceKind::Desktop,
                PlatformKind::Macos,
                "sha256:abc123",
                [
                    Capability::FileTransfer,
                    Capability::FileReceive,
                    Capability::DevicePairing,
                ],
            ),
            "NekoDrop",
            "0.1.0",
        );
        let mut buffer = Vec::new();

        write_device_hello(&mut buffer, &hello).unwrap();
        let received = read_device_hello(&mut Cursor::new(buffer)).unwrap();

        assert_eq!(received, hello);
    }

    #[test]
    fn session_handshake_round_trips_through_nekolink_envelopes() {
        let local_identity = DeviceIdentity::new(
            "neko-device-local",
            "Local Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:local",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let peer_identity = DeviceIdentity::new(
            "neko-device-peer",
            "Peer Windows",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:peer",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::new(
            "session-1",
            local_identity,
            "x25519",
            "base64-local-ephemeral-public-key",
            vec!["xchacha20poly1305".to_string()],
        );
        let ready = SessionReadyPayload::for_hello(
            &hello,
            peer_identity,
            "base64-peer-ephemeral-public-key",
            "xchacha20poly1305",
        )
        .unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let ready_for_receiver = ready.clone();
        let receiver = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let received = read_session_hello(&mut stream).unwrap();
            write_session_ready(&mut stream, &ready_for_receiver).unwrap();
            received
        });

        let mut stream = TcpStream::connect(address).unwrap();
        write_session_hello(&mut stream, &hello).unwrap();
        let received_ready = read_verified_session_ready(&mut stream, &hello).unwrap();
        let received_hello = receiver.join().unwrap();

        assert_eq!(received_hello, hello);
        assert_eq!(received_ready, ready);
    }

    #[test]
    fn verified_session_ready_rejects_mismatched_transcript_hash() {
        let local_identity = DeviceIdentity::new(
            "neko-device-local",
            "Local Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:local",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let peer_identity = DeviceIdentity::new(
            "neko-device-peer",
            "Peer Windows",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:peer",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::new(
            "session-1",
            local_identity,
            "x25519",
            "base64-local-ephemeral-public-key",
            vec!["xchacha20poly1305".to_string()],
        );
        let mut ready = SessionReadyPayload::for_hello(
            &hello,
            peer_identity,
            "base64-peer-ephemeral-public-key",
            "xchacha20poly1305",
        )
        .unwrap();
        ready.handshake_hash = "sha256:tampered".to_string();

        let mut buffer = Vec::new();
        write_session_ready(&mut buffer, &ready).unwrap();

        let error = read_verified_session_ready(&mut Cursor::new(buffer), &hello).unwrap_err();

        assert!(error.to_string().contains("handshake_hash mismatch"));
    }

    #[test]
    fn sends_and_receives_real_file_over_loopback_tcp() {
        let dir = unique_temp_dir("tcp-loopback");
        let source_dir = dir.join("source");
        let receive_dir = dir.join("receive");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&receive_dir).unwrap();
        let source_file = source_dir.join("sample.txt");
        fs::write(&source_file, b"real tcp transfer").unwrap();
        let checksum = sha256_file(&source_file).unwrap().value;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            move || {
                accept_one_file_frame(&listener, |header, stream| {
                    write_received_file(
                        &receive_dir,
                        &header.manifest_path,
                        header.size,
                        &header.sha256,
                        stream,
                    )
                })
            }
        });

        let mut stream = TcpStream::connect(address).unwrap();
        let sent =
            send_single_file_frame(&mut stream, "incoming/sample.txt", &source_file, checksum)
                .unwrap();

        let received = receiver.join().unwrap().unwrap();

        assert_eq!(sent.bytes_sent, 17);
        assert!(received.verified);
        assert_eq!(
            fs::read_to_string(receive_dir.join("incoming/sample.txt")).unwrap(),
            "real tcp transfer"
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sends_and_receives_manifest_files_over_one_loopback_tcp_connection() {
        let dir = unique_temp_dir("tcp-manifest-loopback");
        let source_root = dir.join("source").join("drop");
        let receive_dir = dir.join("receive");
        fs::create_dir_all(source_root.join("nested")).unwrap();
        fs::create_dir_all(&receive_dir).unwrap();
        fs::write(source_root.join("nested").join("one.txt"), b"one").unwrap();
        fs::write(source_root.join("two.txt"), b"two").unwrap();

        let plan = create_source_plan_from_paths(&[source_root]).unwrap();
        let outgoing = plan
            .files
            .iter()
            .map(|file| {
                OutgoingFileFrame::new(
                    file.manifest_path.clone(),
                    file.source_path.clone(),
                    file.sha256.clone(),
                )
            })
            .collect::<Vec<_>>();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let receiver = thread::spawn({
            let receive_dir = receive_dir.clone();
            move || {
                accept_file_frames(&listener, |header, stream| {
                    write_received_file(
                        &receive_dir,
                        &header.manifest_path,
                        header.size,
                        &header.sha256,
                        stream,
                    )
                })
            }
        });

        let mut stream = TcpStream::connect(address).unwrap();
        let sent = send_file_frames(&mut stream, &outgoing).unwrap();
        let received = receiver.join().unwrap().unwrap();

        assert_eq!(sent.len(), 2);
        assert_eq!(received.len(), 2);
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
    fn sends_resumed_file_frames_from_offset() {
        let dir = unique_temp_dir("tcp-resume-offset");
        let file = dir.join("sample.txt");
        fs::write(&file, b"hello world").unwrap();
        let outgoing = vec![OutgoingFileFrame::new(
            "sample.txt",
            file,
            "sha256-placeholder",
        )];
        let resume_files = vec![TransferResumeFile::new("sample.txt", 6).unwrap()];
        let mut stream = Cursor::new(Vec::new());
        let mut progress = Vec::new();

        let sent = send_file_frames_with_resume_and_cancel(
            &mut stream,
            &outgoing,
            11,
            &resume_files,
            |event| progress.push(event),
            || false,
        )
        .unwrap();

        assert_eq!(sent[0].bytes_sent, 5);
        stream.set_position(0);
        let received = receive_file_frames(&mut stream, |header, stream| {
            assert_eq!(header.manifest_path, "sample.txt");
            assert_eq!(header.size, 11);
            assert_eq!(header.offset, 6);
            let mut payload = vec![0_u8; (header.size - header.offset) as usize];
            stream.read_exact(&mut payload).unwrap();
            Ok(payload)
        })
        .unwrap();

        assert_eq!(received, vec![b"world".to_vec()]);
        assert_eq!(
            progress.last().map(|event| event.bytes_transferred),
            Some(11)
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn receive_file_frames_rejects_declared_count_over_limit() {
        let mut stream = Cursor::new((MAX_FILE_FRAME_COUNT + 1).to_be_bytes().to_vec());

        let error = receive_file_frames(&mut stream, |_, _| -> NekoDropResult<()> {
            panic!("count validation should happen before reading file headers");
        })
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("file frame count exceeds maximum"));
    }

    #[test]
    fn receive_file_frames_rejects_count_that_differs_from_expected_count() {
        let mut stream = Cursor::new(2_u32.to_be_bytes().to_vec());

        let error =
            receive_file_frames_with_expected_count(&mut stream, 1, |_, _| Ok(())).unwrap_err();

        assert!(error.to_string().contains("file frame count mismatch"));
    }

    #[test]
    fn transfer_offer_round_trips_through_nekolink_envelope() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let offer = TransferOffer::new(
            "transfer-1",
            "drop",
            vec![TransferOfferFile {
                manifest_path: "drop/sample.txt".to_string(),
                size: 11,
                sha256: "abc123".to_string(),
            }],
        );

        let receiver = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_transfer_offer(&mut stream).unwrap()
        });

        let mut stream = TcpStream::connect(address).unwrap();
        write_transfer_offer(&mut stream, &offer).unwrap();
        let received = receiver.join().unwrap();

        assert_eq!(received, offer);
    }

    #[test]
    fn transfer_decision_round_trips_through_nekolink_envelope() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let receiver = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_transfer_decision(&mut stream).unwrap()
        });

        let mut stream = TcpStream::connect(address).unwrap();
        write_transfer_decision(&mut stream, &TransferDecision::decline("no")).unwrap();
        let received = receiver.join().unwrap();

        assert_eq!(received, TransferDecision::decline("no"));
    }

    #[test]
    fn pairing_request_round_trips_through_nekolink_envelope() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let request = PairingRequestPayload {
            request_id: "pairing-1".to_string(),
            device_id: "neko-device-local".to_string(),
            device_name: "Local Mac".to_string(),
            platform: "macos".to_string(),
            public_key_fingerprint: "sha256:local".to_string(),
            pairing_code: "ABC-123".to_string(),
            listen_port: 45821,
        };

        let receiver = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let received = read_incoming_control_frame(&mut stream).unwrap();
            write_pairing_decision(&mut stream, "pairing-1", &PairingDecisionPayload::accept())
                .unwrap();
            received
        });

        let mut stream = TcpStream::connect(address).unwrap();
        write_pairing_request(&mut stream, &request).unwrap();
        let decision = read_pairing_decision(&mut stream).unwrap();
        let received = receiver.join().unwrap();

        assert_eq!(decision, PairingDecisionPayload::accept());
        assert_eq!(received, IncomingControlFrame::PairingRequest(request));
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
