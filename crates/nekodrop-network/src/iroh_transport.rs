//! iroh 传输：QUIC + NAT 穿透 + 中继，桥接为同步 TcpStream 供既有协议栈复用。
//!
//! 同步桥：iroh 双向流 (SendStream/RecvStream) ↔ 本机回环 TCP 对。
//! 调用方拿到的是真 TcpStream，set_io_timeout 等能力全部原生可用；
//! 桥任务随对端断开自动退出。

use std::net::{TcpListener, TcpStream};
use std::str::FromStr;
use std::sync::OnceLock;
use std::time::Duration;

use iroh::endpoint::presets;
use iroh::Watcher as _;
use iroh::{Endpoint, RelayMode};
use iroh::{EndpointAddr, EndpointId, RelayUrl, TransportAddr};
use nekodrop_core::{NekoDropError, NekoDropResult};

use crate::transport::{
    Endpoint as NekoEndpoint, TransportKind, TransportStream, TCP_IO_STALL_TIMEOUT,
};

pub const NEKODROP_ALPN: &[u8] = b"nekodrop/1";

const IROH_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to build iroh tokio runtime")
    })
}

fn network_error(message: impl std::fmt::Display) -> NekoDropError {
    NekoDropError::Network(format!("iroh: {message}"))
}

/// iroh 接收端：监听并逐个交出同步流。
pub struct IrohServer {
    endpoint: Endpoint,
}

impl IrohServer {
    /// 直连模式：禁用中继（同网段/本机场景，零外部依赖）。
    pub fn bind_direct_only() -> NekoDropResult<Self> {
        let endpoint = runtime()
            .block_on(async {
                Endpoint::builder(presets::Minimal)
                    .alpns(vec![NEKODROP_ALPN.to_vec()])
                    .relay_mode(RelayMode::Disabled)
                    .bind()
                    .await
            })
            .map_err(network_error)?;
        Ok(Self { endpoint })
    }

    /// 公网模式：n0 中继 + pkarr 发现，可跨网络接收。
    pub fn bind_with_public_relays() -> NekoDropResult<Self> {
        let endpoint = runtime()
            .block_on(async {
                Endpoint::builder(presets::N0)
                    .alpns(vec![NEKODROP_ALPN.to_vec()])
                    .bind()
                    .await
            })
            .map_err(network_error)?;
        Ok(Self { endpoint })
    }

    pub fn node_id_hex(&self) -> String {
        self.endpoint.id().to_string()
    }

