use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;

use crate::app_state::{
    AppState, LocalBridgeRuntimeState, LocalBridgeRuntimeStatusState, TransferStatusState,
};
use crate::commands;
use crate::trusted_devices::TrustedDeviceRecord;

const LOCAL_BRIDGE_BIND_HOST: &str = "127.0.0.1";
const LOCAL_BRIDGE_DEFAULT_PORT: u16 = 45921;
const LOCAL_BRIDGE_BIND_ATTEMPTS: u16 = 20;
const LOCAL_BRIDGE_HEADER_LIMIT_BYTES: usize = 16 * 1024;
const LOCAL_BRIDGE_MAX_REQUEST_BYTES: usize = 64 * 1024;
const LOCAL_BRIDGE_READ_TIMEOUT: Duration = Duration::from_secs(5);
const LOCAL_BRIDGE_REQUEST_PATH: &str = "/bridge/request";

struct LocalBridgeRuntimeContext {
    trusted_devices: Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    transfer_status: Arc<Mutex<Option<TransferStatusState>>>,
    runtime: Arc<LocalBridgeRuntimeState>,
}

#[derive(Debug)]
struct LocalBridgeHttpRequest {
    path: String,
    body: String,
}

#[derive(Debug, Serialize)]
struct LocalBridgeErrorResponse {
    status: String,
    message: String,
}

pub fn start_local_bridge_runtime(state: &AppState) {
    let context = Arc::new(LocalBridgeRuntimeContext {
        trusted_devices: state.trusted_devices.clone(),
        transfer_status: state.transfer_status.clone(),
        runtime: state.local_bridge_runtime.clone(),
    });

    thread::spawn(move || {
        let listener = match bind_local_bridge_listener(LOCAL_BRIDGE_DEFAULT_PORT) {
            Ok(listener) => listener,
            Err(error) => {
                set_local_bridge_runtime_status(
                    &context.runtime,
                    false,
                    LOCAL_BRIDGE_DEFAULT_PORT,
                    Some(error.clone()),
                );
                eprintln!("local bridge runtime unavailable: {error}");
                return;
            }
        };
        let Ok(address) = listener.local_addr() else {
            let error = "listener address is unavailable".to_string();
            set_local_bridge_runtime_status(
                &context.runtime,
                false,
                LOCAL_BRIDGE_DEFAULT_PORT,
                Some(error.clone()),
            );
            eprintln!("local bridge runtime unavailable: {error}");
            return;
        };
        set_local_bridge_runtime_status(&context.runtime, true, address.port(), None);
        eprintln!("local bridge runtime listening on {address}");

        for stream in listener.incoming() {
            let context = context.clone();
            match stream {
                Ok(stream) => {
                    thread::spawn(move || {
                        handle_local_bridge_connection(stream, context);
                    });
                }
                Err(error) => {
                    eprintln!("local bridge runtime accept failed: {error}");
                }
            }
        }
    });
}

