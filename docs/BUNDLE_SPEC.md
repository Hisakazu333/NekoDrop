# NekoLink bundle 规格

这份规格定义 NekoLink 后续传递上层数据时的包格式。bundle 不是普通文件夹压缩包，也不是自动同步协议；它是一个可预览、可校验、可拒绝、可由上层应用显式导入的数据包。

当前文档定义协议和安全边界。桌面端实现进度以 [STATUS.md](STATUS.md) 为准。

## 为什么需要 bundle

NekoDrop 现在已经能发送文件和文件夹，但 skills、session、workspace、agent profile 这类数据不能直接当普通文件传。接收端需要先知道这包数据是什么、来自哪个应用、会写入哪些位置、需要哪些权限、能不能被当前客户端理解。

bundle 要解决这些问题：

- 给上层数据一个统一 manifest
- 在接收前展示类型、来源、大小和权限
- 绑定每个 payload 文件的 checksum
- 阻止 token、密钥和本机隐私路径默认进入同步
- 让本机 local bridge 只处理明确类型的数据包
- 让未来 iroh / relay / P2P 只替换 transport，不改变上层包语义

## 非目标

第一版 bundle 不做这些事：

- 不做云盘目录同步
- 不自动导入 skills、session 或 workspace
- 不传系统钥匙串、浏览器 cookie、SSH key、API token
- 不执行远端 Agent 指令
- 不做跨公网 relay
- 不定义某个上层应用的完整业务模型
- 不替代 NekoDrop 现有普通文件传输

## 包结构

bundle 在文件系统中的标准结构：

```text
bundle.json
checksums.json
permissions.json
files/
```

各部分职责：

- `bundle.json`：包身份、类型、来源、版本、文件列表和兼容信息
- `checksums.json`：每个 payload 文件的 SHA-256
- `permissions.json`：导入前需要展示和确认的权限
- `files/`：实际 payload 文件，只能通过相对路径引用

发送时可以把这些文件作为一个目录发送，也可以后续封装成单文件归档。无论外层如何打包，内部结构必须保持一致。

## bundle 类型

第一版只允许这些类型：

| 类型 | 用途 | 默认导入 |
| --- | --- | --- |
| `skill` | 单个技能或插件能力包 | 否 |
| `session` | 上层应用会话数据 | 否 |
| `workspace` | 项目工作区快照或片段 | 否 |
| `agent_profile` | Agent 配置、角色偏好或能力声明 | 否 |
| `config_snapshot` | 应用配置快照 | 否 |

未知类型必须拒绝导入，但可以保存为普通 bundle 文件。

## v1 覆盖范围

bundle v1 要支撑下一阶段的本地多端协作，但不把完整生态一次做完。

v1 必须够用来做：

- 本机应用发送 `skill`
- 本机应用发送 `session`
- 发送 `workspace` 片段
- 发送 `agent_profile`
- 接收端预览 bundle
- 校验文件大小和 SHA-256
- 展示权限请求
- 保存到 staging
- 等用户或上层应用确认后再导入

v1 的判断标准是：多端可以传上层数据，但不会因为收到一个包就自动修改本机配置、执行指令或同步隐私文件。

## 长期缺口

完整 NekoLink 生态还需要这些能力。它们不进入 bundle v1 的实现范围，但 v1 不能把后续路线堵死。

| 能力 | 为什么需要 | 当前策略 |
| --- | --- | --- |
| 加密文件流 | bundle 可能包含 session、workspace 或 agent profile，payload 不能依赖明文 TCP | encrypted session 路径已有加密 file frames 和接收端 streaming 解密；敏感 bundle 只允许 authenticated encrypted session 进入 staging / import |
| 长期身份密钥认证 | bundle 不能只信任自报 sender | 桌面传输主线已交换并验签 session identity；sender 字段用于展示，真实信任来自 verified session |
| replay protection | 防止旧 bundle 或旧控制帧被重放 | v1 记录 `bundle_id` 和 `created_at`，后续加 nonce / replay window |
| bundle 版本迁移 | v1、v2 需要可兼容演进 | `schema` 固定为 `nekolink.bundle.v1`，未知 schema 拒绝导入 |
| bundle 签名 | 公开分发 skill 或跨设备转发时，不能只依赖传输 session | v1 不做签名，但保留 source app、sender、checksum 边界 |
| 权限模型细化 | Agent、workspace、session 的风险不同 | v1 只做粗粒度 scope，后续扩展到资源级权限 |
| local bridge 鉴权 | 本机应用调 NekoLink 不能让任意进程滥用 | v1 只定义权限文件，bridge 阶段做授权码和 client id |
| staging 生命周期 | 收到的 bundle 需要过期、删除和审计 | 桌面端已有 staging、列表和删除；过期清理策略后续补 |
| 导入回滚 | 导入失败不能留下半套配置 | 桌面端手动导入到本机导入区时使用临时目录，失败不留下半成品目标目录；上层应用真实写入仍需自己的回滚 |
| 多端冲突处理 | 两台设备可能有同名 workspace、session 或 skill | v1 只做保存和预览，不自动合并 |
| iroh / relay / P2P | 不同网络下传 bundle | transport 阶段接入，不改变 bundle 语义 |

