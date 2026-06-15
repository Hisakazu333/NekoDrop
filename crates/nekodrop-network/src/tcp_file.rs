use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

pub use nekolink_protocol::{
    DeviceHello, EncryptedFileFrame, EncryptedFileFrameHeader, EncryptedSessionPayload,
    PairingDecisionPayload, PairingRequestPayload, SessionHelloPayload, SessionReadyPayload,
    TransferDecision, TransferOffer, TransferOfferFile, TransferResumeFile,
    VerifiedSessionHandshake,
};

use nekodrop_core::{NekoDropError, NekoDropResult};
use nekolink_protocol::{
    Capability, Envelope, ErrorCode, MessageKind, ProtocolError, SessionFrameKind,
    SessionKeyMaterial, SessionTrafficCounters,
};
use serde::{Deserialize, Serialize};

const COPY_BUFFER_SIZE: usize = 64 * 1024;
const MAX_JSON_FRAME_SIZE: usize = 8 * 1024 * 1024;
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
    SessionHello(SessionHelloPayload),
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

pub fn send_encrypted_file_frames_with_resume_and_cancel<W, F, C>(
    stream: &mut W,
    transfer_id: &str,
    files: &[OutgoingFileFrame],
    total_bytes: u64,
    resume_files: &[TransferResumeFile],
    keys: &SessionKeyMaterial,
    counters: &mut SessionTrafficCounters,
    cipher: &str,
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
    let mut sent = Vec::with_capacity(files.len());

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
        let sent_frame = send_single_encrypted_file_frame_from_offset_with_progress_and_cancel(
            stream,
            transfer_id,
            file.manifest_path.clone(),
            &file.file_path,
            file.sha256.clone(),
            offset,
            keys,
            counters,
            cipher,
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

pub fn send_single_encrypted_file_frame_from_offset_with_progress_and_cancel<W, F, C>(
    stream: &mut W,
    transfer_id: &str,
    manifest_path: impl Into<String>,
    file_path: &Path,
    sha256: impl Into<String>,
    offset: u64,
    keys: &SessionKeyMaterial,
    counters: &mut SessionTrafficCounters,
    cipher: &str,
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

        let traffic_header = counters
            .next_send_header(cipher, SessionFrameKind::File)
            .map_err(protocol_error_to_network)?;
        let frame_header = EncryptedFileFrameHeader::new(
            transfer_id,
            manifest_path.clone(),
            file_bytes_transferred,
            read as u64,
            traffic_header,
        )
        .map_err(protocol_error_to_network)?;
        let frame = EncryptedFileFrame::seal(keys, frame_header, &buffer[..read])
            .map_err(protocol_error_to_network)?;
        write_encrypted_file_frame(stream, &frame)?;

        bytes_sent += read as u64;
        file_bytes_transferred += read as u64;
        on_progress(file_bytes_transferred);
    }
    stream.flush().map_err(|error| {
        NekoDropError::Network(format!(
            "failed to flush TCP stream after encrypted file send: {error}"
        ))
    })?;

    Ok(SentFileFrame {
        manifest_path,
        bytes_sent,
    })
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

pub fn write_session_control_envelope(
    stream: &mut impl Write,
    envelope: &Envelope<EncryptedSessionPayload>,
) -> NekoDropResult<()> {
    envelope
        .validate_kind(MessageKind::SessionControl)
        .map_err(protocol_error_to_network)?;
    write_json_frame(stream, envelope)
}

pub fn write_session_control_payload<T: Serialize>(
    stream: &mut impl Write,
    session_id: impl Into<String>,
    message_id: impl Into<String>,
    keys: &nekolink_protocol::SessionKeyMaterial,
    header: nekolink_protocol::SessionTrafficFrameHeader,
    inner_kind: MessageKind,
    payload: &T,
) -> NekoDropResult<()> {
    let envelope = EncryptedSessionPayload::seal_control(
        session_id, message_id, keys, header, inner_kind, payload,
    )
    .map_err(protocol_error_to_network)?;
    write_session_control_envelope(stream, &envelope)
}

pub fn read_session_control_envelope(
    stream: &mut impl Read,
) -> NekoDropResult<Envelope<EncryptedSessionPayload>> {
    let envelope: Envelope<EncryptedSessionPayload> = read_json_frame(stream)?;
    envelope
        .validate_kind(MessageKind::SessionControl)
        .map_err(protocol_error_to_network)?;
    Ok(envelope)
}

pub fn read_session_control_payload<T: for<'de> Deserialize<'de>>(
    stream: &mut impl Read,
    keys: &nekolink_protocol::SessionKeyMaterial,
) -> NekoDropResult<T> {
    let envelope = read_session_control_envelope(stream)?;
    EncryptedSessionPayload::open_control(&envelope, keys).map_err(protocol_error_to_network)
}

pub fn read_session_control_payload_once<T: for<'de> Deserialize<'de>>(
    stream: &mut impl Read,
    keys: &nekolink_protocol::SessionKeyMaterial,
    replay_window: &mut nekolink_protocol::SessionReplayWindow,
) -> NekoDropResult<T> {
    let envelope = read_session_control_envelope(stream)?;
    EncryptedSessionPayload::open_control_once(&envelope, keys, replay_window)
        .map_err(protocol_error_to_network)
}

pub fn read_session_control_payload_kind_once<T: for<'de> Deserialize<'de>>(
    stream: &mut impl Read,
    keys: &nekolink_protocol::SessionKeyMaterial,
    replay_window: &mut nekolink_protocol::SessionReplayWindow,
    expected_inner_kind: MessageKind,
) -> NekoDropResult<T> {
    let envelope = read_session_control_envelope(stream)?;
    if envelope.payload.inner_kind != expected_inner_kind {
        return Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::UnexpectedMessageKind,
            format!(
                "unexpected encrypted control kind: expected {}, got {}",
                expected_inner_kind.as_str(),
                envelope.payload.inner_kind.as_str()
            ),
        )));
    }
    EncryptedSessionPayload::open_control_once(&envelope, keys, replay_window)
        .map_err(protocol_error_to_network)
}

