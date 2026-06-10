# NekoDrop / NekoLink 后续迭代计划

## 1. 方向定义

NekoDrop 不应该只停留在“文件互传软件”。

更合理的产品和技术定位是：

```text
NekoLink  = 个人设备可信通信协议
NekoDrop  = 第一个落地产品：跨设备文件互传
NekoState = 跨设备状态同步层
OpenNeko  = 上层 AI 桌面伴侣 / Agent 运行时
```

也就是说，NekoDrop 是入口产品，NekoLink 才是长期底座。

短期目标是把 Mac / Windows 文件互传做成真实可用的桌面软件。中期目标是把配对、加密、传输、状态事件抽象成稳定协议。长期目标是让 Mac、Windows、手机、平板、NAS、小主机和 OpenNeko Agent 都能进入同一个可信设备网络。

## 2. 当前基线

当前项目已经具备以下基础：

- Tauri 桌面壳
- React 桌面 UI
- Rust workspace
- 真实文件选择和目录扫描
- manifest 生成
- SHA-256 校验
- TCP connection-code 传输
- transfer offer / accept / decline
- NekoLink Envelope
- 设备身份字段
- 稳定设备 ID 和 fingerprint
- macOS 打包脚本
- Win11 打包脚本

当前还不能假装已经完成的能力：

- 可信配对
- 加密会话
- LAN 自动发现
- 断点续传
- 手机端互通
- Relay / P2P
- OpenNeko Agent 指令通道
- NekoState 状态同步

这些能力可以在 UI 上保留方向，但必须明确标记为“待接入”，不能写 mock 数据伪装已经可用。

## 3. 设计原则

### 3.1 产品原则

- 先把真实文件互传做稳，再扩展生态。
- 桌面端必须是桌面软件，不是浏览器页面。
- UI 只展示真实状态，不展示假设备、假历史、假进度。
- 后续能力可以预留入口，但必须明确是待接入。
- 重要功能必须形成闭环：能触发、能执行、能失败、能恢复、能解释原因。
- 不做一堆分散页面，优先让核心流程清晰。

### 3.2 协议原则

- NekoLink 是应用层协议，不做 TCP/IP 替代品。
- 协议要 transport-agnostic，可以跑在 TCP、QUIC、WebSocket、Relay、P2P、蓝牙或 USB 上。
- 设备身份、配对、加密、消息信封、能力协商必须独立于 NekoDrop 文件业务。
- 协议层不能依赖 Tauri、React 或桌面 UI。
- 每个协议版本都要可验证、可兼容、可迁移。

### 3.3 安全原则

- 发现不等于信任。
- 第一次建立信任必须有用户确认。
- 文件发送前必须有 offer。
- 接收方必须能拒绝。
- 文件写入必须验证 manifest 和 checksum。
- 后续加密会话必须绑定设备身份。
- 设备丢失、换机、撤销信任必须有处理路径。

## 4. 推荐工程拆分

后续可以逐步把协议底座拆清楚：

```text
crates/
  nekolink-protocol/
    协议 Envelope、消息类型、能力枚举、版本校验

  nekolink-identity/
    设备 ID、密钥、fingerprint、设备名、平台信息

  nekolink-pairing/
    配对请求、短码确认、可信设备存储、撤销信任

  nekolink-session/
    加密握手、会话密钥、session resume、心跳

  nekolink-transport/
    TCP、QUIC、WebSocket、Relay、P2P 的统一接口

  nekodrop-core/
    文件传输领域模型、transfer job、manifest

  nekodrop-service/
    NekoDrop 业务服务，调用 NekoLink 完成文件互传

  nekodrop-storage/
    文件扫描、校验、partial 文件、断点续传
```

拆分节奏不要过早一次性大改。只有当某块逻辑已经被多个产品需要时，再从 `nekodrop-*` 里提到 `nekolink-*`。

## 5. 版本路线

### V0.5 Trusted Pairing

目标：从 connection-code 走向可信设备关系。

要做的能力：

- 配对请求消息
- 配对确认消息
- 双端短码确认
- trusted_devices.json
- 忘记设备
- 设备信任状态
- UI 显示“未配对 / 已信任 / 待确认”
- 拒绝未信任设备直接发送文件

验收标准：

