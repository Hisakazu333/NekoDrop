# NekoDrop 当前状态

这份文档记录当前仓库的真实能力。以后 README、路线图和 UI 文案都应该以这里为准，避免把已完成、实验中、待接入混在一起。

更新时间：2026-06-14

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
| 接收端口诊断 | 已接入 | 前端会显示当前收件监听状态、实际端口、可广播的局域网地址；收件关闭时会提示打开收件或复制连接码，没有局域网地址或监听地址异常时会给出克制警告和下一步建议。 |
| 文件选择 | 已接入 | 通过桌面能力选择文件；Windows 文件选择脚本会强制使用 UTF-8 输出，避免中文路径在 IPC 前变成 `�`。 |
| 文件夹选择 | 已接入 | 可以创建文件夹传输计划。 |
| Manifest | 已接入 | 发送前生成真实文件清单。 |
| 传输前扫描/准备状态 | 已接入 | 大文件或文件夹生成 manifest 和 SHA-256 时会显示真实文件数、累计大小和当前路径，避免准备阶段像卡住。 |
| SHA-256 校验 | 已接入 | 接收完成后校验文件完整性。 |
| Transfer offer | 已接入 | 发送前需要接收端接受或拒绝；控制 JSON 帧上限已经按大目录场景提高，数千文件的 offer 不会在传输前被 256KB 旧上限拦住，同时仍保留超大控制帧拒绝。 |
| 接收请求预览 | 已接入 | 接收确认会展示真实 offer 中的来源设备、文件数量、总大小和前几个文件名；桌面 UI 只接收预览文件列表，避免大目录把几千条明细反复序列化到前端。 |
| 接收来源入历史 | 已接入 | 接收完成后，历史记录会写入真实 offer 中的发送方 device_id 和设备名。 |
| 传输进度 / 速度 / ETA | 已接入 | 来自真实传输状态。 |
| 发送中取消 | 已接入 | 当前发送任务可以取消。 |
| 接收中取消 | 已接入 | 正在接收文件时可以发出取消信号，清理当前 partial 文件并关闭收件。 |
| 接收端磁盘空间预检 | 已接入 | 接收端会在接受传输前按 resume 状态估算剩余写入字节；空间不足时提前拒绝，不进入大文件 payload 传输。 |
| 发送端瞬时网络失败自动重试 | 已接入 | 连接拒绝、连接重置、超时等短暂网络错误会自动重试 1 次；用户取消、对方拒绝、校验失败、权限和路径错误不会自动重试。 |
| TCP partial offset 断点续传基础 | 已接入 | 接收端可以基于 `.nekodrop-part` 生成 resume files，发送端按 offset 只补传剩余 payload，接收端追加后做 SHA-256 校验。 |
| 接收端 resume 明细 UI | 已接入 | 接收确认卡片会在存在可续传内容时显示可继续文件数、可跳过已完成文件数和已接收字节数。 |
| 接收策略 | 已接入 | `receive_policy=block_all` 时直接拒绝外部传输；未接入加密会话前，旧配置中的 `auto_accept_trusted` 会按人工确认处理，不再仅凭公开 device_id + fingerprint 静默接收。 |
| 接收目录持久化 | 已接入 | 选择或启动收件时会写入 `app_config.json`，重启后继续使用。 |
| 失败/取消历史进度 | 已接入 | 发送失败或取消时，历史记录会保留最后一次真实已传字节数；未传完的发送记录会显示已传和剩余大小，主操作为“继续发送”，普通失败和不可续传取消显示“重试”，当前失败/取消状态、最近记录和历史详情都会保留下一步提示或备用码兜底入口；重试结果会更新同一条历史记录。 |
| 网络/传输错误提示和地址预检 | 已接入 | 连接阶段有短超时保护；连接拒绝、超时、127.0.0.1、0.0.0.0、169.254.x.x、198.18/198.19、未接入 transport、checksum 等问题会在发送/配对前或失败后转成人能看懂的提示；当前失败状态和历史详情会给一条短的下一步建议。 |
| 传输历史 | 已接入 | 持久化历史，启动时按最新更新时间恢复顺序并按传输 ID 去重，支持打开位置、重发、继续发送、删除、清空；绑定设备 ID 的重发会重新校验可信设备关系。 |
| 连接码兜底 | 已接入 | 自动发现失败时可手动连接；支持完整连接码和 `IP:端口` 直连输入。 |
| 本机目标拒绝 | 已接入 | 连接码或历史记录携带本机 device_id，或手动目标指向任一本机局域网 IP 时，发送入口会拒绝把文件发给自己。 |
| mDNS / DNS-SD 发现 | 已接入 | 同局域网自动发现附近设备。 |
| 发现诊断短提示 | 已接入 | 无设备、未广播、广播异常、发现异常时给出克制的下一步提示；提示会按本机平台聚焦 Windows 专用网络或 macOS 本地网络，并保留备用码作为兜底路径。 |
| 设备离线过期 | 已接入 | 附近设备不会永久假在线。 |
| 设备身份 | 已接入 | 每台设备有稳定 device_id 和 fingerprint。 |
| 可信配对基础 | 已接入 | 配对请求、配对码、接受、拒绝、忘记设备；拒绝、超时、配对码不匹配、设备离线和身份缺失会转成短的下一步提示。 |
| 可信发送校验 | 已接入 | 后端发送到附近设备前会校验 device_id + fingerprint 已在可信设备中，未配对设备不能绕过 UI 直接收文件。 |
| 可信设备地址刷新 | 已接入 | 自动发现扫到可信设备时，会用 device_id + fingerprint 更新可信记录里的 host、port 和 last_seen；收到可信设备真实来件或主动发送成功后也会刷新 last_seen；可信设备列表按最近活跃优先恢复，并按 device_id 去重。 |
| 备用码复制兜底 | 已接入 | 系统剪贴板写入失败时会尝试 DOM fallback，并给出失败提示。 |
| 设备管理页 | 已接入 | 展示附近设备和可信设备；附近设备会区分已信任、未配对和暂不可配对，可信设备会显示在线状态或上次地址兜底发送；选中离线可信设备后，发送页会标明正在使用上次地址。 |
| 设置页 | 已接入 | 独立入口展示并保存本机设备名，展示 fingerprint、收件状态、监听地址、发现广播运行状态、托盘基础状态、接收目录、默认端口和接收策略；接收目录可选择或手动保存，默认端口可保存并用于下次打开收件，收件开启时锁定目录和端口，接收策略和收件开关来自现有真实能力。 |
| 桌面状态刷新 | 已接入 | 实时接收/传输状态保持快速刷新；设备列表、可信设备和传输历史改为慢刷新并避免重叠轮询，降低 macOS 和 Windows 启动后持续卡顿。 |
| macOS DMG | 已接入 | `scripts/package-desktop.sh --dmg`。 |
| Windows NSIS / MSI | 已接入 | `scripts/package-windows.ps1`。 |