pub fn read_session_control_payload_kind<T: for<'de> Deserialize<'de>>(
    stream: &mut impl Read,
    keys: &nekolink_protocol::SessionKeyMaterial,
    expected_inner_kind: MessageKind,
) -> NekoDropResult<T> {
    let envelope = read_session_control_envelope(stream)?;
    if envelope.payload.inner_kind != expected_inner_kind {
        return Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::UnexpectedMessageKind,
            format!(
                "unexpected encrypted control kind: expected {}, got {}",
                expected_inner_kind.as_str(),
                envelope.payload.inner_kind.as_str()
            ),
        )));
    }
    EncryptedSessionPayload::open_control(&envelope, keys).map_err(protocol_error_to_network)
}

pub fn write_session_transfer_offer(
    stream: &mut impl Write,
    session_id: impl Into<String>,
    message_id: impl Into<String>,
    keys: &nekolink_protocol::SessionKeyMaterial,
    header: nekolink_protocol::SessionTrafficFrameHeader,
    offer: &TransferOffer,
) -> NekoDropResult<()> {
    offer.validate().map_err(protocol_error_to_network)?;
    write_session_control_payload(
        stream,
        session_id,
        message_id,
        keys,
        header,
        MessageKind::FileOffer,
        offer,
    )
}

pub fn read_session_transfer_offer(
    stream: &mut impl Read,
    keys: &nekolink_protocol::SessionKeyMaterial,
) -> NekoDropResult<TransferOffer> {
    let offer: TransferOffer =
        read_session_control_payload_kind(stream, keys, MessageKind::FileOffer)?;
    offer.validate().map_err(protocol_error_to_network)?;
    Ok(offer)
}