- A 设备能向 B 设备发起配对。
- B 设备必须确认后才建立信任。
- 重启后 trusted device 仍然存在。
- 用户可以撤销某台设备的信任。
- UI 不出现 fake trusted device。

风险点：

- 配对短码必须防止中间人替换。
- 设备名可以伪造，所以不能只靠设备名判断身份。
- fingerprint 要给用户一个可理解的展示方式。

### V0.6 LAN Discovery

目标：让同一局域网设备自动出现。

要做的能力：

- mDNS 广播
- mDNS 发现
- UDP broadcast fallback
- 在线 / 离线状态
- 设备 heartbeat
- discovery service 生命周期
- Windows 防火墙提示处理
- macOS 本地网络权限提示说明

验收标准：

- Mac 和 Windows 在同一局域网能互相发现。
- 发现到的设备只显示为“附近设备”，不自动信任。
- 设备离线后 UI 能及时更新。
- 防火墙或权限失败时有真实错误提示。

风险点：

- 公司网络、校园网、隔离 Wi-Fi 可能屏蔽 mDNS。
- 有线台式机和无线笔记本不一定在同一广播域。
- Windows 防火墙可能拦截监听端口。

### V0.7 Encrypted Session

目标：把文件互传从明文 TCP 升级为可信加密会话。

要做的能力：

- 设备长期密钥
- 会话密钥协商
- session_id 绑定设备身份
- message authentication
- replay protection
- encrypted envelope
- session heartbeat
- session close

验收标准：

- 已配对设备之间建立加密会话。
- 未配对设备不能直接创建可信 session。
- 文件 offer、accept、decline 走加密 envelope。
- session 断开后能清理状态。

风险点：

- 不要自创密码学算法。
- 需要选择成熟库实现密钥交换和消息加密。
- 私钥存储要走系统安全目录，后续可接 Keychain / Windows Credential Manager。

### V0.8 Transfer Reliability

目标：让大文件和文件夹传输足够可靠。

要做的能力：

- partial 文件
- chunk manifest
- 断点续传
- retry failed chunk
- 取消传输
- 暂停 / 继续
- 接收目录冲突处理
- 同名文件策略
- 传输历史持久化
- 传输速度、ETA、当前文件

验收标准：

- 大文件中断后可以继续传。
- 文件夹结构完整保留。
- checksum mismatch 会失败并给出原因。
- 取消传输不会留下错误的最终文件。
- 历史记录来自真实 transfer DB，不是 mock。

风险点：

- partial 文件命名和清理策略要稳。
- Windows 长路径和非法字符要处理。
- macOS package 目录和普通文件夹的边界要确认。

### V0.9 Desktop Productization

目标：让 NekoDrop 像真实工具，而不是开发 demo。

要做的能力：

- Win11 安装包验证
- macOS `.app` / 后续 DMG
- 应用图标
- 系统托盘
- 开机启动
- 原生通知
- Finder / Explorer reveal
- 接收目录设置
- 错误恢复 UI
- 空状态和权限状态
- 日志导出

验收标准：

- Win11 可以安装、启动、卸载。
- macOS 可以启动 `.app`。
- 用户不用终端也能完成基本互传。
- 所有按钮都对应真实功能或明确待接入。

风险点：

- Tauri Windows bundler 依赖环境要写清楚。
- macOS 签名和公证后续会影响分发。
- 自动更新要等基础稳定后再接。

### V1.0 Mac / Windows Stable

目标：形成第一个可对外试用版本。

范围：

- Mac <-> Windows 文件互传
- 可信配对
- 加密 session
- LAN discovery
- connection-code fallback
- 文件夹传输
- 断点续传
- 历史记录
- 桌面安装包

不放进 V1.0 的内容：

- 手机端完整文件管理
- OpenNeko Agent 执行
- NekoState 分布式状态库
- 公网 Relay
- 账号系统
- 云同步

V1.0 的核心判断标准：

```text
不用微信，不用网盘，不用 U 盘。
Mac 和 Windows 之间能稳定、安全、可恢复地互传文件。
```

### V1.1 Mobile Companion

目标：手机进入可信设备网络。

要做的能力：

- iOS / Android / OpenHarmony 设备身份
- 手机端扫码配对
- 手机发送文件到电脑
- 电脑发送文件到手机
- 手机查看已配对设备
- 手机触发电脑接收模式
- 系统分享面板接入

验收标准：

