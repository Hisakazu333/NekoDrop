# 下一阶段为什么这样排

这份文档解释 NekoDrop / NekoLink 接下来为什么先做加密文件流和 bundle，再做 CCS/OpenNeko 本地桥，最后才做 iroh / relay / P2P。它不替代 [STATUS.md](STATUS.md) 和 [ROADMAP.md](ROADMAP.md)：状态看 STATUS，阶段列表看 ROADMAP，取舍理由看这里。

## 当前基线

当前 beta 已经能完成 macOS 和 Windows 的局域网文件互传。桌面主线包含自动发现、连接码兜底、可信配对、传输历史、进度、失败恢复、大目录 offer、partial/resume 基础和安装包打包。

NekoLink 安全层也已经进入桌面传输主线：`file.offer`、`file.accept`、`file.decline` 已经走 encrypted `session.control`。这意味着传输前的控制消息不再只依赖明文 LAN JSON 帧，但文件 payload 仍是明文 TCP 流。

因此，下一阶段不能直接跳到 iroh 或上层 Agent。底层还缺三个关键边界：

- 文件内容还没进入 session 保护边界
- 上层数据还没有 bundle 包格式
- 本机插件还没有受控调用 NekoLink 的 local bridge

## 阶段 1：加密文件流

加密文件流必须排在 bundle 和跨网络 transport 之前。否则后面传 skills、session、workspace、agent profile 时，只是把敏感数据放进普通文件流里。

这一阶段要完成：

- 定义 encrypted file frame
- 用 session traffic counter 生成 nonce
- 把 transfer_id、manifest_path、offset、size 绑定进 associated data
- 保留 partial/resume 语义
- 保留 SHA-256 作为落盘后的完整性校验
- 增加重放、乱序、篡改 payload 的测试

主要风险：

- 断点续传和加密 chunk 边界容易冲突
- nonce 复用会变成严重安全问题
- 文件流加密后，失败恢复不能只看普通 TCP offset
- 性能不能让大文件传输明显退化

完成标准：

- 控制消息和文件 payload 都在 session 保护边界内
- 失败恢复、取消、历史记录不倒退
- 明文兼容路径有清楚的迁移或拒绝策略

## 阶段 2：NekoLink bundle

bundle 要解决的是“上层数据怎么传”，不是“网络怎么连”。skills、session、workspace、agent profile 不能当普通散文件发，因为接收端需要知道它们是什么、能不能导入、会改哪些本机状态。

bundle 应该先跑在现有 LAN TCP + encrypted session 上。这样可以在稳定网络里把包格式、权限、预览、校验和导入确认压实，再换 transport。

建议包结构：

```text
bundle.json
files/
checksums.json
permissions.json
```

`bundle.json` 至少记录：

- bundle id
- bundle type
- schema version
- source app
- created_at
- sender device identity
- file list
- total bytes
- compatibility hints

主要风险：

- bundle type 过早泛化会变成无法维护的万能包
- session / skills 可能包含 token、密钥、隐私路径或本机绝对路径
- 接收后自动导入会造成权限和数据污染问题
- 版本兼容不清楚会让旧客户端误读新 bundle

完成标准：

- 现有传输通道能发送一个 bundle
- 接收端能预览、校验、拒绝、保存
- 导入必须由上层应用显式触发
- 默认不同步 token、密钥和隐私文件

## 阶段 3：CCS/OpenNeko 本地桥

CCS 插件和 OpenNeko 不应该直接实现 NekoLink 网络协议，也不应该调用 NekoDrop 桌面端的内部函数。它们应该通过本机 local bridge 调用 NekoLink。

推荐调用关系：

```text
CCS / OpenNeko plugin
  -> local bridge API
  -> NekoLink session
  -> paired device
```

local bridge 先只开放受控能力：

- 查询可信设备
- 发送 bundle
- 接收 bundle 通知
- 发起导入确认
- 查看传输状态

暂不开放：

- 任意远端命令执行
- 未确认自动导入
- 默认读取全盘 workspace
- 直接暴露 NekoDrop 内部路径
- 插件绕过 NekoLink 自己连对方设备

主要风险：

- 本机 API 如果没有鉴权，本机任意进程都能借 NekoLink 发敏感数据
- Agent 能力比文件互传风险更高，必须先限制 scope
- UI 同意一次不能代表永久允许所有插件行为
- bridge 如果绑定 NekoDrop 内部模型，后面 OpenNeko 会被桌面文件互传实现拖住

完成标准：

- 本机调用有鉴权和权限 scope
- 插件只能发送明确类型的 bundle
- 收到 bundle 后先通知，再由上层确认导入
- OpenNeko 不依赖 NekoDrop UI 或 Tauri 命令细节

## 阶段 4：iroh / relay / P2P

iroh、relay、P2P 解决的是“不同网络怎么连”。它们不解决文件是否加密、bundle 是否安全、插件能不能乱调用的问题。所以它们应该排在加密文件流、bundle、local bridge 之后。

正确接入方式是把 iroh 当成 NekoLink transport，而不是重写 NekoDrop 文件传输：

```text
NekoDrop / OpenNeko
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

1. `security/encrypted-file-stream`
2. `protocol/nekolink-bundle-manifest`
3. `bridge/local-api-skeleton`
4. `bridge/local-api-permissions`
5. `transport/iroh-spike`

每个分支只做一件事。每个 PR 合并前更新 [STATUS.md](STATUS.md)、[ROADMAP.md](ROADMAP.md) 和相关协议文档。
