use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

use nekodrop_core::{NekoDropError, NekoDropResult};
use serde::{Deserialize, Serialize};

const COPY_BUFFER_SIZE: usize = 64 * 1024;
const MAX_JSON_FRAME_SIZE: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFrameHeader {
    pub manifest_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferOfferFile {
    pub manifest_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferOffer {
    pub protocol: String,
    pub transfer_id: String,
    pub root_name: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub files: Vec<TransferOfferFile>,
}

impl TransferOffer {
    pub fn new(
        transfer_id: impl Into<String>,
        root_name: impl Into<String>,
        files: Vec<TransferOfferFile>,
    ) -> Self {
        let file_count = files.len();
        let total_bytes = files.iter().map(|file| file.size).sum();
        Self {
            protocol: "nekodrop-transfer-v1".to_string(),
            transfer_id: transfer_id.into(),
            root_name: root_name.into(),
            file_count,
            total_bytes,
            files,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferDecision {
    pub accepted: bool,
    pub reason: Option<String>,
}

impl TransferDecision {
    pub fn accept() -> Self {
        Self {
            accepted: true,
            reason: None,
        }
    }

    pub fn decline(reason: impl Into<String>) -> Self {
        Self {
            accepted: false,
            reason: Some(reason.into()),
        }
    }
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
    stream: &mut TcpStream,
    manifest_path: impl Into<String>,
    file_path: &Path,
    sha256: impl Into<String>,
) -> NekoDropResult<SentFileFrame> {
    send_single_file_frame_with_progress(stream, manifest_path, file_path, sha256, |_| {})
}

pub fn send_single_file_frame_with_progress<F>(
    stream: &mut TcpStream,
    manifest_path: impl Into<String>,
    file_path: &Path,
    sha256: impl Into<String>,
    mut on_progress: F,
) -> NekoDropResult<SentFileFrame>
where
    F: FnMut(u64),
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

    let header = FileFrameHeader {
        manifest_path: manifest_path.clone(),
        size: metadata.len(),
        sha256: sha256.into(),
    };
    write_header(stream, &header)?;

    let mut file = File::open(file_path).map_err(|error| {
        NekoDropError::Network(format!("failed to open {}: {error}", file_path.display()))
    })?;
    let mut buffer = [0_u8; COPY_BUFFER_SIZE];
    let mut bytes_sent = 0_u64;
    loop {
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
        on_progress(bytes_sent);
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
    stream: &mut TcpStream,
    files: &[OutgoingFileFrame],
) -> NekoDropResult<Vec<SentFileFrame>> {
    send_file_frames_with_progress(stream, files, 0, |_| {})
}

pub fn send_file_frames_with_progress<F>(
    stream: &mut TcpStream,
    files: &[OutgoingFileFrame],
    total_bytes: u64,
    mut on_progress: F,
) -> NekoDropResult<Vec<SentFileFrame>>
where
    F: FnMut(TransferProgress),
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
    let mut bytes_transferred = 0_u64;

    for (index, file) in files.iter().enumerate() {
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
        let mut last_file_bytes = 0_u64;
        let sent_frame = send_single_file_frame_with_progress(
            stream,
            file.manifest_path.clone(),
            &file.file_path,
            file.sha256.clone(),
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
        )?;
        sent.push(sent_frame);
    }

    Ok(sent)
}

pub fn write_transfer_offer(stream: &mut TcpStream, offer: &TransferOffer) -> NekoDropResult<()> {
    if offer.protocol != "nekodrop-transfer-v1" {
        return Err(NekoDropError::Network(format!(
            "unsupported transfer offer protocol: {}",
            offer.protocol
        )));
    }
    write_json_frame(stream, offer)
}

pub fn read_transfer_offer(stream: &mut TcpStream) -> NekoDropResult<TransferOffer> {
    let offer: TransferOffer = read_json_frame(stream)?;
    if offer.protocol != "nekodrop-transfer-v1" {
        return Err(NekoDropError::Network(format!(
            "unsupported transfer offer protocol: {}",
            offer.protocol
        )));
    }
    if offer.file_count != offer.files.len() {
        return Err(NekoDropError::Network(format!(
            "transfer offer file count mismatch: {} != {}",
            offer.file_count,
            offer.files.len()
        )));
    }
    let total_bytes = offer.files.iter().map(|file| file.size).sum::<u64>();
    if offer.total_bytes != total_bytes {
        return Err(NekoDropError::Network(format!(
            "transfer offer size mismatch: {} != {}",
            offer.total_bytes, total_bytes
        )));
    }
    Ok(offer)
}

pub fn write_transfer_decision(
    stream: &mut TcpStream,
    decision: &TransferDecision,
) -> NekoDropResult<()> {
    write_json_frame(stream, decision)
}

pub fn read_transfer_decision(stream: &mut TcpStream) -> NekoDropResult<TransferDecision> {
    read_json_frame(stream)
}

pub fn receive_single_file_frame<F, T>(stream: &mut TcpStream, receive_file: F) -> NekoDropResult<T>
where
    F: FnOnce(&FileFrameHeader, &mut TcpStream) -> NekoDropResult<T>,
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

pub fn receive_file_frames<F, T>(
    stream: &mut TcpStream,
    mut receive_file: F,
) -> NekoDropResult<Vec<T>>
where
    F: FnMut(&FileFrameHeader, &mut TcpStream) -> NekoDropResult<T>,
{
    let count = read_file_count(stream)?;
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

fn write_header(stream: &mut TcpStream, header: &FileFrameHeader) -> NekoDropResult<()> {
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

fn read_header(stream: &mut TcpStream) -> NekoDropResult<FileFrameHeader> {
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

fn read_file_count(stream: &mut TcpStream) -> NekoDropResult<u32> {
    let mut count_bytes = [0_u8; 4];
    stream
        .read_exact(&mut count_bytes)
        .map_err(|error| NekoDropError::Network(format!("failed to read file count: {error}")))?;
    Ok(u32::from_be_bytes(count_bytes))
}

fn write_json_frame<T: Serialize>(stream: &mut TcpStream, value: &T) -> NekoDropResult<()> {
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

fn read_json_frame<T: for<'de> Deserialize<'de>>(stream: &mut TcpStream) -> NekoDropResult<T> {
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::thread;

    use nekodrop_storage::{create_source_plan_from_paths, sha256_file, write_received_file};

    use super::*;

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
