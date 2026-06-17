# 通用 Adapter 示例

这个示例说明一个本机应用如何接入 NekoDrop / NekoLink bundle。它不绑定任何具体应用。

## 导出

adapter 先把自己的数据导出成一个 bundle 目录：

```text
exported-bundle/
  bundle.json
  checksums.json
  permissions.json
  files/
    session.json
```

导出前必须做两件事：

- 移除 token、cookie、密钥、机器本地路径和账号私密标识。
- 如果不能确认已经脱敏，把 `permissions.json` 里的 `contains_secrets` 设为 `true`，这样接收端只能保存，不能导入。

## 授权

本机应用第一次发送或请求导入前，先申请权限：

```json
{
  "kind": "authorization.request",
  "payload": {
    "request_id": "adapter-auth-001",
    "client": {
      "client_id": "generic.adapter",
      "display_name": "Generic Adapter",
      "app_kind": "agent"
    },
    "requested_scopes": [
      "device.read",
      "bundle.send",
      "bundle.import.request",
      "transfer.status.read"
    ],
    "reason": "Send and import user-selected bundles",
    "ttl_seconds": 3600
  }
}
```

NekoDrop 会返回短授权码。用户在设置 -> 接入里确认后，后续请求才会进入待执行队列。

## 发送

```json
{
  "kind": "bundle.send",
  "payload": {
    "request_id": "adapter-send-001",
    "client": {
      "client_id": "generic.adapter",
      "display_name": "Generic Adapter",
      "app_kind": "agent"
    },
    "target_device_id": "neko-device-target",
    "bundle_root": "/absolute/path/to/exported-bundle",
    "bundle_type": "session",
    "require_trusted_device": true
  }
}
```

请求成功只代表动作入队，不代表已经发送完成。桌面端后台 worker 会自动做 preflight 和真实发送。adapter 优先用 `events.poll` 里的 `action.updated` 观察进度，再用 `actions.results` 查最新结果。

## 查询结果

```json
{
  "kind": "actions.results",
  "payload": {
    "request_id": "adapter-results-001",
    "client": {
      "client_id": "generic.adapter",
      "display_name": "Generic Adapter",
      "app_kind": "agent"
    },
    "after_claimed_at_ms": null,
    "limit": 20
  }
}
```

结果里的 `lifecycle_status` 可能是：

- `queued`
- `running`
- `succeeded`
- `failed`
- `conflict`
- `cancelled`

旧字段 `status` 仍会保留给兼容代码。新 adapter 应优先读 `lifecycle_status`。

常见 `reason`：

- `bundle_root_missing`
- `bundle_invalid`
- `bundle_type_mismatch`
- `trusted_target_missing`
- `bundle_send_failed`
- `bundle_import_conflict`
- `bundle_import_failed`

## 等待事件

`events.poll` 默认立即返回快照。需要减少轮询时，可以加 `timeout_ms`：

```json
{
  "kind": "events.poll",
  "payload": {
    "request_id": "adapter-events-001",
    "client": {
      "client_id": "generic.adapter",
      "display_name": "Generic Adapter",
      "app_kind": "agent"
    },
    "after_event_id": null,
    "limit": 20,
    "timeout_ms": 15000
  }
}
```

`action.updated` 事件会带 `request_id`、`action_kind`、`status`、`reason`、`bundle_id`、`bundle_type` 和 `target_device_id`。事件不会返回本机 `bundle_root`。

这只是本机短等待，不是公网长连接。

## 导入

接收端 adapter 不直接从任意路径导入。它先请求 NekoDrop 导入 staged bundle 到本机导入区：

```json
{
  "kind": "bundle.import",
  "payload": {
    "request_id": "adapter-import-001",
    "client": {
      "client_id": "generic.adapter",
      "display_name": "Generic Adapter",
      "app_kind": "agent"
    },
    "staged_bundle_id": "bundle_1234567890",
    "expected_bundle_type": "session"
  }
}
```

如果同名 bundle 已经存在，NekoDrop 不覆盖，会返回 `bundle_import_conflict`。adapter 需要让用户选择重命名、跳过或合并。