fn handle_local_bridge_connection(mut stream: TcpStream, context: Arc<LocalBridgeRuntimeContext>) {
    let _ = stream.set_read_timeout(Some(LOCAL_BRIDGE_READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(LOCAL_BRIDGE_READ_TIMEOUT));

    let Ok(peer_addr) = stream.peer_addr() else {
        let _ = write_local_bridge_error_response(
            &mut stream,
            403,
            "Forbidden",
            "peer address unavailable",
        );
        return;
    };
    if !peer_addr.ip().is_loopback() {
        let _ = write_local_bridge_error_response(
            &mut stream,
            403,
            "Forbidden",
            "local bridge only accepts loopback clients",
        );
        return;
    }

    let request = match read_local_bridge_http_request(&mut stream) {
        Ok(request) => request,
        Err(error) => {
            let (status, reason) = if error.contains("too large") {
                (413, "Payload Too Large")
            } else if error.contains("POST") {
                (405, "Method Not Allowed")
            } else if error.contains("/bridge/request") {
                (404, "Not Found")
            } else {
                (400, "Bad Request")
            };
            let _ = write_local_bridge_error_response(&mut stream, status, reason, &error);
            return;
        }
    };

    let response = commands::handle_local_bridge_request_for_runtime(
        &context.trusted_devices,
        &context.transfer_status,
        &context.runtime,
        &request.body,
    );
    match response {
        Ok(response) => {
            let _ = write_local_bridge_json_response(&mut stream, 200, "OK", &response);
        }
        Err(error) => {
            let _ = write_local_bridge_error_response(&mut stream, 400, "Bad Request", &error);
        }
    }
}

fn validate_local_bridge_bind_host(bind_host: &str) -> Result<(), String> {
    if bind_host == LOCAL_BRIDGE_BIND_HOST {
        Ok(())
    } else {
        Err(format!(
            "local bridge must bind to {LOCAL_BRIDGE_BIND_HOST}, got {bind_host}"
        ))
    }
}

fn bind_local_bridge_listener(requested_port: u16) -> Result<TcpListener, String> {
    validate_local_bridge_bind_host(LOCAL_BRIDGE_BIND_HOST)?;
    let bind_address = local_bridge_bind_address(requested_port)?;
    if requested_port == 0 {
        return TcpListener::bind(bind_address).map_err(|error| format!("{bind_address}: {error}"));
    }

    let mut last_error = None;
    for offset in 0..LOCAL_BRIDGE_BIND_ATTEMPTS {
        let Some(port) = requested_port.checked_add(offset) else {
            break;
        };
        let bind_address = local_bridge_bind_address(port)?;
        match TcpListener::bind(bind_address) {
            Ok(listener) => return Ok(listener),
            Err(error) => last_error = Some(format!("{bind_address}: {error}")),
        }
    }

    Err(format!(
        "local bridge could not bind to {LOCAL_BRIDGE_BIND_HOST}:{}..{}: {}",
        requested_port,
        requested_port.saturating_add(LOCAL_BRIDGE_BIND_ATTEMPTS.saturating_sub(1)),
        last_error.unwrap_or_else(|| "no port was attempted".to_string())
    ))
}

fn local_bridge_bind_address(requested_port: u16) -> Result<SocketAddr, String> {
    format!("{LOCAL_BRIDGE_BIND_HOST}:{requested_port}")
        .parse::<SocketAddr>()
        .map_err(|error| format!("invalid local bridge bind address: {error}"))
}

fn read_local_bridge_http_request(
    stream: &mut TcpStream,
) -> Result<LocalBridgeHttpRequest, String> {
    let mut buffer = Vec::new();
    let mut header_end = None;
    let mut chunk = [0_u8; 1024];

    while header_end.is_none() {
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("failed to read local bridge request: {error}"))?;
        if read == 0 {
            return Err("local bridge request closed before headers completed".to_string());
        }
        buffer.extend_from_slice(&chunk[..read]);
        header_end = find_header_end(&buffer);
        if header_end.is_none() && buffer.len() > LOCAL_BRIDGE_HEADER_LIMIT_BYTES {
            return Err("local bridge request headers too large".to_string());
        }
    }

    let header_end = header_end.expect("checked above");
    if header_end > LOCAL_BRIDGE_HEADER_LIMIT_BYTES {
        return Err("local bridge request headers too large".to_string());
    }
    let content_length = content_length_from_headers(&buffer[..header_end])?;
    if content_length > LOCAL_BRIDGE_MAX_REQUEST_BYTES {
        return Err(format!(
            "local bridge request body too large: {content_length} bytes"
        ));
    }

    let total_len = header_end + 4 + content_length;
    while buffer.len() < total_len {
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("failed to read local bridge request body: {error}"))?;
        if read == 0 {
            return Err("local bridge request closed before body completed".to_string());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len().saturating_sub(header_end + 4) > LOCAL_BRIDGE_MAX_REQUEST_BYTES {
            return Err("local bridge request body too large".to_string());
        }
    }

    parse_local_bridge_http_request(&buffer[..total_len])
}

