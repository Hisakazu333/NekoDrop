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

如果要看从导出到撤回的完整闭环，用 `full-loop`：

```bash
node docs/examples/generic-adapter/generic-adapter.mjs workflow \
  --mode full-loop \
  --source ./sample-workspace \
  --output ./out \
  --bundle-id bundle_workspace_demo \
  --name "Workspace demo" \
  --target-device-id neko-device-target \
  --staged-bundle-id bundle_workspace_demo \
  --type workspace \
  --conflict-strategy rename \
  --strip-field auth.token
```

输出步骤：

```text
export -> authorize -> send -> observe_send -> send_results
-> inspect_received_bundle -> import -> observe_import
-> inspect_after_import -> import_results -> rollback
-> observe_rollback -> rollback_results
```

这不是让一个脚本跨两台机器自动跑完。真实 adapter 应该按设备拆开：

- 发送端：导出 bundle，申请授权，请求 `bundle.send`，观察发送结果。
- 接收端：收到 staged bundle 后先 `bundle.detail`，再请求 `bundle.import`，观察导入结果。
- 需要撤回时：用 `bundle.rollback` 撤回 NekoDrop 本机导入区里的文件。

`skill`、`session`、`workspace`、`agent_profile` 都按敏感资料处理：发送端必须要求可信目标，接收端只有 authenticated encrypted session 收到的 bundle 才会进入暂存和导入流程。旧兼容路径收到的目录即使有 `bundle.json`，也只当普通文件保存。

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
    "action_request_id": "adapter-import-001",
    "after_claimed_at_ms": null,
    "limit": 20
  }
}
```

脚本生成：

```bash
node docs/examples/generic-adapter/generic-adapter.mjs request results \
  --action-request-id adapter-import-001
```

`request_id` 是这次查询请求本身的 id。`action_request_id` 是之前 `bundle.send`、`bundle.import` 或 `bundle.rollback` 的请求 id，用来精确查询某个动作结果。不传 `action_request_id` 时，NekoDrop 按 `after_claimed_at_ms` 和 `limit` 返回一组最近结果。精确查询时，如果还没有终态结果，但动作仍在同一个 client 的待执行队列里，响应会返回脱敏的 `queued`；如果 worker 已经写入执行状态，则返回 `running` 或终态结果。

也可以让示例脚本把精确查询响应归类：

```bash
node docs/examples/generic-adapter/generic-adapter.mjs action-state \
  --response bridge-results-response.json \
  --action-request-id adapter-import-001
```

返回的 `state` 只有四类：

- `pending`：`lifecycle_status=queued`，NekoDrop 已接手，还没开始执行
- `running`：worker 正在执行
- `result`：动作已有终态结果
- `missing`：当前 client 查不到这个动作，可能是 request id 不对、权限不够、动作属于别的 client，或结果已被清理

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
- `has_import_receipt`
- `rollback_file_count`
- `can_request_rollback`
- `rollback_blocking_reason`
- `rolled_back_file_count`

普通 bridge response 不返回 NekoDrop 本机 `import_receipt_path`。adapter 应用 `has_import_receipt` 和 `can_request_rollback` 判断是否可以请求 `bundle.rollback`，不要依赖本机私有路径。

常见 `reason`：

- `bundle_root_missing`
- `bundle_invalid`
- `bundle_type_mismatch`
- `trusted_target_missing`
- `bundle_send_failed`
- `bundle_import_receipt_missing`
- `bundle_rollback_blocked`
- `bundle_rollback_failed`
- `bundle_import_conflict`
- `bundle_import_failed`

当 `reason` 是 `bundle_rollback_blocked` 时，`rollback_blocking_reason` 会说明阻断类型。当前可能值：

- `destination_missing`
- `imported_file_missing`
- `already_rolled_back`

这些值只用于决定下一步提示，不包含本机路径。

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

事件处理建议：

- `action.updated` 里的 `request_id` 是最稳定的关联键。
- `queued` / `running` 只表示 NekoDrop 已接手动作，不代表业务完成。
- `succeeded` 后再用 `actions.results.action_request_id` 查这次动作结果，拿 `has_import_receipt`、`can_request_rollback`、`rollback_file_count`、`rolled_back_file_count`。
- `conflict` 时先读 `bundle.detail`，让用户选 `rename` 或 `skip_conflicts`，不要自动覆盖。
- cursor 丢失时从 `after_event_id=null` 重新拉快照，再用本机保存的 request_id 作为 `action_request_id` 对齐结果。

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

导入成功后，NekoDrop 会在本机导入区写一条 import receipt。它记录目标目录、导入策略、实际导入和跳过的 payload 路径。NekoDrop 可以用它生成回滚计划，也可以执行保守撤回：只删除本次 import receipt 记录的导入文件，`skip_conflicts` 跳过的既有文件不会被删除。

生成撤回请求：

```bash
node docs/examples/generic-adapter/generic-adapter.mjs request rollback \
  --bundle-id bundle_1234567890
```

```json
{
  "kind": "bundle.rollback",
  "payload": {
    "request_id": "adapter-rollback-001",
    "client": {
      "client_id": "generic.adapter.sample",
      "display_name": "Generic Adapter Sample",
      "app_kind": "generic"
    },
    "bundle_id": "bundle_1234567890"
  }
}
```

撤回只作用于 NekoDrop 本机导入区，不会撤销上层应用已经落库或合并的数据。真实 adapter 如果把 bundle 写进自己的应用目录，需要自己记录导入结果并做回滚。

## 样例边界

这个脚本只演示通用接入方式：

- 生成合法 bundle
- 生成 local bridge 请求
- 可选 POST 到本机 bridge

它不会读取某个第三方应用目录，也不会把导入结果写回某个应用。真实产品应在自己的 adapter 里完成“从应用导出”和“导入回应用”的部分。
