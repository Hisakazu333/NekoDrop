# NekoDrop 当前状态

这份文档记录当前仓库的真实能力。以后 README、路线图和 UI 文案都应该以这里为准，避免把已完成、实验中、待接入混在一起。

更新时间：2026-06-23

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
| 接收策略 | 已接入 | `receive_policy=block_all` 时直接拒绝外部传输；`auto_accept_trusted` 只会在 authenticated session 已验签且长期 public key 命中可信设备记录时自动接收。旧明文只能走手动兼容；如果明文请求声明了已配对设备的 device_id，会在弹出确认前拒绝。 |
| 接收目录持久化 | 已接入 | 选择或启动收件时会写入 `app_config.json`，重启后继续使用。 |
| 失败/取消历史进度 | 已接入 | 发送失败或取消时，历史记录会保留最后一次真实已传字节数；未传完的发送记录会显示已传和剩余大小，主操作为“继续发送”，普通失败和不可续传取消显示“重试”，当前失败/取消状态、最近记录和历史详情都会保留下一步提示或备用码兜底入口；重试结果会更新同一条历史记录。 |
| 网络/传输错误提示和地址预检 | 已接入 | 连接阶段有短超时保护；连接拒绝、超时、127.0.0.1、0.0.0.0、169.254.x.x、198.18/198.19、未接入 transport、checksum 等问题会在发送/配对前或失败后转成人能看懂的提示；当前失败状态和历史详情会给一条短的下一步建议。 |
| 传输历史 | 已接入 | 持久化历史，启动时按最新更新时间恢复顺序并按传输 ID 去重，支持打开位置、重发、继续发送、删除、清空；绑定设备 ID 的重发会重新校验可信设备关系。 |
| 传输安全状态 UI | 已接入 | 最近完成、收件结果和传输历史会显示真实 `security_mode`：`authenticated_encrypted_session` 显示“已认证加密”，`encrypted_session` 显示“已加密”，`legacy_plain` 显示“兼容明文”。旧历史没有该字段时不补假状态。 |
| 连接码兜底 | 已接入 | 自动发现失败时可手动连接；支持完整连接码和 `IP:端口` 直连输入。 |
| 本机目标拒绝 | 已接入 | 连接码或历史记录携带本机 device_id，或手动目标指向任一本机局域网 IP 时，发送入口会拒绝把文件发给自己。 |
| mDNS / DNS-SD 发现 | 已接入 | 同局域网自动发现附近设备。 |
| 发现诊断短提示 | 已接入 | 无设备、未广播、广播异常、发现异常时给出克制的下一步提示；提示会按本机平台聚焦 Windows 专用网络或 macOS 本地网络，并保留备用码作为兜底路径。 |
| 设备离线过期 | 已接入 | 附近设备不会永久假在线。 |
| 设备身份 | 已接入 | 每台设备有稳定 device_id、Ed25519 public key 和 fingerprint。 |
| 可信配对基础 | 已接入 | 配对请求、配对码、接受、拒绝、忘记设备；发现广播和配对请求会带长期 public key，可信记录会保存 public key + fingerprint，并拒绝二者不匹配的记录；拒绝、超时、配对码不匹配、设备离线和身份缺失会转成短的下一步提示。 |
| 可信发送校验 | 已接入 | 后端发送到附近设备前会校验 device_id + public key + fingerprint 已在可信设备中，未配对设备不能绕过 UI 直接收文件。 |
| 可信设备地址刷新 | 已接入 | 自动发现扫到可信设备时，会用 device_id + public key + fingerprint 更新可信记录里的 host、port 和 last_seen；收到可信设备真实来件或主动发送成功后也会刷新 last_seen；可信设备列表按最近活跃优先恢复，并按 device_id 去重。 |
| 备用码复制兜底 | 已接入 | 系统剪贴板写入失败时会尝试 DOM fallback，并给出失败提示。 |
| 设备管理页 | 已接入 | 展示附近设备和可信设备；附近设备会区分已信任、未配对和暂不可配对，可信设备会显示在线状态或上次地址兜底发送；无附近设备时显示扫描中、未广播或发现异常，不再只显示 `0 附近在线`；选中离线可信设备后，发送页会标明正在使用上次地址。 |
| 设置页 | 已接入 | 独立入口展示并保存本机设备名，展示 fingerprint、收件状态、监听地址、发现广播运行状态、托盘基础状态、接收目录、默认端口和接收策略；接收目录可选择或手动保存，默认端口可保存并用于下次打开收件，收件开启时锁定目录和端口，接收策略和收件开关来自现有真实能力；本机接入状态收在设置页，并提供内部只读自测和授权码确认，不作为日常主导航入口。 |
| 桌面状态刷新 | 已接入 | 实时收件、待确认、配对、传输和发现状态通过一个桌面 snapshot 刷新；设备列表、可信设备和传输历史改为按页面需要慢刷新并避免重叠轮询；相同状态不会重复写入 React state，降低 macOS 和 Windows 启动后持续卡顿。 |
| macOS DMG | 已接入 | `scripts/package-desktop.sh --dmg`。 |
| Windows NSIS / MSI | 已接入 | `scripts/package-windows.ps1`。 |