fn parse_local_bridge_http_request(bytes: &[u8]) -> Result<LocalBridgeHttpRequest, String> {
    let header_end = find_header_end(bytes)
        .ok_or_else(|| "local bridge request headers are incomplete".to_string())?;
    let headers = std::str::from_utf8(&bytes[..header_end])
        .map_err(|error| format!("local bridge request headers are not UTF-8: {error}"))?;
    let mut lines = headers.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "local bridge request line is missing".to_string())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default();
    let path = request_parts.next().unwrap_or_default();
    if method != "POST" {
        return Err("local bridge only accepts POST requests".to_string());
    }
    if path != LOCAL_BRIDGE_REQUEST_PATH {
        return Err(format!(
            "local bridge only accepts {LOCAL_BRIDGE_REQUEST_PATH}"
        ));
    }

    let content_length = content_length_from_headers(&bytes[..header_end])?;
    if content_length > LOCAL_BRIDGE_MAX_REQUEST_BYTES {
        return Err(format!(
            "local bridge request body too large: {content_length} bytes"
        ));
    }

    let body_start = header_end + 4;
    let body_end = body_start
        .checked_add(content_length)
        .ok_or_else(|| "local bridge request body length overflow".to_string())?;
    if bytes.len() < body_end {
        return Err("local bridge request body is incomplete".to_string());
    }
    let body = std::str::from_utf8(&bytes[body_start..body_end])
        .map_err(|error| format!("local bridge request body is not UTF-8: {error}"))?;

    Ok(LocalBridgeHttpRequest {
        path: path.to_string(),
        body: body.to_string(),
    })
}

fn content_length_from_headers(headers: &[u8]) -> Result<usize, String> {
    let headers = std::str::from_utf8(headers)
        .map_err(|error| format!("local bridge request headers are not UTF-8: {error}"))?;
    for line in headers.split("\r\n").skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            let length = value
                .trim()
                .parse::<usize>()
                .map_err(|error| format!("invalid local bridge Content-Length: {error}"))?;
            return Ok(length);
        }
        if name.eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("chunked"))
        {
            return Err("local bridge does not accept chunked requests".to_string());
        }
    }
    Err("local bridge request requires Content-Length".to_string())
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_local_bridge_error_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    message: &str,
) -> std::io::Result<()> {
    write_local_bridge_json_response(
        stream,
        status,
        reason,
        &LocalBridgeErrorResponse {
            status: "error".to_string(),
            message: message.to_string(),
        },
    )
}

fn write_local_bridge_json_response<T: Serialize>(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    value: &T,
) -> std::io::Result<()> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| {
        br#"{"status":"error","message":"failed to serialize local bridge response"}"#.to_vec()
    });
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)
}

