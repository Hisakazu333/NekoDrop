# NekoLink 协议层

NekoLink 是个人设备可信通信协议。

它不是 NekoDrop 的内部工具类，也不是 TCP/IP 替代品。它是应用层协议，负责把 Mac、Windows、手机、平板、NAS、小主机和 OpenNeko Agent 节点连接成一个可信设备网络。

## 1. 定位

```text
NekoLink
  = 设备身份
  + 可信配对
  + 加密会话
  + 消息信封
  + 能力协商
  + 多传输适配
  + Agent / 文件 / 状态消息通道
```

NekoLink 要能跑在不同 transport 上：

```text
TCP
LocalSend-style LAN transport
iroh QUIC transport
Relay transport
P2P transport
WebSocket transport
Bluetooth / USB transport
```

上层产品不应该关心底层是 TCP 还是 iroh。

## 2. 当前状态

已接入：

- `crates/nekolink-protocol`
- Envelope
- message kind
- capability
- `DeviceIdentity`
- device kind
- platform kind
- connection code 携带 device identity
- `DeviceHello`
- pairing request / accept / reject payload
- file offer / accept / decline payload
- `NekoLinkTransport` 抽象
- `TransportKind`
- `Endpoint`
- `TcpTransport`
- iroh / QUIC / Relay transport 的明确未接入错误

待接入：

- `nekolink-identity`
- `nekolink-pairing`
- `nekolink-session`
- `nekolink-transport`
- iroh runtime transport
- encrypted session
- relay fallback

## 3. 建议 crate 拆分

### nekolink-protocol

职责：

- Envelope
- message kind
- capability
- schema validation
- protocol version
- payload model

不负责：

- 设备密钥生成
- 文件扫描
- TCP 连接
- Tauri 命令
- UI 展示

### nekolink-identity

职责：

- stable device_id
- public fingerprint
- device kind
- platform kind
- local key material
- key version
- identity persistence

不负责：

- trusted pairing
- 文件传输
- discovery

### nekolink-pairing

职责：

- pairing request
- pairing response
- short code
- fingerprint confirm
- trusted device store
- revoke trust
- blocked device

不负责：

- 文件发送
- 局域网扫描
- Agent 执行

### nekolink-session

职责：

- session handshake
- encrypted session key
- message authentication
- replay protection
- heartbeat
- session close

不负责：

- 路由选择
- UI 状态展示
- 文件落盘

### nekolink-transport

职责：

- 抽象 transport
- TCP transport
- iroh transport
- relay transport
- stream open / close
- datagram
- reconnect

不负责：

- Envelope schema
- 文件业务
- 用户交互

## 4. Transport 抽象

当前已经落地的最小接口：

```text
TransportKind
Endpoint
NekoLinkTransport::connect(endpoint)
TcpTransport
connect_endpoint(endpoint)
```

真实可用：

```text
tcp
```

已预留但会返回明确错误：

```text
iroh
quic
relay
```

后续完整传输层应该扩展成：

```text
NekoLinkTransport
  listen(local_identity)
  discover()
  connect(peer_identity)
  open_stream(kind)
  send_envelope(envelope)
  close_session()
```

当前 TCP connection-code 只是一个 transport：

```text
TcpConnectionCodeTransport
```

后续 iroh 也是一个 transport：

```text
IrohTransport
```

上层 NekoDrop 只依赖：

```text
NekoLinkTransport
```

不能依赖具体 TCP 或 iroh 细节。

## 5. iroh 切换路径

不要一次性把 TCP 删除。

正确步骤：

```text
Step 1
  把当前 TCP 文件传输包进 NekoLinkTransport。

Step 2
  所有 file.offer / file.accept / file.decline 都走 Envelope。

Step 3
  引入 iroh key。

Step 4
  device_id 绑定 iroh public key。

Step 5
  iroh stream 上先跑当前文件帧。

Step 6
  Relay / P2P 稳定后，iroh 成为默认 transport。

Step 7
  TCP connection code 降级为 fallback。
```

## 6. NekoLink 消息分类

### device

```text
device.hello
device.heartbeat
device.capabilities
device.offline
```

用于设备发现、在线状态和能力协商。

### pairing

```text
pairing.request
pairing.confirm
pairing.reject
pairing.revoke
```

用于可信设备关系。

### file

```text
file.offer
file.accept
file.decline
file.header
file.chunk
file.complete
transfer.complete
```

用于 NekoDrop 文件互传。

### agent

```text
agent.command
agent.progress
agent.result
agent.cancel
```

用于 OpenNeko Agent 跨设备任务。

### state

```text
state.update
state.patch
state.snapshot
state.ack
state.sync
```

用于 NekoState。

### error

```text
error
session.error
permission.denied
capability.unsupported
```

用于错误表达。

## 7. Capability 设计

能力必须显式协商，不能默认对方支持。

示例：

```text
file_transfer
file_send
file_receive
file_sha256
file_resume
device_pairing
encrypted_session
local_discovery
iroh_transport
relay_transport
agent_command
companion_state
state_sync
```

发送前必须判断：

```text
对方是否支持 file_receive
对方是否支持当前 transport
对方是否可信
对方是否需要接收确认
```

## 8. 与 NekoDrop 的关系

NekoDrop 只能调用 NekoLink：

```text
create_file_offer()
send_file_offer()
open_transfer_stream()
send_file_chunks()
receive_file_chunks()
close_transfer()
```

NekoDrop 不应该定义：

- device_id 格式
- pairing 消息格式
- session key 格式
- Agent 消息格式
- NekoState 消息格式

这些属于 NekoLink。

## 9. 与 OpenNeko 的关系

OpenNeko 不应该直接调用 NekoDrop 的文件传输内部函数。

OpenNeko 应该调用 NekoLink：

```text
send_agent_command()
subscribe_agent_result()
send_file_offer()
subscribe_state()
```

如果 Agent 需要发文件，再由 NekoLink 路由到 NekoDrop 的 file capability。

## 10. 开源边界

NekoLink 最终应该开源。

建议：

```text
MIT OR Apache-2.0
```

必须包含：

- protocol docs
- schema examples
- compatibility tests
- security model
- third-party licenses
- contribution guide

不要在 NekoLink 开源仓库放：

- OpenNeko 私有角色资产
- 官方 relay 密钥
- 商业服务配置
- 未公开 Agent prompt

## 11. 判断标准

NekoLink 成熟的标准：

- NekoDrop 桌面端真实使用
- 手机端真实使用
- OpenNeko Agent 能发起跨设备任务
- 同一套 Envelope 能承载 file / agent / state
- TCP 和 iroh 至少两个 transport 可切换
- 旧版本能和新版本能力协商
- 所有未支持能力都能明确返回 unsupported