## 协议与传输

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| `nekolink-protocol` | 已接入 | 独立 crate，不依赖 Tauri / React。 |
| Envelope | 已接入 | 包含 protocol、version、session_id、message_id、kind、capabilities、payload。 |
| Capability | 已接入 | 文件、配对、加密、Agent、状态同步等能力枚举。 |
| Device identity model | 已接入 | desktop / phone / tablet / OpenHarmony / NAS / Agent 等设备类型。 |
| Device hello | 已接入 | 用于设备发现和能力说明；桌面端只声明当前已实现的文件传输、SHA-256、配对和加密 session 能力，不声明未完成的 Agent host。 |
| Encrypted session control | 部分接入 | 桌面 TCP 传输主线已经建立 `session.hello` / `session.ready`，基于 X25519 和 HKDF-SHA256 派生会话 key，并让 `file.offer` / `file.accept` / `file.decline` 走 encrypted `session.control`；接收端会校验 session identity 和实际发送方身份一致，offer / decision 控制消息读取路径已接入 replay window。桌面真实发送/接收路径已交换并验签 `session.identity`，签名不匹配会拒绝 session；如果对方已经在可信设备记录里，authenticated session 还会钉到记录里保存的长期 public key。旧明文路径现在标记为 `legacy_plain`，只能手动确认；明文请求只要声明了已配对设备的 device_id 就会拒绝，不会自动接受，也不会刷新可信设备状态。 |
| Session identity binding 签名模型 | 已接入 | `nekolink-protocol` 已能从 verified handshake 生成 initiator / responder identity binding，并提供稳定 canonical payload hash，把 session_id、设备 ID、fingerprint、session ephemeral key 和 handshake_hash 绑定到签名材料；协议层已有 Ed25519 `SignedSessionIdentityBinding`，桌面 `device_identity.json` schema v2 会持久化本机签名 seed 并迁移旧 schema v1。`session.identity` 会在 `session.ready` 后、encrypted control 前交换，双方验签通过后才继续传输。 |
| Encrypted file stream | 部分接入 | encrypted session 发送/接收路径已经把文件 payload 切成加密 file frames，frame AAD 绑定 transfer_id、manifest_path、offset 和 plain_size；接收端会按 reader 读取逐帧解密，不再先把单文件 payload 全部解密进内存；旧明文文件流路径仍保留给 plain offer 兼容，但已经从可信设备状态更新和自动接收路径隔离。 |
| Pairing message | 已接入 | request / accept / reject 基础消息。 |
| File offer / decision | 已接入 | file.offer / file.accept / file.decline；桌面端发送 offer 会携带发送方 device_id、设备名和 fingerprint；协议校验会拒绝空 root_name、不安全 manifest_path、Windows 不安全路径片段和半截 sender identity。 |
| TCP transport | 已接入 | 当前真实传输主线；接收文件帧数量有上限，并会按已接受 offer 的 file_count 做早期校验。 |
| Transport 抽象 | 已接入 | `NekoLinkTransport`、`Endpoint`、`TransportKind`、`TcpTransport`。 |
| iroh transport | 实验中 | 只有类型预留和明确错误，未接入 iroh runtime。 |
| Relay / P2P transport | 实验中 | 只有类型预留和明确错误。 |
| NekoLink bundle manifest | 部分接入 | [BUNDLE_SPEC.md](BUNDLE_SPEC.md) 已定义包结构、权限、校验和导入边界；`nekolink-protocol` 已有 bundle manifest、checksums、permissions 类型和校验，`nekodrop-storage` 已能识别、校验、保存到 staging，也能把用户选择的目录打成 v1 bundle；`nekodrop-service` 已有接收完成后的 staged bundle report。桌面端的资料包创建入口已收进发送页，收到的 staged bundle 在收件流程里查看、删除和手动导入到本机导入区；导入计划会暴露将写入的文件数和冲突文件，默认拒绝覆盖，也支持重命名导入和跳过冲突文件；导入使用临时目录落盘，失败不留下半成品目标目录；成功后会写入 import receipt，记录目标目录、策略、实际导入和跳过的 payload 路径。现在可以基于 receipt 执行保守撤回：只删除本次导入记录里的文件，跳过冲突策略保留下来的既有文件，不写入第三方应用目录。桌面端会清理过期暂存，删除、导入失败、已导入、可撤回和已撤回状态会留在收件流程里。`bundle.send` 的本机 local bridge 执行入口现在可以消费待执行动作并交给桌面发送主线；`bundle.import` 可以消费待执行动作并把 staged bundle 导入本机导入区，结果会记录所用冲突策略、跳过文件数、receipt 和可撤回文件数；`bundle.rollback` 可以消费待执行动作并撤回 NekoDrop 本机导入区里的本次导入文件。`skill`、`session`、`workspace`、`agent_profile` 只允许 authenticated encrypted session 进入 staging / import；legacy plain 和非认证 encrypted session 收到这些 bundle 形态目录时只按普通文件保存。上层应用自动导出 session / skill / workspace 还没有接入。 |
| Adapter 规范和 bundle 样例 | 部分接入 | [ADAPTER_SPEC.md](ADAPTER_SPEC.md) 已定义上层应用导出/导入 bundle 的边界；[bundle-samples](bundle-samples/) 提供 `skill`、`session`、`workspace`、`agent_profile`、`config_snapshot` 五类可校验样例；[generic-adapter](examples/generic-adapter/) 能生成导出、授权、发送、事件观察、cursor 恢复、详情、带冲突策略的导入、按动作 `request_id` 查询结果、回滚请求和回滚结果查询的通用请求序列。真实上层应用 adapter 还没有接入。 |
| 本机 local bridge 协议模型 | 部分接入 | `nekolink-protocol` 已定义 `LocalBridgeRequest` / `LocalBridgeEvent` 的 JSON 模型，覆盖查询设备、申请本机授权、查询 staged bundle 详情、发送 bundle、收到 bundle 通知、请求导入、请求撤回导入、查询传输状态、查询动作结果和 `events.poll` 事件轮询；请求可以带本机 `client` 标识，授权申请已有通用 scope：`device.read`、`transfer.status.read`、`bundle.read`、`bundle.send`、`bundle.import.request`。桌面端内部 handler 可以把只读请求映射到可信设备、staged bundle 列表/详情、导入 receipt 详情和 transfer status，并区分 `read_only` / `requires_user_confirmation`、`anonymous` / `identified`；设置页可以触发一次内部 `devices.list` 只读自测，并显示 localhost runtime 的真实监听状态、地址、待确认授权、已授权数量、待执行动作数量、待执行细节和最近结果失败原因。桌面端启动时会开启只绑定 `127.0.0.1` 的 localhost runtime，只接受 `POST /bridge/request`，请求体有大小上限；只读请求和授权申请走同一套 handler。用户确认授权码后，runtime 会记录该 client 的限时权限并写入本机授权文件；下次启动会恢复未过期授权。已授权 client 调用 `bundle.send` / `bundle.import` / `bundle.rollback` 时，runtime 会把请求写入内存待执行队列，后台 worker 会自动消费；设置页可以查看概要并移除这些待执行动作；`bundle.send` 会先做 preflight，敏感 bundle 不能关闭可信目标要求，再复用现有 authenticated send 主线发送到目标设备；`bundle.import` 可以按 FIFO 消费待执行动作，并把 staged bundle 导入本机导入区，支持 `reject`、`rename`、`skip_conflicts`；`bundle.rollback` 只撤回 NekoDrop 本机导入区的 receipt 文件清单。动作生命周期会写入 `queued`、`running`、`succeeded`、`failed`、`conflict`、`cancelled`，授权 client 可通过 `events.poll` 的 `action.updated` 持续观察，也可用 `actions.results` 补偿查询自己的最新动作结果；传 `action_request_id` 时只查指定动作，不传时按时间游标返回最近结果。普通列表、事件和结果都不暴露本机 `bundle_root`。runtime 现在有内存事件队列，真实发送/接收主流程会写入 `transfer.updated`，收到 staged bundle 会写入 `bundle.received`；已授权 client 可用 `events.poll` 轮询快照或短等待新事件，cursor 丢失时会返回 `missing` 让调用方从快照重拉。 |