pub(crate) fn local_bridge_runtime_status(
    runtime: &LocalBridgeRuntimeState,
) -> LocalBridgeRuntimeStatusSnapshot {
    let status = runtime
        .status
        .lock()
        .map(|status| status.clone())
        .unwrap_or_default();
    let pending_authorization_client =
        runtime
            .pending_authorization
            .lock()
            .ok()
            .and_then(|pending| {
                pending
                    .as_ref()
                    .map(|pending| pending.client.display_name.clone())
            });
    let authorization_count = runtime
        .authorizations
        .lock()
        .map(|authorizations| authorizations.len())
        .unwrap_or_default();

    LocalBridgeRuntimeStatusSnapshot {
        active: status.active,
        bind_host: status.bind_host,
        port: status.port,
        request_path: status.request_path,
        max_request_bytes: status.max_request_bytes,
        pending_authorization_client,
        authorization_count,
        last_error: status.last_error,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalBridgeRuntimeStatusSnapshot {
    pub active: bool,
    pub bind_host: String,
    pub port: u16,
    pub request_path: String,
    pub max_request_bytes: usize,
    pub pending_authorization_client: Option<String>,
    pub authorization_count: usize,
    pub last_error: Option<String>,
}

fn set_local_bridge_runtime_status(
    runtime: &LocalBridgeRuntimeState,
    active: bool,
    port: u16,
    last_error: Option<String>,
) {
    if let Ok(mut status) = runtime.status.lock() {
        *status = LocalBridgeRuntimeStatusState {
            active,
            bind_host: LOCAL_BRIDGE_BIND_HOST.to_string(),
            port,
            request_path: LOCAL_BRIDGE_REQUEST_PATH.to_string(),
            max_request_bytes: LOCAL_BRIDGE_MAX_REQUEST_BYTES,
            last_error,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_bridge_bind_address_is_loopback_only() {
        let address = local_bridge_bind_address(45921).unwrap();

        assert!(address.ip().is_loopback());
        assert_eq!(address.ip().to_string(), "127.0.0.1");
        assert_eq!(address.port(), 45921);
    }

    #[test]
    fn local_bridge_rejects_non_loopback_bind_address() {
        let error = validate_local_bridge_bind_host("0.0.0.0").unwrap_err();

        assert!(error.contains("127.0.0.1"));
    }

    #[test]
    fn local_bridge_accepts_read_only_http_request() {
        let request = serde_json::json!({
            "kind": "transfer.status",
            "payload": {
                "request_id": "bridge-request-status",
                "transfer_id": null
            }
        })
        .to_string();
        let http = format!(
            "POST /bridge/request HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            request.len(),
            request
        );

        let parsed = parse_local_bridge_http_request(http.as_bytes()).unwrap();

        assert_eq!(parsed.path, "/bridge/request");
        assert_eq!(parsed.body, request);
    }

    #[test]
    fn local_bridge_rejects_oversized_request_body() {
        let body = "x".repeat(LOCAL_BRIDGE_MAX_REQUEST_BYTES + 1);
        let http = format!(
            "POST /bridge/request HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        let error = parse_local_bridge_http_request(http.as_bytes()).unwrap_err();

        assert!(error.contains("too large"));
    }

    #[test]
    fn local_bridge_rejects_mutating_method_and_path() {
        let method_error =
            parse_local_bridge_http_request(b"GET /bridge/request HTTP/1.1\r\n\r\n").unwrap_err();
        let path_error =
            parse_local_bridge_http_request(b"POST /wrong HTTP/1.1\r\nContent-Length: 0\r\n\r\n")
                .unwrap_err();

        assert!(method_error.contains("POST"));
        assert!(path_error.contains("/bridge/request"));
    }

    #[test]
    fn local_bridge_runtime_keeps_authorized_mutations_pending() {
        let runtime = LocalBridgeRuntimeState::default();
        let context = LocalBridgeRuntimeContext {
            trusted_devices: Arc::new(Mutex::new(Vec::new())),
            transfer_status: Arc::new(Mutex::new(None)),
            runtime: Arc::new(runtime),
        };
        context.runtime.authorizations.lock().unwrap().push(
            crate::app_state::LocalBridgeAuthorizationRecord {
                client_id: "local-app".to_string(),
                display_name: "Local App".to_string(),
                app_kind: Some("generic".to_string()),
                scopes: vec![nekolink_protocol::LocalBridgePermissionScope::BundleImportRequest],
                granted_at_ms: 1_000,
                expires_at_ms: None,
            },
        );
        let request = serde_json::json!({
            "kind": "bundle.import",
            "payload": {
                "request_id": "bridge-request-import",
                "client": {
                    "client_id": "local-app",
                    "display_name": "Local App",
                    "app_kind": "generic"
                },
                "staged_bundle_id": "bundle_1234567890",
                "expected_bundle_type": "skill"
            }
        })
        .to_string();

        let response = commands::handle_local_bridge_request_for_runtime(
            &context.trusted_devices,
            &context.transfer_status,
            &context.runtime,
            &request,
        )
        .unwrap();

        assert_eq!(response.status, "pending_runtime");
        assert_eq!(response.security_state, "authorized");
        assert!(response.message.contains("not connected yet"));
    }

    #[test]
    fn local_bridge_runtime_status_reports_loopback_boundary() {
        let runtime = LocalBridgeRuntimeState::default();
        set_local_bridge_runtime_status(&runtime, true, LOCAL_BRIDGE_DEFAULT_PORT, None);
        runtime.pending_authorization.lock().unwrap().replace(
            crate::app_state::PendingLocalBridgeAuthorization {
                request_id: "bridge-auth".to_string(),
                client: nekolink_protocol::LocalBridgeClientIdentity {
                    client_id: "local-app".to_string(),
                    display_name: "Local App".to_string(),
                    app_kind: Some("generic".to_string()),
                },
                requested_scopes: vec![nekolink_protocol::LocalBridgePermissionScope::BundleSend],
                reason: "Send a local bundle".to_string(),
                authorization_code: "ABC-123".to_string(),
                requested_at_ms: 1_000,
                expires_at_ms: 2_000,
            },
        );

        let status = local_bridge_runtime_status(&runtime);

        assert!(status.active);
        assert_eq!(status.bind_host, "127.0.0.1");
        assert_eq!(status.request_path, "/bridge/request");
        assert_eq!(status.max_request_bytes, LOCAL_BRIDGE_MAX_REQUEST_BYTES);
        assert_eq!(
            status.pending_authorization_client.as_deref(),
            Some("Local App")
        );
    }
}
