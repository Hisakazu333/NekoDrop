# NekoDrop Roadmap

这份路线图只写接下来怎么走。真实完成状态以 [STATUS.md](STATUS.md) 为准。

## 当前阶段

NekoDrop 已经有一个可用的 macOS / Windows 桌面互传主线：

- 局域网 TCP 文件和文件夹传输
- mDNS / DNS-SD 自动发现
- 连接码兜底
- 可信设备配对
- 接收确认
- SHA-256 校验
- 传输进度、速度、ETA、当前文件
- 发送/接收取消
- partial/resume 基础
- 失败历史、继续发送、重试、备用码入口
- 大目录 offer 支持
- 桌面端状态刷新减负
- macOS DMG 和 Windows 安装包打包脚本

这个阶段的目标不是继续堆 UI，而是把 NekoLink 的安全会话和后续上层数据传输能力接稳。

## 已完成：Encrypted Session Desktop Wiring

目标：让桌面真实传输主线开始使用 NekoLink 加密控制消息。

已经接入：

- 保持当前 TCP 文件传输主线可用。
- 在发送端建立 `session.hello`。
- 在接收端返回并校验 `session.ready`。
- 基于 X25519 shared secret 和 HKDF 派生 session key material。
- 让 `file.offer`、`file.accept`、`file.decline` 走 encrypted `session.control`。

仍未完成：

- 文件 payload 加密
- replay window
- 长期身份密钥认证
- iroh runtime
- relay server
- Agent command
- skills/session bundle
- 手机端互通

## 下一阶段：Encrypted File Stream

目标：把文件 payload 也放进 session 保护边界。

范围：

- 定义 encrypted file frame header。
- 使用 session traffic counter 生成 nonce。
- 加密文件 payload 或 chunk payload。
- 校验 AAD，绑定 transfer_id、manifest_path、offset、size。
- 断点续传继续可用。
- checksum 仍作为落盘后的完整性校验。

完成标准：

- 控制消息和文件内容都不再依赖明文局域网信任。
- 失败恢复、取消和历史记录行为不倒退。

## 随后：NekoLink Bundle

目标：给上层数据传输建立统一包格式，不把 skills、session、agent profile 当作普通散文件乱传。

规格文档：[BUNDLE_SPEC.md](BUNDLE_SPEC.md)。当前只定义包格式和安全边界，还没有接入桌面发送、接收检测或导入流程。

候选包类型：

- `skill`
- `session`
- `workspace`
- `agent_profile`
- `config_snapshot`

包结构方向：

```text
bundle.json
files/
checksums.json
permissions.json
```

`bundle.json` 至少应包含：

- bundle id
- bundle type
- schema version
- source app
- created_at
- sender device identity
- file list
- total bytes
- compatibility hints

完成标准：

- 可以用现有 NekoDrop/NekoLink 通道发送一个 bundle。
- 接收端能预览、校验、拒绝、保存。
- 导入行为必须由上层应用显式触发，不能收到就自动改本机配置。

## 随后：CCS / OpenNeko Local Bridge

目标：让 CCS 插件或 OpenNeko 通过本机服务调用 NekoLink，而不是插件直接实现网络协议。

方向：

```text
CCS / OpenNeko plugin
  -> local bridge API
  -> NekoLink session
  -> paired device
```

本阶段要解决：

- 本机 API 鉴权
- 插件调用权限
- bundle 发送入口
- bundle 接收通知
- 导入确认

不做：

- 插件绕过 NekoLink 直接连对方设备
- 未确认自动导入 session / skills
- 把 token、密钥或隐私文件默认纳入同步

## transport：iroh / Relay / P2P

iroh 应该作为 NekoLink transport 接入，而不是直接替换 NekoDrop 的 TCP 代码。

顺序：

1. 调整 transport trait，支持 async stream / bidirectional stream。
2. 保持 TCP transport 为默认稳定路径。
3. 新增实验 `IrohTransport`。
4. 绑定 device_id 与 iroh public key / endpoint id。
5. 在 iroh stream 上跑现有 NekoLink envelope。
6. 在 iroh stream 上跑文件流。
7. P2P 失败时再考虑 relay fallback。

完成标准：

- 同一套 NekoLink session 和 bundle 能跑在 TCP 或 iroh 上。
- 上层 NekoDrop / OpenNeko 不依赖具体 transport 细节。
- iroh 失败时能回退到明确错误或 TCP / 连接码路径。

## 暂不进入范围

- 云盘式文件存储
- 账号系统
- 默认跨公网 relay
- 自动同步用户全部配置
- 自动执行远端 Agent 指令
- 未加密 session 的 skills/session 传输
- 未确认导入的 session / skills / agent profile

## 文档维护

- 新功能合并后先更新 [STATUS.md](STATUS.md)。
- 路线图只能写阶段目标和边界。
- README 只写用户能理解的能力和方向。
- 协议细节写入 [PROTOCOL.md](PROTOCOL.md)。
- 安全边界写入 [SECURITY.md](SECURITY.md)。