## 协议与传输

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| `nekolink-protocol` | 已接入 | 独立 crate，不依赖 Tauri / React。 |
| Envelope | 已接入 | 包含 protocol、version、session_id、message_id、kind、capabilities、payload。 |
| Capability | 已接入 | 文件、配对、加密、Agent、状态同步等能力枚举。 |
| Device identity model | 已接入 | desktop / phone / tablet / OpenHarmony / NAS / Agent 等设备类型。 |
| Device hello | 已接入 | 用于设备发现和能力说明；桌面端只声明当前已实现的文件传输、SHA-256 和配对能力，不声明未完成的加密文件流 / Agent host。 |
| Encrypted session control | 部分接入 | 桌面 TCP 传输主线已经建立 `session.hello` / `session.ready`，基于 X25519 和 HKDF-SHA256 派生会话 key，并让 `file.offer` / `file.accept` / `file.decline` 走 encrypted `session.control`；接收端会校验 session identity 和实际发送方身份一致。文件 payload 仍是明文 TCP 流；replay window、长期身份密钥认证和加密文件流还没接入。 |
| Pairing message | 已接入 | request / accept / reject 基础消息。 |
| File offer / decision | 已接入 | file.offer / file.accept / file.decline；桌面端发送 offer 会携带发送方 device_id、设备名和 fingerprint；协议校验会拒绝空 root_name、不安全 manifest_path、Windows 不安全路径片段和半截 sender identity。 |
| TCP transport | 已接入 | 当前真实传输主线；接收文件帧数量有上限，并会按已接受 offer 的 file_count 做早期校验。 |
| Transport 抽象 | 已接入 | `NekoLinkTransport`、`Endpoint`、`TransportKind`、`TcpTransport`。 |
| iroh transport | 实验中 | 只有类型预留和明确错误，未接入 iroh runtime。 |
| Relay / P2P transport | 实验中 | 只有类型预留和明确错误。 |
| NekoLink bundle manifest | 部分接入 | [BUNDLE_SPEC.md](BUNDLE_SPEC.md) 已定义包结构、权限、校验和导入边界；`nekolink-protocol` 已有 bundle manifest、checksums、permissions 类型和校验，`nekodrop-storage` 已能识别、校验并保存到 staging，且可以列出、删除、按过期时间清理 staged bundle，`nekodrop-service` 已有接收完成后的 staged bundle report，桌面后端 DTO 已能暴露 staged bundle 元数据，桌面接收完成区会显示紧凑 bundle 摘要、已保存状态和删除暂存入口。local bridge 导入还没有接入。 |
| CCS / OpenNeko local bridge 协议模型 | 部分接入 | `nekolink-protocol` 已定义 `LocalBridgeRequest` / `LocalBridgeEvent` 的 JSON 模型，覆盖查询设备、查询 staged bundle 详情、发送 bundle、收到 bundle 通知、请求导入和查询传输状态；桌面端已经有内部 local bridge handler skeleton，可以把只读请求映射到可信设备、staged bundle 列表/详情和 transfer status，并区分 `read_only` / `requires_user_confirmation`。localhost runtime、持久化授权、权限 scope 和导入执行还没有接入。 |

