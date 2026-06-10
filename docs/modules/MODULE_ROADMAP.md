# 模块化路线图

这份路线图按模块推进，不按页面堆功能。

目标是避免后续出现：

```text
UI 看起来很多功能
底层协议没有完成
设备发现不可靠
文件传输还不稳
Agent 又提前接进来
```

## V0.5 Trusted Pairing

主模块：

- NekoLink Identity
- NekoLink Pairing
- Desktop UI

要做：

- trusted device store
- pairing request
- pairing confirm
- short code
- fingerprint display
- forget device
- blocked device

不做：

- 自动 Agent 执行
- NekoState
- Relay

验收：

- 两台设备可以建立信任。
- 重启后信任关系存在。
- 用户可以撤销信任。
- 未信任设备不能被伪装成可信设备。

## V0.6 Auto Discovery

主模块：

- Discovery
- NekoDrop UI
- NekoLink Device Hello

要做：

- 启动后自动接收
- UDP multicast 或 mDNS discovery
- nearby devices
- 点设备发送
- 连接码兜底
- IP 过滤

必须修：

- 不再生成 `198.18.x.x` 连接地址。
- 不再默认给另一台电脑 `127.0.0.1`。

验收：

- 同一局域网两台电脑自动出现。
- 用户不用复制连接码即可发送。
- 发现失败时连接码仍可使用。
- UI 只显示真实发现到的设备。

## V0.7 NekoLink Transport Abstraction

主模块：

- NekoLink Transport
- NekoDrop Service
- NekoLink Protocol

要做：

- `NekoLinkTransport` trait
- `TcpTransport`
- file offer 走统一 Envelope
- file decision 走统一 Envelope
- transport error model
- capability negotiation

验收：

- 当前 TCP 文件传输不退化。
- 发送逻辑不直接依赖 connection code。
- 后续 iroh 可以作为新 transport 接入。

## V0.8 Encrypted Session

主模块：

- NekoLink Session
- NekoLink Identity
- NekoLink Pairing

要做：

- session handshake
- session key
- message authentication
- replay protection
- heartbeat
- session close

验收：

- 已配对设备之间建立加密 session。
- 未配对设备不能发起可信传输。
- 文件 offer / accept / decline 走加密消息。

## V0.9 Transfer Reliability

主模块：

- NekoDrop Storage
- NekoDrop Service
- Desktop UI

要做：

- partial files
- chunk manifest
- resume
- retry
- cancel
- pause / continue
- transfer history
- better error messages

验收：

- 大文件中断后可继续。
- 文件夹结构完整保留。
- checksum mismatch 不写最终文件。
- 取消后不会留下错误完成状态。

## V1.0 Desktop Stable

主模块：

- NekoDrop Desktop
- NekoLink Protocol
- Discovery
- Storage

要做：

- Mac -> Windows
- Windows -> Mac
- 自动发现
- 可信配对
- 加密 session
- 文件夹传输
- 断点续传
- 安装包
- 日志导出

验收：

- 普通用户不用终端完成互传。
- Win11 安装包可用。
- macOS `.app` / DMG 可用。
- 失败原因可读。

## V1.1 Mobile Companion

主模块：

- Mobile App
- NekoLink Identity
- NekoDrop Service

要做：

- iOS / Android / OpenHarmony identity
- 扫码配对
- 手机发文件到电脑
- 电脑发文件到手机
- 分享面板
- 手机设备列表

验收：

- 手机真实加入设备网络。
- 手机与桌面能互传文件。
- 手机端权限失败有明确提示。

## V1.2 iroh Transport

主模块：

- NekoLink Transport
- NekoLink Session
- Relay

要做：

- iroh key
- iroh endpoint
- iroh stream
- peer dialing
- relay fallback
- P2P attempt
- transport selection

验收：

- 同局域网可直连。
- 不同网络可尝试 P2P。
- P2P 失败可走 relay。
- 文件传输可跑在 iroh stream 上。

## V1.3 Clipboard / Quick Share

主模块：

- NekoDrop Product
- NekoLink Capability
- Desktop Integration

要做：

- 文本剪贴板
- 图片剪贴板
- 选中文件快速发送
- 最近设备
- per-device policy

验收：

- 剪贴板同步默认可控。
- 只对已信任设备启用。
- 不出现同步循环。

## V1.4 NekoState

主模块：

- NekoState
- NekoLink State Messages
- OpenNeko Integration

要做：

- namespace
- key-value
- event log
- device cursor
- checkpoint
- offline replay
- conflict policy

验收：

- 两台设备能同步一个 namespace。
- 离线设备回来后可追状态。
- 冲突不静默覆盖。

## V1.5 OpenNeko Agent Integration

主模块：

- OpenNeko
- NekoLink Agent Messages
- NekoState
- NekoDrop File Capability

要做：

- agent.command
- agent.progress
- agent.result
- agent.cancel
- permission scope
- remote task confirmation
- file capability binding

验收：

- 手机可发起桌面 Agent 任务。
- 桌面高风险操作必须确认。
- 任务状态能同步到其他设备。
- 文件流可由 Agent 安全调用。

## V2.0 NekoLink SDK

主模块：

- NekoLink
- SDK
- Docs
- Open Source

要做：

- Rust SDK
- TypeScript SDK
- mobile bindings
- conformance tests
- protocol docs
- sample apps
- compatibility matrix

验收：

- 新应用可以不依赖 NekoDrop UI 使用 NekoLink。
- 旧版本和新版本能协商能力。
- NekoLink 可以拆出独立仓库。

## 优先级

当前最应该做：

```text
1. Auto Discovery
2. IP 过滤
3. 点设备发送
4. Trusted Pairing
5. Transport Abstraction
6. Encrypted Session
```

当前不应该做：

```text
1. 假设备列表
2. 假历史
3. 假 Agent
4. 复杂多页面
5. 过早拆 NekoLink 仓库
6. 过早做云账号
```
