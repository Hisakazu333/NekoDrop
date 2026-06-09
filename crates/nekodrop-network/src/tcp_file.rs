use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

use nekodrop_core::{NekoDropError, NekoDropResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFrameHeader {
    pub manifest_path: String,
    pub size: u64,
    pub sha256: String,
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
    let bytes_sent = std::io::copy(&mut file, stream).map_err(|error| {
        NekoDropError::Network(format!(
            "failed to send file {} over TCP: {error}",
            file_path.display()
        ))
    })?;
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
    let count = u32::try_from(files.len())
        .map_err(|_| NekoDropError::Network("too many files in one transfer".into()))?;
    stream
        .write_all(&count.to_be_bytes())
        .map_err(|error| NekoDropError::Network(format!("failed to write file count: {error}")))?;

    let mut sent = Vec::with_capacity(files.len());
    for file in files {
        sent.push(send_single_file_frame(
            stream,
            file.manifest_path.clone(),
            &file.file_path,
            file.sha256.clone(),
        )?);
    }

    Ok(sent)
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
