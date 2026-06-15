# Adapter 规范

Adapter 是上层应用和 NekoLink bundle 之间的边界。

NekoLink 不理解某个应用的内部数据库，也不应该写死某个应用的目录。应用要把 session、skill、workspace 或 agent profile 传给另一台设备时，先由 adapter 导出成 bundle；接收端再由 adapter 在用户确认后导入。

## 角色

```text
上层应用
  -> adapter 导出
  -> NekoLink bundle
  -> NekoDrop 传输和暂存
  -> adapter 导入
  -> 上层应用
```

NekoDrop 负责传输、校验、暂存、权限展示和本机授权。adapter 负责理解应用自己的数据格式。

## 导出合约

adapter 导出时必须生成一个完整 bundle 目录：

```text
bundle.json
checksums.json
permissions.json
files/
```

导出规则：

- 必须选择一个已知 `bundle_type`
- 必须只写入 `files/` 下的相对路径
- 必须计算真实 SHA-256 和文件大小
- 必须写 `permissions.json`
- 必须声明逻辑写入目标，例如 `agent.sessions`，不能写本机绝对路径
- 必须在导出前移除 token、密钥、cookie、SSH key、账户私密标识和机器本地路径
- 如果无法确认敏感字段已经移除，必须设置 `contains_secrets=true`

`contains_secrets=true` 的 bundle 可以保存和预览，但不能自动导入。

## 导入合约

adapter 导入时只接收 NekoDrop 已经校验并暂存的 bundle。

导入规则：

- 必须再次检查 `bundle_type`
- 必须检查 `permissions.json`
- 必须让用户确认写入目标
- 必须使用临时目录或事务式写入
- 导入失败不能留下半成品配置
- 同名 session、skill、workspace 或 profile 必须走覆盖、重命名或跳过策略，不能静默覆盖

adapter 不应该从任意路径读取 bundle。真实导入入口应来自 NekoDrop staging 或授权 local bridge。

## Local Bridge 请求

本机应用通过 local bridge 请求发送 bundle 时，只提交请求：

```json
{
  "kind": "bundle.send",
  "payload": {
    "request_id": "request-001",
    "client": {
      "client_id": "generic-agent-app",
      "display_name": "Generic Agent App",
      "app_kind": "agent"
    },
    "target_device_id": "neko-device-target",
    "bundle_root": "/path/to/exported/bundle",
    "bundle_type": "workspace",
    "require_trusted_device": true
  }
}
```

`client_id` 是本机应用自报身份，不是凭证。发送、导入这类动作必须先通过授权码确认，并且授权可以在设置页撤销。当前桌面端会把已授权的发送或导入请求放进待执行队列，设置页可以查看和移除。内部 worker 可以取出 `bundle.send` 动作并做 preflight：确认 `bundle_root` 存在、bundle 校验通过、请求里的 `bundle_type` 和 manifest 一致，并在 `require_trusted_device=true` 时确认目标设备已经可信。preflight 通过只表示可以进入桌面发送 worker，不代表文件已经发出。

## 类型建议

| 类型 | adapter 应放入 | adapter 不应放入 |
| --- | --- | --- |
| `skill` | skill manifest、源码、资源索引 | 安装脚本自动执行权限、账号 token |
| `session` | 已脱敏会话摘要、上下文片段 | provider token、cookie、完整账号标识 |
| `workspace` | 用户明确选择的文件片段和索引 | 整个用户目录、隐藏密钥目录 |
| `agent_profile` | 角色偏好、能力声明、非敏感配置 | 私钥、云端 refresh token |
| `config_snapshot` | 可迁移的应用设置 | 机器本地绝对路径、系统钥匙串引用 |

## 样例

可校验样例放在 [bundle-samples](bundle-samples/)：

- `skill-basic`
- `session-summary`
- `workspace-fragment`
- `agent-profile`
- `config-snapshot`

这些样例使用通用应用名，不绑定任何第三方项目。测试会校验样例的 manifest、checksum、权限和 payload 文件。

## 仍未实现

- 上层应用自动导出
- 上层应用真实导入
- local bridge 真实发送和真实导入
- local bridge 事件订阅
- 跨网络 iroh / relay / P2P 传输

这些能力后面接，但不能改变 adapter 和 bundle 的边界。
