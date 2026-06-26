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
- 给 offer / decision 控制消息读取路径接入 replay window。

已经接入：

- encrypted session 发送/接收路径的文件 payload 已切成加密 file frames。
- file frame AAD 绑定 transfer_id、manifest_path、offset 和 plain_size。
- encrypted session resume 仍按 partial offset 补传剩余 payload。
- 桌面主线已经交换并验签 `session.identity`。
- 可信设备的 authenticated session 会钉到记录里的长期 public key。

仍未完成：

- legacy plain file stream 的迁移或拒绝策略继续收口
- key rotation、OS keychain / credential-manager 存储和跨平台身份策略
- iroh runtime
- relay server
- Agent command
- 真实上层应用自动导出 / 导入 skills、session、workspace
- 手机端互通

## 已完成：Encrypted File Stream 接收端 streaming 解密

目标：把加密文件流从“整文件解密后写入”改成适合大文件长期使用的接收路径。

已经接入：

- 接收端改成 streaming 解密，不再为单文件完整 payload 分配内存。
- storage 按普通 reader 写入文件，network 层按需读取和解密 encrypted file frames。
- encrypted frame 的 path、offset、AAD 篡改仍会失败。

后续范围：

- 给 encrypted file frame 增加更完整的乱序、截断和重放测试。
- 明确 legacy plain file stream 的兼容策略和迁移策略。
- checksum 继续作为落盘后的完整性校验。

## 已完成：Session Identity Binding 桌面主线

目标：把 encrypted session 从“ephemeral 会话加密”推进到“可绑定长期设备身份”的路径。

已经接入：

- `nekolink-protocol` 可以从 verified handshake 生成 initiator / responder identity binding。
- binding 的 canonical payload hash 绑定 session_id、设备 ID、fingerprint、session ephemeral key 和 handshake_hash。
- 桌面端持久化 Ed25519 signing seed。
- `session.ready` 后交换 signed `session.identity`。
- 验签失败或可信设备 public key 不匹配会拒绝 session。

后续范围：

- key rotation 和撤销策略。
- macOS Keychain / Windows Credential Manager 或 DPAPI。
- 更多跨版本迁移和异常路径测试。
- legacy plain 兼容路径继续收窄。

完成标准：

- 控制消息和文件内容都不再依赖明文局域网信任。
- 失败恢复、取消和历史记录行为不倒退。

## 随后：NekoLink Bundle

目标：给上层数据传输建立统一包格式，不把 skills、session、agent profile 当作普通散文件乱传。

规格文档：[BUNDLE_SPEC.md](BUNDLE_SPEC.md)。当前已有协议模型、校验、staging、手动创建、收到后查看、删除、过期清理、导入计划、冲突策略、导入到 NekoDrop 本机导入区和保守撤回。通用 adapter 样例已经覆盖导出、local bridge 请求、adapter-owned 目标导入和 adapter 私有 receipt 撤回；真实上层应用 adapter 还没有接入。

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
- local bridge 能授权请求发送、导入和查询动作结果。
- 导入行为必须由上层应用显式触发，不能收到就自动改本机配置。

## 随后：本机 Local Bridge

目标：让本机应用通过受控服务调用 NekoLink，而不是每个应用都自己实现网络协议。

方向：

```text
local application
  -> local bridge API
  -> NekoLink session
  -> paired device
```

本阶段要解决：

- 当前优先做真实上层 application adapter，其次收紧 bridge event stream 和 adapter transaction / migration contract
- 更完整的事件订阅，不只依赖短等待轮询
- 本机接入 UI 对待授权、待执行和失败原因的展示
- 真实上层应用 adapter，让应用按通用样例导出、发送、接收、导入和回滚
- 导入到第三方应用后的事务、冲突和回滚契约

不做：

- 本机应用绕过 NekoLink 直接连对方设备
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