这些缺口不应该现在全部塞进 v1。v1 先把“可识别、可校验、可保存、可确认导入、可保守撤回”的闭环做稳。

## `bundle.json`

`bundle.json` 是接收端预览和兼容判断的入口。

示例：

```json
{
  "schema": "nekolink.bundle.v1",
  "bundle_id": "bundle_1234567890",
  "bundle_type": "skill",
  "display_name": "voice_transcribe",
  "source_app": "Generic Agent App",
  "created_at": "2026-06-14T10:30:00Z",
  "sender": {
    "device_id": "neko-device-1234567890",
    "device_name": "MacBook",
    "fingerprint": "sha256:0123456789abcdef"
  },
  "compatibility": {
    "min_nekolink_version": 1,
    "required_capabilities": ["bundle_transfer"]
  },
  "summary": {
    "file_count": 2,
    "total_bytes": 4096
  },
  "files": [
    {
      "path": "files/manifest.json",
      "size": 1024,
      "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      "role": "manifest"
    },
    {
      "path": "files/content.bin",
      "size": 3072,
      "sha256": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
      "role": "payload"
    }
  ]
}
```

字段规则：

- `schema` 必须是 `nekolink.bundle.v1`
- `bundle_id` 必须非空，并且只用于去重和展示，不作为信任依据
- `bundle_type` 必须是已知类型
- `display_name` 只能用于展示，不能用于路径拼接
- `source_app` 用于说明来源应用
- `created_at` 使用 UTC ISO 8601
- `sender` 必须来自当前 verified session，不信任 bundle 内自报身份
- `compatibility.required_capabilities` 必须在导入前检查
- `summary.file_count` 必须等于 `files` 数组长度
- `summary.total_bytes` 必须等于 `files` 中 `size` 总和
- `files[].path` 必须是相对 slash path
- `files[].sha256` 必须是 64 位十六进制 SHA-256

## `checksums.json`

`checksums.json` 是冗余校验索引，方便接收端在不解析业务 payload 的情况下验证文件。

示例：

```json
{
  "algorithm": "sha256",
  "files": {
    "files/manifest.json": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "files/content.bin": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
  }
}
```

校验规则：

- `algorithm` 第一版只接受 `sha256`
- `files` 的 key 必须与 `bundle.json.files[].path` 完全一致
- `files` 的 value 必须与 `bundle.json.files[].sha256` 完全一致
- 缺失、额外、大小写不一致或 checksum 不匹配都必须拒绝导入

## `permissions.json`

`permissions.json` 描述导入前需要展示给用户或上层应用确认的权限。

示例：

```json
{
  "requested_scopes": [
    "skill.install",
    "workspace.write"
  ],
  "writes": [
    {
      "target": "agent.skills",
      "mode": "create_only"
    }
  ],
  "secrets": {
    "contains_secrets": false,
    "redacted_fields": []
  }
}
```

权限规则：

- `requested_scopes` 必须是已知 scope
- `writes[].target` 只能是上层应用定义的逻辑目标，不能是本机绝对路径
- `mode` 第一版只接受 `create_only` 或 `manual_import`
- `contains_secrets=true` 的 bundle 第一版必须拒绝导入
- 缺少 `permissions.json` 时可以保存 bundle，但不能导入

第一版建议 scope：

| Scope | 含义 |
| --- | --- |
| `skill.install` | 安装 skill |
| `session.import` | 导入会话 |
| `workspace.import` | 导入工作区快照 |
| `agent_profile.import` | 导入 Agent 配置 |
| `config.import` | 导入应用配置 |

## 路径安全

bundle 路径必须沿用 NekoDrop 当前 manifest 的安全原则，并且更严格。

必须拒绝：