## 当前不能宣传为已完成

- iroh 真实运行时
- Relay 服务器
- P2P / NAT 打洞
- 加密文件流
- 手机端互传主流程
- OpenNeko Agent 指令通道
- NekoLink bundle 发送和导入主流程
- CCS / OpenNeko local bridge runtime
- NekoState 状态同步
- 系统级 Windows 防火墙自动配置
- 云账号 / 云盘 / 中心化文件存储

## 下一阶段重点

```text
V0.6
  桌面 LAN 互传已经进入 beta 收口：
  大目录 offer、进度、历史、续传、失败恢复、设备发现和性能轮询都已接入；后续只继续补实机验证、发布记录和少量阻塞级体验问题。

V0.7
  加密 session 接入桌面主线：
  file.offer / file.accept / file.decline 已经走 encrypted session.control；下一步做 replay 保护、长期身份密钥认证和文件流加密。

V0.8
  NekoLink 上层包格式：
  定义 bundle manifest，为 skills、session、agent profile、workspace 这类上层数据传输提供统一校验、权限和兼容边界。

V0.9
  transport 技术验证：
  TCP 保持默认稳定路径，iroh / Relay / P2P 作为实验 transport 接入 NekoLink，不直接替换现有桌面互传主线。
```

## 文档维护规则

- 新增真实功能后，先更新本文件。
- UI 出现的功能必须能在本文件找到状态。
- README 只能引用本文件中的真实能力。
- Roadmap 只能把未完成能力写成未来计划。
- 占位设备、占位历史、占位进度、占位配对和占位扫描结果不能描述为已实现能力。