pub fn read_session_transfer_offer_once(
    stream: &mut impl Read,
    keys: &nekolink_protocol::SessionKeyMaterial,
    replay_window: &mut nekolink_protocol::SessionReplayWindow,
) -> NekoDropResult<TransferOffer> {
    let offer: TransferOffer = read_session_control_payload_kind_once(
        stream,
        keys,
        replay_window,
        MessageKind::FileOffer,
    )?;
    offer.validate().map_err(protocol_error_to_network)?;
    Ok(offer)
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
        MessageKind::SessionHello => {
            let hello = serde_json::from_value::<SessionHelloPayload>(envelope.payload).map_err(
                |error| NekoDropError::Network(format!("failed to decode session hello: {error}")),
            )?;
            hello.validate().map_err(protocol_error_to_network)?;
            Ok(IncomingControlFrame::SessionHello(hello))
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

pub fn write_session_transfer_decision(
    stream: &mut impl Write,
    session_id: impl Into<String>,
    message_id: impl Into<String>,
    keys: &nekolink_protocol::SessionKeyMaterial,
    header: nekolink_protocol::SessionTrafficFrameHeader,
    decision: &TransferDecision,
) -> NekoDropResult<()> {
    decision.validate().map_err(protocol_error_to_network)?;
    let kind = if decision.accepted {
        MessageKind::FileAccept
    } else {
        MessageKind::FileDecline
    };
    write_session_control_payload(stream, session_id, message_id, keys, header, kind, decision)
}

pub fn read_session_transfer_decision(
    stream: &mut impl Read,
    keys: &nekolink_protocol::SessionKeyMaterial,
) -> NekoDropResult<TransferDecision> {
    let envelope = read_session_control_envelope(stream)?;
    if !matches!(
        envelope.payload.inner_kind,
        MessageKind::FileAccept | MessageKind::FileDecline
    ) {
        return Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::UnexpectedMessageKind,
            format!(
                "unexpected encrypted transfer decision kind: {}",
                envelope.payload.inner_kind.as_str()
            ),
        )));
    }
    let decision: TransferDecision = EncryptedSessionPayload::open_control(&envelope, keys)
        .map_err(protocol_error_to_network)?;
    decision.validate().map_err(protocol_error_to_network)?;
    if decision.accepted && envelope.payload.inner_kind != MessageKind::FileAccept {
        return Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "accepted encrypted transfer decision must use file.accept",
        )));
    }
    if !decision.accepted && envelope.payload.inner_kind != MessageKind::FileDecline {
        return Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "declined encrypted transfer decision must use file.decline",
        )));
    }
    Ok(decision)
}