- 空路径
- 绝对路径
- `.` 或 `..` 片段
- 反斜杠
- NUL 字符
- Windows 保留设备名
- 以空格或点结尾的路径片段
- 包含 `< > : " | ? *` 或控制字符的路径片段
- 指向 `bundle.json`、`checksums.json`、`permissions.json` 之外的根级未知文件

接收端只能把 bundle 保存到应用控制的 staging 目录。桌面端可以在用户确认后把 payload 导入到 NekoDrop 的本机导入区；写入上层应用真实配置仍必须由上层应用或授权 bridge 完成。

## 接收流程

第一版接收流程：

1. 接收普通 NekoLink transfer offer
2. 识别根目录中是否存在 `bundle.json`
3. 校验 `bundle.json`、`checksums.json` 和 `permissions.json`
4. 校验所有 payload 文件路径、大小和 SHA-256
5. 展示 bundle 类型、来源、文件数、大小和权限
6. 用户选择保存或拒绝
7. 保存到 staging 目录
8. 用户可以选择保存、删除，或导入到本机导入区
9. 上层应用或授权 bridge 再次确认后，才允许写入真实应用目录

接收端不能在第 7 步之前修改上层应用配置，也不能自动写入真实应用目录。

## 与现有文件传输的关系

bundle 第一版应复用现有文件传输能力：

- `FileManifest` 继续表示文件树
- `TransferOffer` 继续表达传输前预览
- encrypted `session.control` 继续承载 offer / accept / decline
- encrypted session 路径继续承载加密 file frames
- 敏感 bundle 进入默认主流程前，需要完成长期身份认证和导入确认

bundle 不应该把 NekoDrop 文件传输改成上层应用专用通道。普通文件夹发送仍然是普通文件夹发送；只有符合本规格并通过校验的目录才被识别为 bundle。

## 与本机 local bridge 的关系

local bridge 依赖 bundle，而不是替代 bundle。

bridge 可以做：

- 创建 bundle
- 请求发送 bundle
- 订阅收到 bundle 的通知
- 请求导入 staging bundle
- 查询可信设备和传输状态

第一版 bridge 合约先落在 `nekolink-protocol`：

- `LocalBridgeRequest`
  - `devices.list`
  - `authorization.request`
  - `bundle.send`
  - `bundle.detail`
  - `bundle.import`
  - `bundle.rollback`
  - `transfer.status`
  - `events.poll`
  - `actions.results`
- `LocalBridgeEvent`
  - `bundle.received`
  - `bundle.send.preflight`
  - `action.updated`
  - `transfer.updated`

这些是本机 API 的稳定 JSON 模型。桌面端已有只绑定 `127.0.0.1` 的 localhost runtime，可以处理只读请求、授权申请和授权后的事件轮询；已授权的 `bundle.send`、`bundle.import`、`bundle.rollback` 会进入桌面端内存待执行队列，设置页可以查看并移除这些待执行动作。后台 worker 会按 FIFO 自动消费队列。`bundle.send` 执行前会做 preflight，并交给现有发送主线；执行要求有目标设备，并按可信设备和 bundle 校验结果决定是否继续。`bundle.import` 会把 staged bundle 导入 NekoDrop 本机导入区，默认拒绝覆盖，也支持 `rename` 和 `skip_conflicts`。导入成功会写入本机 import receipt，记录目标目录、策略、实际导入和跳过的 payload 路径；storage 层可以基于 receipt 生成回滚计划，也能执行保守撤回。撤回只删除 receipt 中本次导入的文件；`skip_conflicts` 跳过的既有文件不会被删除。冲突仍会标记为 `bundle_import_conflict`。动作生命周期会通过 `action.updated` 暴露 `queued`、`running`、`succeeded`、`failed`、`conflict`、`cancelled`。

bridge 请求可以带可选 `client`：

```json
{
  "client_id": "generic-agent-app",
  "display_name": "Generic Agent App",
  "app_kind": "agent"
}
```

`client` 只表示本机调用方自报身份，方便 UI 和日志说明来源。它不是授权凭证，不能证明调用方可信，也不能绕过用户确认。

本机应用需要写入或导入前，应先发 `authorization.request` 说明想要的能力：

- `device.read`
- `transfer.status.read`
- `bundle.read`
- `bundle.send`
- `bundle.import.request`

授权请求必须带 `client`、`requested_scopes`、`reason`，可以带 `ttl_seconds`。这一步只定义申请模型，不发 token，也不写入授权记录。