## 当前不能宣传为已完成

- iroh 真实运行时
- Relay 服务器
- P2P / NAT 打洞
- key rotation / OS keychain 级别的长期密钥管理
- 手机端互传主流程
- OpenNeko Agent 指令通道
- 上层应用自动导出和直接写入 NekoLink bundle
- 本机 local bridge 真正长连接事件流订阅接口
- NekoState 状态同步
- 系统级 Windows 防火墙自动配置
- 云账号 / 云盘 / 中心化文件存储

## 下一阶段重点

```text
V0.6.x
  工程结构治理：
  当前分层还能继续演进，但桌面 commands、App.tsx、全局 CSS、协议 lib.rs、TCP file 和 service lib 已经是热点文件。后续功能迭代要边做边拆：新命令族进 commands/<area>.rs，新页面状态进 views/<View>.tsx，新协议域从 nekolink-protocol/src/lib.rs 拆出独立模块。目标不是重写，而是避免主线变成难维护的大文件堆。

V0.6
  桌面 LAN 互传已经进入 beta 收口：
  大目录 offer、进度、历史、续传、失败恢复、设备发现和性能轮询都已接入；后续只继续补实机验证、发布记录和少量阻塞级体验问题。

V0.7
  加密 session 接入桌面主线：
  file.offer / file.accept / file.decline 已经走 encrypted session.control，offer / decision 控制消息读取路径已接入 replay window；encrypted session 路径的文件 payload 已经进入加密 file frames，接收端已改为逐帧 streaming 解密。session.identity 签名校验、可信设备 public key pinning、legacy plain 隔离和 authenticated trusted auto-accept 策略已接入。

V0.8
  NekoLink 上层包格式：
  定义 bundle manifest，为 skills、session、agent profile、workspace 这类上层数据传输提供统一校验、权限和兼容边界；桌面端已有 staging、预览、导入计划、冲突文件提示、删除、过期清理、重命名导入、跳过冲突、import receipt、回滚计划、保守撤回执行和手动导入到本机导入区；local bridge 已有 localhost runtime、授权、待执行队列、动作结果、事件轮询和 `bundle.rollback`。下一步补真实上层应用适配和导入到第三方应用后的回滚协议。

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