- 手机可以和桌面端建立可信配对。
- 手机可以真实发送文件到电脑。
- 桌面端可以真实发送文件到手机。
- 手机端能力不足时不显示 fake 功能。

风险点：

- iOS 后台传输限制。
- Android 文件权限复杂。
- OpenHarmony 文件选择和后台能力需要单独验证。
- 移动端局域网发现可能受系统权限影响。

### V1.2 Relay / Non-LAN Transport

目标：解决不在同一局域网、无线和有线不通、校园网隔离等问题。

要做的能力：

- relay transport
- relay auth
- NAT 类型检测
- P2P 打洞尝试
- relay fallback
- connection-code remote mode
- 带宽和限速策略

验收标准：

- 不在同一广播域时仍然能通过 relay 建立连接。
- P2P 成功时不走 relay 流量。
- relay 失败时给出明确原因。
- 大文件走 relay 时有速率、成本和安全提示。

风险点：

- relay 会引入服务器成本。
- 公网传输安全要求更高。
- P2P 打洞不保证成功，所以必须有 fallback。

### V1.3 Clipboard / Quick Share

目标：扩展 NekoDrop 的日常使用频率。

要做的能力：

- 剪贴板文本同步
- 图片剪贴板同步
- 选中文件快速发送
- 最近设备快速发送
- 自动接收规则
- trusted device per-device policy

验收标准：

- 已信任设备之间可以发送剪贴板。
- 用户可以关闭剪贴板同步。
- 自动接收只对明确授权设备生效。

风险点：

- 剪贴板可能包含敏感信息。
- 自动同步必须默认关闭或强提示。
- 多设备同时同步要避免循环。

### V1.4 NekoState

目标：从文件传输升级到跨设备状态同步。

要做的能力：

- state namespace
- key-value state
- append-only event log
- conflict policy
- device cursor
- sync checkpoint
- offline replay
- 状态订阅

可同步的数据：

- 应用设置
- 设备状态
- 任务状态
- Agent 执行状态
- OpenNeko 角色偏好
- 轻量记忆索引

验收标准：

- 两台设备能同步一个 namespace 的状态。
- 离线设备回来后能追上状态。
- 冲突有明确策略，不静默覆盖。
- NekoState 不直接绑定 NekoDrop UI。

风险点：

- 不要一开始就做复杂分布式数据库。
- 先做轻量状态同步，不碰重事务。
- 用户隐私和删除同步必须设计清楚。

### V1.5 OpenNeko Agent Integration

目标：让 NekoLink 成为 OpenNeko 跨设备 Agent 通信底座。

要做的能力：

- agent.command
- agent.result
- agent.progress
- agent.cancel
- agent permission scope
- 设备能力注册
- 远端任务执行确认
- 手机发起桌面 Agent 任务
- 桌面 Agent 调用 NekoDrop 文件流

典型场景：

```text
手机说：
把 Mac 下载目录里今天的设计图发到 Windows 台式机，然后让台式机压缩归档。

OpenNeko 负责理解任务。
NekoLink 负责设备通信。
NekoDrop 负责文件流。
NekoState 负责任务状态。
```

验收标准：

- 手机端可以向桌面端发送一个安全的 Agent 指令。
- 桌面端必须确认高风险操作。
- Agent 结果能回传到发起设备。
- 任务状态可以被 NekoState 订阅。

风险点：

- Agent 权限必须比普通文件传输更严格。
- 文件删除、脚本执行、系统操作必须有权限边界。
- 不能让“伴侣 UI”掩盖真实安全风险。

### V2.0 NekoLink Protocol SDK

目标：把 NekoLink 从内部模块升级为可复用协议。

要做的能力：

- 协议规范文档
- Rust SDK
- TypeScript SDK
- Swift / Kotlin 或移动端绑定
- conformance tests
- message schema versioning
- transport adapter API
- sample apps
- compatibility matrix

验收标准：

- 新应用可以不依赖 NekoDrop UI，直接使用 NekoLink 建立设备通信。
- 同一套协议能跑桌面、手机、OpenNeko Agent 节点。
- 旧版本设备能和新版本设备协商能力。

风险点：

- SDK 一旦公开，兼容成本会明显上升。
- 协议字段不能随意改。
- 安全模型要先稳定，再谈生态。

## 6. 页面和 UI 迭代方向