pub fn read_session_transfer_decision_once(
    stream: &mut impl Read,
    keys: &nekolink_protocol::SessionKeyMaterial,
    replay_window: &mut nekolink_protocol::SessionReplayWindow,
) -> NekoDropResult<TransferDecision> {
    let envelope = read_session_control_envelope(stream)?;
    if !matches!(
        envelope.payload.inner_kind,
        MessageKind::FileAccept | MessageKind::FileDecline
    ) {
        return Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::UnexpectedMessageKind,
            format!(
                "unexpected encrypted transfer decision kind: {}",
                envelope.payload.inner_kind.as_str()
            ),
        )));
    }
    let decision: TransferDecision =
        EncryptedSessionPayload::open_control_once(&envelope, keys, replay_window)
            .map_err(protocol_error_to_network)?;
    decision.validate().map_err(protocol_error_to_network)?;
    if decision.accepted && envelope.payload.inner_kind != MessageKind::FileAccept {
        return Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "accepted encrypted transfer decision must use file.accept",
        )));
    }
    if !decision.accepted && envelope.payload.inner_kind != MessageKind::FileDecline {
        return Err(protocol_error_to_network(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "declined encrypted transfer decision must use file.decline",
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

pub fn receive_encrypted_file_frames_with_expected_count<R, F, T>(
    stream: &mut R,
    expected_count: usize,
    keys: &SessionKeyMaterial,
    mut receive_file: F,
) -> NekoDropResult<Vec<T>>
where
    R: Read,
    F: FnMut(&FileFrameHeader, &mut std::io::Cursor<Vec<u8>>) -> NekoDropResult<T>,
{
    let expected_count = u32::try_from(expected_count).map_err(|_| {
        NekoDropError::Network(format!(
            "expected file frame count exceeds maximum: {expected_count}"
        ))
    })?;
    receive_encrypted_file_frames_checked(stream, Some(expected_count), keys, &mut receive_file)
}

fn receive_encrypted_file_frames_checked<R, F, T>(
    stream: &mut R,
    expected_count: Option<u32>,
    keys: &SessionKeyMaterial,
    receive_file: &mut F,
) -> NekoDropResult<Vec<T>>
where
    R: Read,
    F: FnMut(&FileFrameHeader, &mut std::io::Cursor<Vec<u8>>) -> NekoDropResult<T>,
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
        let plaintext = read_encrypted_file_payload(stream, keys, &header)?;
        let mut reader = std::io::Cursor::new(plaintext);
        received.push(receive_file(&header, &mut reader)?);
    }

    Ok(received)
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

fn write_encrypted_file_frame(
    stream: &mut impl Write,
    frame: &EncryptedFileFrame,
) -> NekoDropResult<()> {
    let header = serde_json::to_vec(&frame.header).map_err(|error| {
        NekoDropError::Network(format!(
            "failed to encode encrypted file frame header: {error}"
        ))
    })?;
    let header_len = u32::try_from(header.len())
        .map_err(|_| NekoDropError::Network("encrypted file frame header is too large".into()))?;
    let ciphertext_len = u32::try_from(frame.ciphertext.len()).map_err(|_| {
        NekoDropError::Network("encrypted file frame ciphertext is too large".into())
    })?;
    stream
        .write_all(&header_len.to_be_bytes())
        .map_err(|error| {
            NekoDropError::Network(format!(
                "failed to write encrypted file frame header length: {error}"
            ))
        })?;
    stream.write_all(&header).map_err(|error| {
        NekoDropError::Network(format!(
            "failed to write encrypted file frame header: {error}"
        ))
    })?;
    stream
        .write_all(&ciphertext_len.to_be_bytes())
        .map_err(|error| {
            NekoDropError::Network(format!(
                "failed to write encrypted file frame ciphertext length: {error}"
            ))
        })?;
    stream.write_all(&frame.ciphertext).map_err(|error| {
        NekoDropError::Network(format!(
            "failed to write encrypted file frame ciphertext: {error}"
        ))
    })?;
    Ok(())
}

fn read_encrypted_file_payload(
    stream: &mut impl Read,
    keys: &SessionKeyMaterial,
    file_header: &FileFrameHeader,
) -> NekoDropResult<Vec<u8>> {
    let remaining = file_header.size.saturating_sub(file_header.offset);
    let mut plaintext = Vec::with_capacity(remaining.min(usize::MAX as u64) as usize);
    let mut expected_offset = file_header.offset;
    while expected_offset < file_header.size {
        let frame = read_encrypted_file_frame(stream)?;
        if frame.header.manifest_path != file_header.manifest_path {
            return Err(NekoDropError::Network(format!(
                "encrypted file frame path mismatch: {} != {}",
                frame.header.manifest_path, file_header.manifest_path
            )));
        }
        if frame.header.offset != expected_offset {
            return Err(NekoDropError::Network(format!(
                "encrypted file frame offset mismatch for {}: {} != {}",
                file_header.manifest_path, frame.header.offset, expected_offset
            )));
        }
        let chunk = frame.open(keys).map_err(protocol_error_to_network)?;
        expected_offset = expected_offset.saturating_add(chunk.len() as u64);
        plaintext.extend_from_slice(&chunk);
    }
    if expected_offset != file_header.size {
        return Err(NekoDropError::Network(format!(
            "encrypted file payload size mismatch for {}: {} != {}",
            file_header.manifest_path, expected_offset, file_header.size
        )));
    }
    Ok(plaintext)
}

fn read_encrypted_file_frame(stream: &mut impl Read) -> NekoDropResult<EncryptedFileFrame> {
    let mut header_len_bytes = [0_u8; 4];
    stream.read_exact(&mut header_len_bytes).map_err(|error| {
        NekoDropError::Network(format!(
            "failed to read encrypted file frame header length: {error}"
        ))
    })?;
    let header_len = u32::from_be_bytes(header_len_bytes) as usize;
    if header_len == 0 || header_len > 64 * 1024 {
        return Err(NekoDropError::Network(format!(
            "invalid encrypted file frame header length: {header_len}"
        )));
    }
    let mut header_payload = vec![0_u8; header_len];
    stream.read_exact(&mut header_payload).map_err(|error| {
        NekoDropError::Network(format!(
            "failed to read encrypted file frame header: {error}"
        ))
    })?;
    let header: EncryptedFileFrameHeader =
        serde_json::from_slice(&header_payload).map_err(|error| {
            NekoDropError::Network(format!(
                "failed to decode encrypted file frame header: {error}"
            ))
        })?;
    header.validate().map_err(protocol_error_to_network)?;

    let mut ciphertext_len_bytes = [0_u8; 4];
    stream
        .read_exact(&mut ciphertext_len_bytes)
        .map_err(|error| {
            NekoDropError::Network(format!(
                "failed to read encrypted file frame ciphertext length: {error}"
            ))
        })?;
    let ciphertext_len = u32::from_be_bytes(ciphertext_len_bytes) as usize;
    if ciphertext_len == 0 || ciphertext_len > COPY_BUFFER_SIZE + 32 {
        return Err(NekoDropError::Network(format!(
            "invalid encrypted file frame ciphertext length: {ciphertext_len}"
        )));
    }
    let mut ciphertext = vec![0_u8; ciphertext_len];
    stream.read_exact(&mut ciphertext).map_err(|error| {
        NekoDropError::Network(format!(
            "failed to read encrypted file frame ciphertext: {error}"
        ))
    })?;

    Ok(EncryptedFileFrame { header, ciphertext })
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
            "JSON frame exceeds maximum size: {} > {}",
            payload.len(),
            MAX_JSON_FRAME_SIZE
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
            "invalid JSON frame length: {len} (max {MAX_JSON_FRAME_SIZE})"
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
        default_session_cipher_preference, Capability, DeviceIdentity, DeviceKind, MessageKind,
        PlatformKind, SessionFrameDirection, SessionFrameKind, SessionHelloPayload,
        SessionKeyMaterial, SessionReadyPayload, SessionTrafficFrameHeader,
        SESSION_CIPHER_XCHACHA20POLY1305, SESSION_TRAFFIC_KEY_LEN,
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
        let hello = SessionHelloPayload::default_crypto(
            "session-1",
            local_identity,
            "base64-local-ephemeral-public-key",
        );
        let ready = SessionReadyPayload::for_hello_with_cipher_preference(
            &hello,
            peer_identity,
            "base64-peer-ephemeral-public-key",
            &default_session_cipher_preference(),
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
    fn incoming_control_frame_can_read_session_hello_first_frame() {
        let identity = DeviceIdentity::new(
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
        let hello = SessionHelloPayload::default_crypto(
            "session-1",
            identity,
            "base64-local-ephemeral-public-key",
        );
        let mut buffer = Vec::new();

        write_session_hello(&mut buffer, &hello).unwrap();
        let received = read_incoming_control_frame(&mut Cursor::new(buffer)).unwrap();

        assert_eq!(received, IncomingControlFrame::SessionHello(hello));
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
        let hello = SessionHelloPayload::default_crypto(
            "session-1",
            local_identity,
            "base64-local-ephemeral-public-key",
        );
        let mut ready = SessionReadyPayload::for_hello_with_cipher_preference(
            &hello,
            peer_identity,
            "base64-peer-ephemeral-public-key",
            &default_session_cipher_preference(),
        )
        .unwrap();
        ready.handshake_hash = format!("sha256:{}", "0".repeat(64));

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
    fn encrypted_file_frames_round_trip_and_reject_tampered_chunk_header() {
        let dir = unique_temp_dir("tcp-encrypted-file-frame");
        let file = dir.join("sample.txt");
        fs::write(&file, b"hello encrypted file payload").unwrap();
        let outgoing = vec![OutgoingFileFrame::new(
            "drop/sample.txt",
            file,
            "sha256-placeholder",
        )];
        let keys = SessionKeyMaterial {
            send_key: [41_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [41_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let mut counters = nekolink_protocol::SessionTrafficCounters::default();
        let mut stream = Cursor::new(Vec::new());

        send_encrypted_file_frames_with_resume_and_cancel(
            &mut stream,
            "transfer-1",
            &outgoing,
            28,
            &[],
            &keys,
            &mut counters,
            SESSION_CIPHER_XCHACHA20POLY1305,
            |_| {},
            || false,
        )
        .unwrap();

        let payload = stream.into_inner();

        let received = receive_encrypted_file_frames_with_expected_count(
            &mut Cursor::new(payload.clone()),
            1,
            &keys,
            |header, reader| {
                let mut plaintext = Vec::new();
                reader.read_to_end(&mut plaintext).unwrap();
                Ok((header.clone(), plaintext))
            },
        )
        .unwrap();
        assert_eq!(received[0].0.manifest_path, "drop/sample.txt");
        assert_eq!(received[0].1, b"hello encrypted file payload");

        let mut tampered_payload = payload;
        let manifest_header_len =
            u32::from_be_bytes(tampered_payload[4..8].try_into().unwrap()) as usize;
        let encrypted_header_len_start = 8 + manifest_header_len;
        let encrypted_header_len = u32::from_be_bytes(
            tampered_payload[encrypted_header_len_start..encrypted_header_len_start + 4]
                .try_into()
                .unwrap(),
        ) as usize;
        let encrypted_header_start = encrypted_header_len_start + 4;
        let mut encrypted_header: nekolink_protocol::EncryptedFileFrameHeader =
            serde_json::from_slice(
                &tampered_payload
                    [encrypted_header_start..encrypted_header_start + encrypted_header_len],
            )
            .unwrap();
        encrypted_header.transfer_id = "transfer-2".to_string();
        let tampered_header = serde_json::to_vec(&encrypted_header).unwrap();
        assert_eq!(tampered_header.len(), encrypted_header_len);
        tampered_payload[encrypted_header_start..encrypted_header_start + encrypted_header_len]
            .copy_from_slice(&tampered_header);

        let error = receive_encrypted_file_frames_with_expected_count(
            &mut Cursor::new(tampered_payload),
            1,
            &keys,
            |header, reader| {
                let mut plaintext = Vec::new();
                reader.read_to_end(&mut plaintext).unwrap();
                Ok((header.clone(), plaintext))
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("failed to open session payload"));

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
    fn large_transfer_offer_round_trips_under_control_frame_limit() {
        let files = (0..5_000)
            .map(|index| TransferOfferFile {
                manifest_path: format!(
                    "drop/album-{}/disc-{}/track-{:04}-sample-audio-file.m4a",
                    index / 100,
                    index / 10,
                    index
                ),
                size: 128 * 1024 * 1024,
                sha256: "a".repeat(64),
            })
            .collect::<Vec<_>>();
        let offer = TransferOffer::new("transfer-large-folder", "drop", files);
        let mut buffer = Vec::new();

        write_transfer_offer(&mut buffer, &offer).unwrap();
        let received = read_transfer_offer(&mut Cursor::new(buffer)).unwrap();

        assert_eq!(received, offer);
    }

    #[test]
    fn json_frame_writer_rejects_control_payload_over_limit() {
        let payload = serde_json::json!({
            "blob": "x".repeat(MAX_JSON_FRAME_SIZE)
        });
        let mut buffer = Vec::new();

        let error = write_json_frame(&mut buffer, &payload).unwrap_err();

        assert!(error
            .to_string()
            .contains("JSON frame exceeds maximum size"));
    }

    #[test]
    fn json_frame_reader_rejects_declared_control_payload_over_limit() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&((MAX_JSON_FRAME_SIZE + 1) as u32).to_be_bytes());

        let error = read_json_frame::<serde_json::Value>(&mut Cursor::new(buffer)).unwrap_err();

        assert!(error.to_string().contains("invalid JSON frame length"));
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
    fn encrypted_session_control_envelope_round_trips_through_json_frame() {
        let keys = SessionKeyMaterial {
            send_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            7,
        )
        .unwrap();
        let envelope = EncryptedSessionPayload::seal_control(
            "session-1",
            "session-1:control-1",
            &keys,
            header,
            MessageKind::FileDecline,
            &TransferDecision::decline("no"),
        )
        .unwrap();
        let mut buffer = Vec::new();

        write_session_control_envelope(&mut buffer, &envelope).unwrap();
        let received = read_session_control_envelope(&mut Cursor::new(buffer)).unwrap();
        let opened: TransferDecision =
            EncryptedSessionPayload::open_control(&received, &keys).unwrap();

        assert_eq!(received.kind, MessageKind::SessionControl);
        assert_eq!(opened, TransferDecision::decline("no"));
    }

    #[test]
    fn session_control_reader_rejects_unexpected_outer_kind() {
        let keys = SessionKeyMaterial {
            send_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            7,
        )
        .unwrap();
        let mut envelope = EncryptedSessionPayload::seal_control(
            "session-1",
            "session-1:control-1",
            &keys,
            header,
            MessageKind::FileDecline,
            &TransferDecision::decline("no"),
        )
        .unwrap();
        envelope.kind = MessageKind::FileOffer;
        let mut buffer = Vec::new();
        write_json_frame(&mut buffer, &envelope).unwrap();

        let error = read_session_control_envelope(&mut Cursor::new(buffer)).unwrap_err();

        assert!(error.to_string().contains("unexpected message kind"));
    }

    #[test]
    fn encrypted_session_control_payload_reader_opens_inner_payload() {
        let keys = SessionKeyMaterial {
            send_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            9,
        )
        .unwrap();
        let envelope = EncryptedSessionPayload::seal_control(
            "session-1",
            "session-1:control-2",
            &keys,
            header,
            MessageKind::FileDecline,
            &TransferDecision::decline("busy"),
        )
        .unwrap();
        let mut buffer = Vec::new();
        write_session_control_envelope(&mut buffer, &envelope).unwrap();

        let opened: TransferDecision =
            read_session_control_payload(&mut Cursor::new(buffer), &keys).unwrap();

        assert_eq!(opened, TransferDecision::decline("busy"));
    }

    #[test]
    fn encrypted_session_control_payload_reader_rejects_replayed_frame() {
        let keys = SessionKeyMaterial {
            send_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            9,
        )
        .unwrap();
        let envelope = EncryptedSessionPayload::seal_control(
            "session-1",
            "session-1:control-2",
            &keys,
            header,
            MessageKind::FileDecline,
            &TransferDecision::decline("busy"),
        )
        .unwrap();
        let mut first_buffer = Vec::new();
        let mut second_buffer = Vec::new();
        write_session_control_envelope(&mut first_buffer, &envelope).unwrap();
        write_session_control_envelope(&mut second_buffer, &envelope).unwrap();
        let mut replay_window = nekolink_protocol::SessionReplayWindow::default();

        let opened: TransferDecision = read_session_control_payload_once(
            &mut Cursor::new(first_buffer),
            &keys,
            &mut replay_window,
        )
        .unwrap();
        let error = read_session_control_payload_once::<TransferDecision>(
            &mut Cursor::new(second_buffer),
            &keys,
            &mut replay_window,
        )
        .unwrap_err();

        assert_eq!(opened, TransferDecision::decline("busy"));
        assert!(error.to_string().contains("replayed session frame"));
    }

    #[test]
    fn encrypted_session_control_payload_kind_reader_rejects_replayed_frame() {
        let keys = SessionKeyMaterial {
            send_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            10,
        )
        .unwrap();
        let envelope = EncryptedSessionPayload::seal_control(
            "session-1",
            "session-1:control-2",
            &keys,
            header,
            MessageKind::FileDecline,
            &TransferDecision::decline("busy"),
        )
        .unwrap();
        let mut first_buffer = Vec::new();
        let mut second_buffer = Vec::new();
        write_session_control_envelope(&mut first_buffer, &envelope).unwrap();
        write_session_control_envelope(&mut second_buffer, &envelope).unwrap();
        let mut replay_window = nekolink_protocol::SessionReplayWindow::default();

        let opened: TransferDecision = read_session_control_payload_kind_once(
            &mut Cursor::new(first_buffer),
            &keys,
            &mut replay_window,
            MessageKind::FileDecline,
        )
        .unwrap();
        let error = read_session_control_payload_kind_once::<TransferDecision>(
            &mut Cursor::new(second_buffer),
            &keys,
            &mut replay_window,
            MessageKind::FileDecline,
        )
        .unwrap_err();

        assert_eq!(opened, TransferDecision::decline("busy"));
        assert!(error.to_string().contains("replayed session frame"));
    }

    #[test]
    fn encrypted_session_control_payload_writer_seals_transfer_offer() {
        let keys = SessionKeyMaterial {
            send_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            11,
        )
        .unwrap();
        let offer = TransferOffer::new(
            "transfer-1",
            "drop",
            vec![TransferOfferFile {
                manifest_path: "drop/sample.txt".to_string(),
                size: 11,
                sha256: "abc123".to_string(),
            }],
        );
        let mut buffer = Vec::new();

        write_session_control_payload(
            &mut buffer,
            "session-1",
            "session-1:control-3",
            &keys,
            header,
            MessageKind::FileOffer,
            &offer,
        )
        .unwrap();
        let opened: TransferOffer =
            read_session_control_payload(&mut Cursor::new(buffer), &keys).unwrap();

        assert_eq!(opened, offer);
    }

    #[test]
    fn encrypted_session_control_payload_reader_checks_inner_kind() {
        let keys = SessionKeyMaterial {
            send_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            12,
        )
        .unwrap();
        let mut buffer = Vec::new();
        write_session_control_payload(
            &mut buffer,
            "session-1",
            "session-1:control-4",
            &keys,
            header,
            MessageKind::FileDecline,
            &TransferDecision::decline("busy"),
        )
        .unwrap();

        let error = read_session_control_payload_kind::<TransferOffer>(
            &mut Cursor::new(buffer),
            &keys,
            MessageKind::FileOffer,
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("unexpected encrypted control kind"));
    }

    #[test]
    fn session_transfer_offer_helpers_use_encrypted_control_frames() {
        let keys = SessionKeyMaterial {
            send_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            13,
        )
        .unwrap();
        let offer = TransferOffer::new(
            "transfer-1",
            "drop",
            vec![TransferOfferFile {
                manifest_path: "drop/sample.txt".to_string(),
                size: 11,
                sha256: "abc123".to_string(),
            }],
        );
        let mut buffer = Vec::new();

        write_session_transfer_offer(
            &mut buffer,
            "session-1",
            "session-1:offer-1",
            &keys,
            header,
            &offer,
        )
        .unwrap();
        let received = read_session_transfer_offer(&mut Cursor::new(buffer), &keys).unwrap();

        assert_eq!(received, offer);
    }

    #[test]
    fn session_transfer_offer_once_helper_rejects_replayed_frame() {
        let keys = SessionKeyMaterial {
            send_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            14,
        )
        .unwrap();
        let offer = TransferOffer::new(
            "transfer-1",
            "drop",
            vec![TransferOfferFile {
                manifest_path: "drop/sample.txt".to_string(),
                size: 11,
                sha256: "abc123".to_string(),
            }],
        );
        let envelope = EncryptedSessionPayload::seal_control(
            "session-1",
            "session-1:offer-2",
            &keys,
            header,
            MessageKind::FileOffer,
            &offer,
        )
        .unwrap();
        let mut first_buffer = Vec::new();
        let mut second_buffer = Vec::new();
        write_session_control_envelope(&mut first_buffer, &envelope).unwrap();
        write_session_control_envelope(&mut second_buffer, &envelope).unwrap();
        let mut replay_window = nekolink_protocol::SessionReplayWindow::default();

        assert_eq!(
            read_session_transfer_offer_once(
                &mut Cursor::new(first_buffer),
                &keys,
                &mut replay_window,
            )
            .unwrap(),
            offer
        );
        let error = read_session_transfer_offer_once(
            &mut Cursor::new(second_buffer),
            &keys,
            &mut replay_window,
        )
        .unwrap_err();

        assert!(error.to_string().contains("replayed session frame"));
    }

    #[test]
    fn session_transfer_decision_helpers_use_encrypted_control_frames() {
        let keys = SessionKeyMaterial {
            send_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let accept_header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            14,
        )
        .unwrap();
        let decline_header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            15,
        )
        .unwrap();
        let mut accepted_buffer = Vec::new();
        let mut declined_buffer = Vec::new();

        write_session_transfer_decision(
            &mut accepted_buffer,
            "session-1",
            "session-1:accept-1",
            &keys,
            accept_header,
            &TransferDecision::accept(),
        )
        .unwrap();
        write_session_transfer_decision(
            &mut declined_buffer,
            "session-1",
            "session-1:decline-1",
            &keys,
            decline_header,
            &TransferDecision::decline("busy"),
        )
        .unwrap();

        assert_eq!(
            read_session_transfer_decision(&mut Cursor::new(accepted_buffer), &keys).unwrap(),
            TransferDecision::accept()
        );
        assert_eq!(
            read_session_transfer_decision(&mut Cursor::new(declined_buffer), &keys).unwrap(),
            TransferDecision::decline("busy")
        );
    }

    #[test]
    fn session_transfer_decision_once_helper_rejects_replayed_frame() {
        let keys = SessionKeyMaterial {
            send_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            16,
        )
        .unwrap();
        let envelope = EncryptedSessionPayload::seal_control(
            "session-1",
            "session-1:accept-2",
            &keys,
            header,
            MessageKind::FileAccept,
            &TransferDecision::accept(),
        )
        .unwrap();
        let mut first_buffer = Vec::new();
        let mut second_buffer = Vec::new();
        write_session_control_envelope(&mut first_buffer, &envelope).unwrap();
        write_session_control_envelope(&mut second_buffer, &envelope).unwrap();
        let mut replay_window = nekolink_protocol::SessionReplayWindow::default();

        assert_eq!(
            read_session_transfer_decision_once(
                &mut Cursor::new(first_buffer),
                &keys,
                &mut replay_window,
            )
            .unwrap(),
            TransferDecision::accept()
        );
        let error = read_session_transfer_decision_once(
            &mut Cursor::new(second_buffer),
            &keys,
            &mut replay_window,
        )
        .unwrap_err();

        assert!(error.to_string().contains("replayed session frame"));
    }

    #[test]
    fn encrypted_session_control_payload_reader_rejects_tampered_envelope_aad() {
        let keys = SessionKeyMaterial {
            send_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [23_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            9,
        )
        .unwrap();
        let mut envelope = EncryptedSessionPayload::seal_control(
            "session-1",
            "session-1:control-2",
            &keys,
            header,
            MessageKind::FileDecline,
            &TransferDecision::decline("busy"),
        )
        .unwrap();
        envelope.message_id = "session-1:control-tampered".to_string();
        let mut buffer = Vec::new();
        write_json_frame(&mut buffer, &envelope).unwrap();

        let error =
            read_session_control_payload::<TransferDecision>(&mut Cursor::new(buffer), &keys)
                .unwrap_err();

        assert!(error.to_string().contains("failed to open session payload"));
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
