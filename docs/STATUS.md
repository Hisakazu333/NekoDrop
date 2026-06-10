# NekoDrop 当前状态

这份文档记录当前仓库的真实能力。以后 README、路线图和 UI 文案都应该以这里为准，避免把已完成、实验中、待接入混在一起。

更新时间：2026-06-11

## 状态定义

```text
已接入
  代码存在，当前桌面端可以真实运行。

实验中
  有基础代码或接口预留，但还不能作为主流程承诺给用户。

待接入
  只允许出现在 roadmap、文档或明确标注的 UI 预告里。

不做
  当前阶段明确不进入范围。
```

## 产品能力

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| macOS / Windows 桌面端 | 已接入 | Tauri 桌面工程存在，目标是安装包运行，不是浏览器页面。 |
| 后台接收服务 | 已接入 | 应用启动后自动开启接收服务。 |
| 收件端口控制 | 已接入 | 前端会校验 1-65535；如果请求端口被占用，后端自动换端口后会回填实际监听端口。 |
| 文件选择 | 已接入 | 通过桌面能力选择文件。 |
| 文件夹选择 | 已接入 | 可以创建文件夹传输计划。 |
| Manifest | 已接入 | 发送前生成真实文件清单。 |
| SHA-256 校验 | 已接入 | 接收完成后校验文件完整性。 |
| Transfer offer | 已接入 | 发送前需要接收端接受或拒绝。 |
| 接收请求预览 | 已接入 | 接收确认会展示真实 offer 中的来源设备、文件数量、总大小和前几个文件名。 |
| 接收来源入历史 | 已接入 | 接收完成后，历史记录会写入真实 offer 中的发送方 device_id 和设备名。 |
| 传输进度 / 速度 / ETA | 已接入 | 来自真实传输状态。 |
| 发送中取消 | 已接入 | 当前发送任务可以取消。 |
| 接收策略 | 已接入 | `receive_policy=block_all` 时直接拒绝外部传输；`auto_accept_trusted` 只会自动接受 device_id + fingerprint 匹配的可信设备。 |
| 接收目录持久化 | 已接入 | 选择或启动收件时会写入 `app_config.json`，重启后继续使用。 |
| 失败/取消历史进度 | 已接入 | 发送失败或取消时，历史记录会保留最后一次真实已传字节数。 |
| 网络/传输错误提示和地址预检 | 已接入 | 连接阶段有短超时保护；连接拒绝、超时、127.0.0.1、0.0.0.0、169.254.x.x、198.18/198.19、未接入 transport、checksum 等问题会在发送/配对前或失败后转成人能看懂的提示。 |
| 传输历史 | 已接入 | 持久化历史，启动时按最新更新时间恢复顺序，支持打开位置、重发、删除、清空。 |
| 连接码兜底 | 已接入 | 自动发现失败时可手动连接；支持完整连接码和 `IP:端口` 直连输入。 |
| 本机目标拒绝 | 已接入 | 连接码或历史记录携带本机 device_id，或手动目标指向本机当前局域网 IP 时，发送入口会拒绝把文件发给自己。 |
| mDNS / DNS-SD 发现 | 已接入 | 同局域网自动发现附近设备。 |
| 发现诊断短提示 | 已接入 | 无设备、未广播、发现异常时给出克制的下一步提示。 |
| 设备离线过期 | 已接入 | 附近设备不会永久假在线。 |
| 设备身份 | 已接入 | 每台设备有稳定 device_id 和 fingerprint。 |
| 可信配对基础 | 已接入 | 配对请求、配对码、接受、拒绝、忘记设备。 |
| 可信发送校验 | 已接入 | 后端发送到附近设备前会校验 device_id + fingerprint 已在可信设备中，未配对设备不能绕过 UI 直接收文件。 |
| 可信设备地址刷新 | 已接入 | 自动发现扫到可信设备时，会用 device_id + fingerprint 更新可信记录里的 host、port 和 last_seen；收到可信设备真实来件或主动发送成功后也会刷新 last_seen；可信设备列表按最近活跃优先恢复。 |
| 备用码复制兜底 | 已接入 | 系统剪贴板写入失败时会尝试 DOM fallback，并给出失败提示。 |
| 设备管理页 | 已接入 | 展示附近设备和可信设备。 |
| macOS DMG | 已接入 | `scripts/package-desktop.sh --dmg`。 |
| Windows NSIS / MSI | 已接入 | `scripts/package-windows.ps1`。 |

## 协议与传输

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| `nekolink-protocol` | 已接入 | 独立 crate，不依赖 Tauri / React。 |
| Envelope | 已接入 | 包含 protocol、version、session_id、message_id、kind、capabilities、payload。 |
| Capability | 已接入 | 文件、配对、加密、Agent、状态同步等能力枚举。 |
| Device identity model | 已接入 | desktop / phone / tablet / OpenHarmony / NAS / Agent 等设备类型。 |
| Device hello | 已接入 | 用于设备发现和能力说明。 |
| Pairing message | 已接入 | request / accept / reject 基础消息。 |
| File offer / decision | 已接入 | file.offer / file.accept / file.decline；桌面端发送 offer 会携带发送方 device_id、设备名和 fingerprint。 |
| TCP transport | 已接入 | 当前真实传输主线。 |
| Transport 抽象 | 已接入 | `NekoLinkTransport`、`Endpoint`、`TransportKind`、`TcpTransport`。 |
| iroh transport | 实验中 | 只有类型预留和明确错误，未接入 iroh runtime。 |
| Relay / P2P transport | 实验中 | 只有类型预留和明确错误。 |

## 当前不能宣传为已完成

- iroh 真实运行时
- Relay 服务器
- P2P / NAT 打洞
- 加密 session
- 断点续传完整产品流程
- 手机端互传主流程
- OpenNeko Agent 指令通道
- NekoState 状态同步
- 系统级 Windows 防火墙自动配置
- 云账号 / 云盘 / 中心化文件存储

## 下一阶段重点

```text
V0.6
  继续打磨桌面互传体验：
  自动发现稳定性、Windows 防火墙提示、失败错误恢复、发送/接收交互、历史和取消体验。

V0.7
  把 NekoLink transport 抽象用实：
  TCP 主线保持稳定，iroh / Relay / P2P 做技术验证，但不影响现有互传。

V0.8
  加密 session：
  基于设备身份和可信配对建立会话密钥，让文件 offer 和传输消息进入可信加密通道。
```

## 文档维护规则

- 新增真实功能后，先更新本文件。
- UI 出现的功能必须能在本文件找到状态。
- README 只能引用本文件中的真实能力。
- Roadmap 只能把未完成能力写成未来计划。
- 不允许写假设备、假历史、假进度、假配对、假扫描。
