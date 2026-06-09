use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use nekodrop_core::{NekoDropError, NekoDropResult};
use nekodrop_network::{
    accept_file_frames, send_file_frames, ConnectionTicket, Endpoint, OutgoingFileFrame,
    SentFileFrame, TransportKind,
};
use nekodrop_storage::{create_source_plan_from_paths, write_received_file, ReceivedFile};

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
    if endpoint.transport != TransportKind::Tcp {
        return Err(NekoDropError::Network(format!(
            "unsupported transport for file send: {:?}",
            endpoint.transport
        )));
    }

    let plan = create_source_plan_from_paths(paths)?;
    let outgoing = outgoing_frames_from_plan(&plan);
    let mut stream =
        TcpStream::connect((endpoint.host.as_str(), endpoint.port)).map_err(|error| {
            NekoDropError::Network(format!(
                "failed to connect to {}:{}: {error}",
                endpoint.host, endpoint.port
            ))
        })?;
    let sent_files = send_file_frames(&mut stream, &outgoing)?;

    Ok(TransferSendReport { plan, sent_files })
}

pub fn accept_transfer(
    listener: &TcpListener,
    receive_dir: &Path,
) -> NekoDropResult<TransferReceiveReport> {
    let files = accept_file_frames(listener, |header, stream| {
        write_received_file(
            receive_dir,
            &header.manifest_path,
            header.size,
            &header.sha256,
            stream,
        )
    })?;

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