UI 必须跟真实能力同步。

推荐页面保持克制：

```text
首页
  真实快速发送、接收状态、当前设备身份

设备 / 工作区
  附近设备、可信设备、配对、传输任务

传输
  当前传输、历史、失败重试、断点续传

设置
  设备名、接收目录、安全、网络、日志
```

如果未来要接入 OpenNeko 主项目，可以映射为：

```text
陪伴首页
  Live2D 角色、当前设备状态、快速指令

Agent 工作空间
  跨设备任务、Agent 执行、文件流调用

世界
  设备网络、NekoDrop、NekoState、玩法功能

我的
  身份、安全、设备、设置
```

设计要求：

- 不要堆卡片包裹。
- 不要做一堆 fake 数据面板。
- 状态必须来自真实服务。
- 未完成能力显示为“待接入”。
- 主要操作要明显：发送、接收、配对、取消、重试。
- 设备连接状态要比装饰性视觉更重要。

## 7. 测试路线

### 单元测试

- manifest 路径安全
- checksum
- envelope validate
- identity validate
- pairing state
- trusted device store
- chunk resume

### 集成测试

- loopback TCP 文件传输
- transfer offer / decline
- 文件夹传输
- checksum mismatch
- partial resume
- encrypted session

### 跨平台手测矩阵

```text
macOS -> macOS
macOS -> Windows 11
Windows 11 -> macOS
Windows 11 -> Windows 11
Android -> Windows 11
Android -> macOS
iOS -> macOS
OpenHarmony -> Windows 11
```

### 网络场景

```text
同一 Wi-Fi
有线 + 无线同路由
有线 + 无线不同网段
校园网隔离
手机热点
公网 Relay
P2P 成功
P2P 失败后 Relay fallback
```

## 8. 打包和发布路线

### macOS

- `.app` 稳定产出
- DMG 修复
- 应用图标
- 签名
- 公证
- 自动更新

### Windows 11

- NSIS 安装包
- MSI 安装包
- 应用图标
- Windows 防火墙说明
- 开机启动
- 卸载清理策略
- 自动更新

### 手机端

- Android debug build
- Android release build
- iOS TestFlight
- OpenHarmony hap
- 分享面板接入
- 后台任务策略

## 9. 数据和兼容策略

本地数据建议分层：

```text
device_identity.json
trusted_devices.json
sessions.db
transfers.db
partial_transfers/
settings.json
logs/
```

兼容规则：

- device_id 一旦生成，不轻易重置。
- trusted device 需要记录协议版本和 key version。
- transfer history 可以迁移，但不能影响文件安全。
- 协议消息必须带 version。
- 新 capability 必须可协商，不能默认假设对方支持。

## 10. 风险清单

### 技术风险

- Windows 防火墙导致无法监听。
- mDNS 在部分网络不可用。
- P2P 打洞成功率不可控。
- 大文件断点续传复杂度高。
- 移动端后台限制会影响体验。
- OpenNeko Agent 权限边界复杂。

### 产品风险

- 功能太多导致主流程不清晰。
- UI 过度装饰，真实状态不明显。
- 未完成能力包装得像已完成，会破坏可信度。
- 太早做生态，核心互传还不稳定。

### 安全风险

- 设备名伪造。
- 配对被中间人攻击。
- 未授权设备发起文件。
- 自动接收导致敏感文件落地。
- Agent 指令权限过大。

## 11. 优先级建议

最推荐的下一步顺序：

```text
1. V0.5 Trusted Pairing
2. V0.6 LAN Discovery
3. V0.7 Encrypted Session
4. V0.8 Transfer Reliability
5. V0.9 Desktop Productization
6. V1.0 Mac / Windows Stable
7. V1.1 Mobile Companion
8. V1.2 Relay / P2P
9. V1.4 NekoState
10. V1.5 OpenNeko Agent Integration
```

当前最不应该跳过的是可信配对和加密会话。没有这两层，后面手机控制电脑、Agent 跨设备执行、状态同步都会缺少安全地基。

## 12. 一句话总结

NekoDrop 的短期价值是稳定的 Mac / Windows 文件互传。

NekoLink 的长期价值是本地优先的个人设备可信网络。

OpenNeko 的最终价值是把这个可信设备网络变成 AI 伴侣和 Agent 的执行底座。
