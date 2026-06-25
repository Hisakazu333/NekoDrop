# 下一阶段为什么这样排

这份文档解释 NekoDrop / NekoLink 接下来为什么先收口加密文件流，再做 bundle 闭环和本机 local bridge，最后才做 iroh / relay / P2P。真实完成状态以 [STATUS.md](STATUS.md) 为准，阶段列表以 [ROADMAP.md](ROADMAP.md) 为准。

## 当前基线

当前 beta 已经能完成 macOS 和 Windows 的局域网文件互传。桌面主线包含自动发现、连接码兜底、可信配对、传输历史、进度、失败恢复、大目录 offer、partial/resume 基础、安装包打包和刷新减负。

NekoLink 安全层已经进入桌面传输主线：

- `session.hello` / `session.ready` 建立 ephemeral encrypted session
- `file.offer`、`file.accept`、`file.decline` 走 encrypted `session.control`
- offer / decision 控制消息读取路径有 replay window
- encrypted session 路径的文件 payload 已经切成加密 file frames
- `nekolink-protocol` 已有 session identity binding 的 canonical payload hash

这意味着控制消息和 encrypted session 文件 payload 已经不再依赖明文 LAN 信任。bundle、local bridge 和通用 adapter 的基础闭环也已经接入，但还有三个边界没有收口：

- legacy plain file stream 仍然保留，需要迁移或拒绝策略
- 真实上层应用 adapter 还没有接入，当前只有通用样例
- local bridge 还是事件轮询和短等待，还没有真正长连接订阅

所以现在不应该直接跳到 iroh、跨公网或 Agent 上层能力。跨网络 transport 解决的是“怎么连”，不能替代加密、权限和导入边界。

## 阶段 1：Encrypted Session 收口

加密文件流已经接进 encrypted session 路径，接收端也已经改成按 reader 逐帧解密。下一刀不是重写文件流，而是把 session 的身份和兼容边界收住。

这一阶段已经开始：

- 定义 session identity binding
- 让 binding 绑定 session_id、设备 ID、fingerprint、session ephemeral key 和 handshake_hash
- 提供 canonical payload hash，作为后续签名输入

后续要完成：

- 长期身份密钥认证
- 明确 legacy plain file stream 的兼容策略
- 增加截断、乱序和更多 replay 边界测试
- 保持 partial/resume、cancel、history、progress 不倒退

主要风险：

- nonce/counter 不能复用
- encrypted chunk 边界不能破坏 resume
- 失败恢复不能只看普通 TCP offset
- 旧 plain path 不能继续承载敏感 bundle 或本机接入能力

完成标准：

- 控制消息和文件 payload 都在 session 保护边界内
- trusted device 和 session 绑定到长期身份密钥
- 明文兼容路径有明确迁移或拒绝策略
- 失败恢复、取消、历史记录不倒退

## 阶段 2：NekoLink Bundle 闭环

bundle 要解决的是“上层数据怎么传”，不是“网络怎么连”。skills、session、workspace、agent profile 不能当普通散文件发，因为接收端需要知道它们是什么、能不能导入、会改哪些本机状态。

仓库里已经有 bundle manifest、checksums、permissions、staging、手动创建入口、导入计划、冲突策略、NekoDrop 本机导入区、import receipt 和保守撤回。通用 adapter 样例已经演示导出、bridge 请求、adapter-owned 导入和 adapter 私有 receipt 回滚。下一阶段要补的是把这些样例接到真实上层应用：

- 应用自己的导出入口
- 应用自己的导入确认和事务写入
- 应用自己的冲突策略
- 应用自己的回滚记录
- 上层数据版本迁移

主要风险：

- bundle type 过早泛化会变成无法维护的万能包
- session / skills 可能包含 token、密钥、隐私路径或本机绝对路径
- 接收后自动导入会造成权限和数据污染问题
- 版本兼容不清楚会让旧客户端误读新 bundle

完成标准：

