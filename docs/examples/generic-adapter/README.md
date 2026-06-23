# 通用 Adapter 示例

这个示例说明一个本机应用如何接入 NekoDrop / NekoLink bundle。它不绑定任何具体应用。

示例脚本：

```bash
node docs/examples/generic-adapter/generic-adapter.mjs
```

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

可以直接跑：

```bash
node docs/examples/generic-adapter/generic-adapter.mjs export \
  --source ./sample-session \
  --output ./out \
  --bundle-id bundle_session_demo \
  --type session \
  --name "Session demo" \
  --source-app "Generic Adapter" \
  --strip-field token
```

脚本会复制 `--source` 下的普通文件，生成 `bundle.json`、`checksums.json` 和 `permissions.json`。JSON 文件可以用 `--strip-field token` 这种参数删掉敏感字段；嵌套字段用点号，例如 `auth.refresh_token`。

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

也可以让脚本生成请求：

```bash
node docs/examples/generic-adapter/generic-adapter.mjs request auth
```

NekoDrop 会返回短授权码。用户在设置 -> 接入里确认后，后续请求才会进入待执行队列。

如果只想看完整接入顺序，可以让脚本生成一组通用请求：

```bash
node docs/examples/generic-adapter/generic-adapter.mjs workflow \
  --mode roundtrip \
  --bundle-root /absolute/path/to/exported-bundle \
  --target-device-id neko-device-target \
  --staged-bundle-id bundle_1234567890 \
  --type session
```

输出顺序固定为：

```text
authorize -> send -> observe -> inspect -> import -> results
```

真实应用可以拆开执行这些步骤；不需要把自己的数据目录暴露给 NekoDrop。

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

脚本生成：

```bash
node docs/examples/generic-adapter/generic-adapter.mjs request send \
  --bundle-root /absolute/path/to/exported-bundle \
  --target-device-id neko-device-target \
  --type session
```

如果已经知道 local bridge 端口，也可以直接 POST：

```bash
node docs/examples/generic-adapter/generic-adapter.mjs post send \
  --port 47321 \
  --bundle-root /absolute/path/to/exported-bundle \
  --target-device-id neko-device-target \
  --type session
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

脚本生成：

```bash
node docs/examples/generic-adapter/generic-adapter.mjs request results
```

结果里的 `lifecycle_status` 可能是：

- `queued`
- `running`
- `succeeded`
- `failed`
- `conflict`
- `cancelled`

旧字段 `status` 仍会保留给兼容代码。新 adapter 应优先读 `lifecycle_status`。

导入结果还会带：

- `conflict_strategy`
- `skipped_file_count`
- `import_receipt_path`

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

响应里除了 `events` 数组，还会带：

- `events_last_id`
- `events_next_after_id`
- `events_has_more`
- `events_cursor_state`

adapter 下一次请求可以把 `events_next_after_id` 放回 `after_event_id`。如果 `events_has_more=true`，应继续拉下一页，不要等下一轮定时器。

`events_cursor_state` 有三个常见值：

- `ok`：cursor 有效，可以继续使用 `events_next_after_id`。
- `empty`：当前没有事件，保留原来的 cursor 或继续用 `null`。
- `missing`：传入的 `after_event_id` 已经不在 NekoDrop 的事件队列里，通常是本地应用停太久或队列被裁剪。adapter 应丢弃旧 cursor，从 `after_event_id=null` 重新拉快照。

脚本可以从一次 bridge response 中取下一次 cursor：

```bash
node docs/examples/generic-adapter/generic-adapter.mjs cursor \
  --response bridge-events-response.json
```

这只是本机短等待，不是公网长连接。

## 导入

接收端 adapter 不直接从任意路径导入。它先请求 NekoDrop 导入 staged bundle 到本机导入区：

导入前可以先读取 staged bundle 详情：

```bash
node docs/examples/generic-adapter/generic-adapter.mjs request detail \
  --staged-bundle-id bundle_1234567890
```

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
    "expected_bundle_type": "session",
    "conflict_strategy": "reject"
  }
}
```

脚本生成：

```bash
node docs/examples/generic-adapter/generic-adapter.mjs request import \
  --staged-bundle-id bundle_1234567890 \
  --type session \
  --conflict-strategy reject
```

`conflict_strategy` 支持：

- `reject`：默认，不覆盖，冲突时返回 `bundle_import_conflict`
- `rename`：导入到新的目标目录
- `skip_conflicts`：已有文件不覆盖，只补缺失文件

如果同名 bundle 已经存在，adapter 应先让用户选择策略，不要静默覆盖。

这一步仍然不是“写进上层应用目录”。NekoDrop 只负责把 staged bundle 校验后放到本机导入区；上层应用自己的 adapter 再读取导入区内容，按自己的数据模型落库、合并或回滚。

导入成功后，NekoDrop 会在本机导入区写一条 import receipt。它记录目标目录、导入策略、实际导入和跳过的 payload 路径，方便 adapter 后续做导入确认、回滚或冲突处理。

## 样例边界

这个脚本只演示通用接入方式：

- 生成合法 bundle
- 生成 local bridge 请求
- 可选 POST 到本机 bridge

它不会读取某个第三方应用目录，也不会把导入结果写回某个应用。真实产品应在自己的 adapter 里完成“从应用导出”和“导入回应用”的部分。