    /// 当前可直连地址（轮询等地址就绪，最多 3 秒）。
    pub fn direct_addrs(&self) -> Vec<String> {
        let mut watcher = self.endpoint.watch_addr();
        let addr = runtime().block_on(async {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
            loop {
                let addr = watcher.get();
                if !addr.addrs.is_empty() || tokio::time::Instant::now() >= deadline {
                    return addr;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
        addr.addrs
            .iter()
            .filter_map(|transport_addr| match transport_addr {
                TransportAddr::Ip(socket_addr) => Some(socket_addr.to_string()),
                _ => None,
            })
            .collect()
    }

    pub fn relay_url(&self) -> Option<String> {
        self.endpoint.watch_addr().get().addrs.iter().find_map(
            |transport_addr| match transport_addr {
                TransportAddr::Relay(url) => Some(url.to_string()),
                _ => None,
            },
        )
    }

    /// 带轮询的接受：每 poll 窗口检查一次外部取消，None 表示本轮无连接。
    /// 桌面收件线程用它实现干净的停机（阻塞 accept 无法响应取消标志）。
    pub fn accept_transfer_stream_with_deadline(
        &self,
        poll: Duration,
    ) -> NekoDropResult<Option<TcpStream>> {
        let endpoint = self.endpoint.clone();
        runtime().block_on(async move {
            match tokio::time::timeout(poll, endpoint.accept()).await {
                Err(_) => Ok(None),
                Ok(None) => Err(network_error("endpoint closed")),
                Ok(Some(incoming)) => {
                    let connection = incoming
                        .accept()
                        .map_err(network_error)?
                        .await
                        .map_err(network_error)?;
                    let (send_stream, recv_stream) =
                        connection.accept_bi().await.map_err(network_error)?;
                    Ok(Some(bridge_to_sync_stream(send_stream, recv_stream).await?))
                }
            }
        })
    }

    /// 阻塞等待一个 iroh 连接，桥接为同步 TcpStream。
    pub fn accept_transfer_stream(&self) -> NekoDropResult<TcpStream> {
        let endpoint = self.endpoint.clone();
        runtime().block_on(async move {
            let incoming = endpoint
                .accept()
                .await
                .ok_or_else(|| network_error("endpoint closed"))?;
            let connection = incoming
                .accept()
                .map_err(network_error)?
                .await
                .map_err(network_error)?;
            let (send_stream, recv_stream) = connection.accept_bi().await.map_err(network_error)?;
            bridge_to_sync_stream(send_stream, recv_stream).await
        })
    }
}

/// 客户端全局端点：默认启用 n0 中继（局域网仍会优先直连）。
fn client_endpoint() -> NekoDropResult<&'static Endpoint> {
    static CLIENT: OnceLock<Result<Endpoint, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            runtime()
                .block_on(async {
                    Endpoint::builder(presets::N0)
                        .alpns(vec![NEKODROP_ALPN.to_vec()])
                        .bind()
                        .await
                })
                .map_err(|error| format!("{error}"))
        })
        .as_ref()
        .map_err(|message| network_error(format!("client endpoint unavailable: {message}")))
}

/// 连接 iroh 端点（Endpoint.host = 节点 ID hex）。
pub fn iroh_connect(target: &NekoEndpoint) -> NekoDropResult<TcpStream> {
    if target.transport != TransportKind::Iroh {
        return Err(NekoDropError::Network(format!(
            "iroh transport expected, got {}",
            target.transport.as_str()
        )));
    }
    let node_id = EndpointId::from_str(target.host.trim())
        .map_err(|error| network_error(format!("invalid iroh node id: {error}")))?;

    let mut addrs: Vec<TransportAddr> = Vec::new();
    if let Some(relay) = target
        .relay_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let url = RelayUrl::from_str(relay)
            .map_err(|error| network_error(format!("invalid relay url: {error}")))?;
        addrs.push(TransportAddr::Relay(url));
    }
    for addr in &target.direct_addrs {
        let socket_addr = addr
            .trim()
            .parse()
            .map_err(|error| network_error(format!("invalid direct addr {addr}: {error}")))?;
        addrs.push(TransportAddr::Ip(socket_addr));
    }
    let endpoint_addr = EndpointAddr::from_parts(node_id, addrs);

    let endpoint = client_endpoint()?.clone();
    runtime().block_on(async move {
        let connection = tokio::time::timeout(
            IROH_CONNECT_TIMEOUT,
            endpoint.connect(endpoint_addr, NEKODROP_ALPN),
        )
        .await
        .map_err(|_| {
            network_error(format!(
                "connect timed out after {}s",
                IROH_CONNECT_TIMEOUT.as_secs()
            ))
        })?
        .map_err(network_error)?;
        let (send_stream, recv_stream) = connection.open_bi().await.map_err(network_error)?;
        bridge_to_sync_stream(send_stream, recv_stream).await
    })
}