- 现有传输通道能发送一个 bundle
- 接收端能预览、校验、拒绝、保存
- 导入必须由用户或授权上层应用显式触发
- 默认不同步 token、密钥和隐私文件

## 阶段 3：本机 Local Bridge

本机应用不应该直接实现 NekoLink 网络协议，也不应该调用 NekoDrop 桌面端内部函数。它们应该通过本机 local bridge 请求 NekoLink 能力。

推荐调用关系：

```text
local application
  -> local bridge API
  -> NekoLink session
  -> paired device
```

仓库里已经有 `LocalBridgeRequest` / `LocalBridgeEvent` 模型、权限 scope、localhost runtime、授权码、持久化授权、待执行队列、后台 worker、动作结果和 `events.poll`。下一阶段要补的是更稳定的订阅和真实应用接入：

- 事件订阅或更低成本的长轮询
- 上层 adapter 对动作结果、receipt 和回滚状态的持续观察
- 本机接入 UI 的授权、待执行、结果和失败原因收口
- 真实应用 adapter 的最小接入样例

暂不开放：

- 任意远端命令执行
- 未确认自动导入
- 默认读取全盘 workspace
- 直接暴露 NekoDrop 内部路径
- 本机应用绕过 NekoLink 自己连对方设备

完成标准：

- 本机调用有鉴权和权限 scope
- 本机应用只能发送明确类型的 bundle
- 收到 bundle 后先通知，再由用户或授权上层应用确认导入
- local bridge 不绑定某一个第三方应用

## 阶段 4：iroh / relay / P2P

iroh、relay、P2P 解决的是“不同网络怎么连”。它们不解决文件是否加密、bundle 是否安全、本机应用能不能乱调用的问题。所以它们应该排在加密文件流、bundle、local bridge 之后。

正确接入方式是把 iroh 当成 NekoLink transport，而不是重写 NekoDrop 文件传输：

```text
NekoDrop / OpenNeko / other app
  -> NekoLink session
  -> NekoLink bundle or file stream
  -> TCP transport or iroh transport
```

这一阶段要做：

- 调整 transport trait，支持 async stream / bidirectional stream
- 保持 TCP transport 为默认稳定路径
- 新增实验 `IrohTransport`
- 绑定 device_id 与 iroh public key / endpoint id
- 在 iroh stream 上跑现有 NekoLink envelope
- 在 iroh stream 上跑文件流和 bundle
- P2P 失败时再考虑 relay fallback

主要风险：

- NAT 打洞成功率不可控，不能把它写成必达能力
- relay 成本、滥用、防刷和隐私边界都要设计
- 不同 transport 的错误模型不能破坏现有用户提示
- iroh 失败时必须能回到 TCP / 连接码或明确错误

完成标准：

- 同一套 session 和 bundle 能跑在 TCP 或 iroh 上
- 上层产品不依赖具体 transport 细节
- 不同网络能尝试 P2P，失败时有明确 fallback
- relay 不保存最终文件，也不变成云盘

## 暂时不要做的事

这些事会让项目变宽，但不会先解决底层问题：

- 直接做跨公网 relay 产品
- 直接做游戏联机 UI
- 直接开放 Agent 远程命令
- 把 skills/session 当普通文件夹自动同步
- 做账号系统或云盘式文件存储
- 为远期手机端提前大改桌面 UI

游戏联机、跨设备 Agent、应用多端协同都属于 NekoLink 压实后的上层能力。当前要把它们当作设计约束，而不是当前 beta 的功能承诺。

## 下一步可执行顺序

短期建议按这个顺序开分支：

1. `security/session-identity-binding`
2. `bundle/staging-import-lifecycle`
3. `bridge/local-runtime-auth`
4. `bridge/bundle-send-import-requests`
5. `transport/iroh-spike`

每个分支只做一件事。每个 PR 合并前更新 [STATUS.md](STATUS.md)、[ROADMAP.md](ROADMAP.md) 和相关协议文档。