桌面端现在有一个只绑定 `127.0.0.1` 的 localhost runtime，可以处理 `devices.list`、`bundle.detail` 和 `transfer.status` 的只读快照，但这些只读请求也必须有对应 scope：`device.read`、`bundle.read` 或 `transfer.status.read`。`bundle.detail` 会优先看 staged bundle；如果暂存已不在但本机导入区有 import receipt，也会返回已导入、可撤回或已撤回状态。`authorization.request` 会返回申请的 scope、reason、ttl 和短授权码；设置页可以确认授权码，runtime 会记录该 client 的限时权限并写入本机授权文件，下次启动只恢复未过期授权。设置页也可以查看、撤销和清理这些本机授权。真实发送/接收主流程会向 runtime 内存队列写入 `transfer.updated`，收到 staged bundle 时会写入 `bundle.received`。已授权 client 可以通过 `events.poll` 轮询这些事件：`bundle.read` 可读 `bundle.received`，`transfer.status.read` 可读 `transfer.updated`，`bundle.send` 可读 `bundle.send.preflight` 和发送动作的 `action.updated`，`bundle.import.request` 可读导入和撤回动作的 `action.updated`；`timeout_ms` 可用于短等待，默认仍是快照。已授权 client 调用 `bundle.send` / `bundle.import` / `bundle.rollback` 时，runtime 会把请求放入内存待执行队列，后台 worker 会自动执行；设置页可以查看待授权、待执行、最近结果和失败原因，也可以移除队列项。`bundle.send` 会 preflight 后发送到可信目标；`bundle.import` 会导入到 NekoDrop 本机导入区，支持 `reject`、`rename`、`skip_conflicts`，成功后写入 import receipt，并返回可撤回文件数预览；`bundle.rollback` 按 bundle id 找最新 receipt，保守删除本次导入的文件。已授权 client 也可以用 `actions.results` 查询自己的动作结果；传 `action_request_id` 时只查指定动作，不传时按时间游标返回最近结果；结果按 client 和 scope 过滤，不返回本机 `bundle_root`。响应会标记 `read_only`、`requires_user_confirmation` 或 `authorized`。它不是局域网服务，也不会绕过用户确认去发送或写入第三方应用目录。

上层应用和 bundle 的适配边界见 [ADAPTER_SPEC.md](ADAPTER_SPEC.md)。NekoLink bundle 保持通用，不绑定某个具体应用；具体应用只在 adapter 层处理自己的导出和导入。

bridge 不可以做：

- 直接写入对方设备文件系统
- 绕过 bundle 权限导入
- 绕过 NekoLink session 自己建立网络连接
- 把本机任意目录打成 bundle
- 自动打包 token、密钥或隐私文件

## 与 iroh / relay / P2P 的关系

iroh / relay / P2P 只能替换 transport，不能改变 bundle 语义。

未来关系应保持为：

```text
bundle
  -> NekoLink session
  -> TCP transport or iroh transport
```

如果同一个 bundle 在 TCP 和 iroh 上行为不同，说明 transport 边界泄漏到了应用层，需要先修 NekoLink 抽象。

## 第一版实现切片

建议按这个顺序实现：

1. `nekolink-protocol` 增加 bundle manifest 类型和校验
2. `nekodrop-storage` 增加 bundle 目录识别
3. `nekodrop-storage` 增加 staging 保存和校验
4. `nekodrop-service` 在接收完成后产生 bundle detected 事件
5. 桌面 UI 只展示 bundle 预览和保存状态
6. 本机 local bridge 增加 bundle 发送和接收通知
7. 上层应用 adapter 接入真实导入确认和第三方应用写入回滚
8. 本机 local bridge 再接导入动作

第一版测试必须覆盖：

- 合法 bundle 通过校验
- 未知 `bundle_type` 拒绝导入
- 路径穿越被拒绝
- checksum 缺失或不匹配被拒绝
- `contains_secrets=true` 被拒绝
- `summary` 和文件列表不一致被拒绝
- 缺少 `permissions.json` 时只能保存，不能导入

## 完成标准

规格完成后，下一步实现必须满足：

- 能构造一个 `skill` bundle manifest
- 能从已接收目录识别 bundle
- 能校验路径、大小、checksum 和权限
- 能保存到 staging 目录
- 不会自动导入或执行任何上层数据
- 不要求 iroh / relay / P2P

这些条件满足后，才进入本机 local bridge。

## 下一步顺序

当前不要继续扩大 bundle spec。实现顺序固定为：

```text
BundleManifest 校验
-> bundle 目录识别
-> staging 保存
-> UI 预览
-> 手动导入确认
-> local bridge 发送/接收
```

等这个闭环跑起来，再根据真实上层应用使用场景补 `nekolink.bundle.v2`。