/// 把 iroh 双向流桥成本机 TCP 对，返回给同步调用方的一端。
async fn bridge_to_sync_stream(
    mut send_stream: iroh::endpoint::SendStream,
    mut recv_stream: iroh::endpoint::RecvStream,
) -> NekoDropResult<TcpStream> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| network_error(format!("bridge bind failed: {error}")))?;
    let bridge_addr = listener
        .local_addr()
        .map_err(|error| network_error(format!("bridge addr failed: {error}")))?;

    let bridge_tcp = tokio::net::TcpStream::connect(bridge_addr)
        .await
        .map_err(|error| network_error(format!("bridge connect failed: {error}")))?;
    let (mut sync_side, _) = listener
        .accept()
        .map_err(|error| network_error(format!("bridge accept failed: {error}")))?;
    sync_side
        .set_nodelay(true)
        .map_err(|error| network_error(format!("bridge nodelay failed: {error}")))?;
    let _ = sync_side.set_io_timeout(TCP_IO_STALL_TIMEOUT);

    let (mut tcp_read, mut tcp_write) = bridge_tcp.into_split();
    // 本机 TCP → iroh 发送流；对端关闭后 finish 让远端读到 EOF
    tokio::spawn(async move {
        let _ = tokio::io::copy(&mut tcp_read, &mut send_stream).await;
        let _ = send_stream.finish();
    });
    // iroh 接收流 → 本机 TCP；远端 EOF 后关闭本机写半边
    tokio::spawn(async move {
        let _ = tokio::io::copy(&mut recv_stream, &mut tcp_write).await;
        use tokio::io::AsyncWriteExt;
        let _ = tcp_write.shutdown().await;
    });

    Ok(sync_side)
}

/// 从连接码字段构建 iroh 端点。
pub fn endpoint_from_iroh_fields(
    node: &str,
    relay: Option<String>,
    addrs: Vec<String>,
) -> NekoDropResult<NekoEndpoint> {
    let node = node.trim();
    if node.is_empty() {
        return Err(NekoDropError::InvalidConnectionCode(
            "iroh connection code missing node id".to_string(),
        ));
    }
    EndpointId::from_str(node)
        .map_err(|error| network_error(format!("invalid iroh node id: {error}")))?;
    Ok(NekoEndpoint {
        host: node.to_string(),
        port: 0,
        transport: TransportKind::Iroh,
        relay_url: relay,
        direct_addrs: addrs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection_ticket::ConnectionTicket;

    fn test_endpoint_fields() -> NekoEndpoint {
        NekoEndpoint {
            host: "a".repeat(64),
            port: 0,
            transport: TransportKind::Iroh,
            relay_url: Some("https://use1-1.relay.iroh.network/".to_string()),
            direct_addrs: vec!["192.168.1.9:45821".to_string()],
        }
    }

    #[test]
    fn iroh_ticket_roundtrip() {
        let ticket = ConnectionTicket::new(test_endpoint_fields()).unwrap();
        let code = ticket.to_code().unwrap();
        assert!(code.starts_with("nekodrop-v1;transport=iroh;node="));
        assert!(code.contains("relay="));
        assert!(code.contains("addrs="));
        let parsed = ConnectionTicket::parse(&code).unwrap();
        assert_eq!(parsed.endpoint, ticket.endpoint);
    }

    #[test]
    fn iroh_ticket_rejects_bad_node() {
        let code = "nekodrop-v1;transport=iroh;node=zz";
        assert!(ConnectionTicket::parse(code).is_err());
    }

    #[test]
    fn iroh_direct_loopback_echo() {
        // 纯直连（禁中继）：本机两端 QUIC 打通并桥成 TcpStream 回声。
        // 注意：iroh 端点随最后持有者 drop 而关闭且会吞掉发送队列，
        // 因此 server 必须活到断言之后（scoped thread 借用）。
        let server = IrohServer::bind_direct_only().unwrap();
        let node_id = server.node_id_hex();
        let addrs = server.direct_addrs();
        assert!(
            !addrs.is_empty(),
            "direct addrs should be ready on loopback"
        );

        std::thread::scope(|scope| {
            scope.spawn(|| {
                let mut stream = server.accept_transfer_stream().unwrap();
                use std::io::{Read, Write};
                let mut buf = [0_u8; 5];
                stream.read_exact(&mut buf).unwrap();
                stream.write_all(&buf).unwrap();
            });

            let target = endpoint_from_iroh_fields(&node_id.clone(), None, addrs.clone()).unwrap();
            let mut client = iroh_connect(&target).unwrap();
            use std::io::{Read, Write};
            client.write_all(b"hello").unwrap();
            let mut echo = [0_u8; 5];
            client.read_exact(&mut echo).unwrap();
            assert_eq!(&echo, b"hello");
        });
    }
}
