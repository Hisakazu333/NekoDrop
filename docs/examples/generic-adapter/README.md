# 通用 Adapter 样板

这个目录提供一个可执行的最小样板，说明本机应用怎样围绕 NekoLink bundle 走完整流程。

## 文件

```text
generic-adapter.mjs
generic-adapter.test.mjs
```

## 流程

1. 导出一个已经校验过的 bundle 目录。
2. 生成 `authorization.request`、`bundle.send`、`bundle.detail`、`events.poll`、`actions.results`、`bundle.import` 请求。
3. 先看 `bundle.detail` 的只读预览，再看 `actions.results` 和 `events.poll` 的状态词。
4. 导入后写 receipt。
5. 需要撤销时删除导出目录。

## 运行

```bash
node docs/examples/generic-adapter/generic-adapter.mjs export --out /tmp/generic-adapter-bundle
node docs/examples/generic-adapter/generic-adapter.mjs plan --bundle /tmp/generic-adapter-bundle
node docs/examples/generic-adapter/generic-adapter.mjs receipt --bundle /tmp/generic-adapter-bundle --receipt-out /tmp/generic-receipt.json
node docs/examples/generic-adapter/generic-adapter.mjs rollback --bundle /tmp/generic-adapter-bundle
```

## 样板约定

- `bundle.detail` 只做只读预览，预览状态是 `saved`；导入后的 receipt 用 `imported`。
- `actions.results` 和 `events.poll` 共享同一组动作状态词：`queued`、`running`、`succeeded`、`failed`、`conflict`、`cancelled`。
- `bundle.import` 只接受已经暂存的 bundle，不直接写第三方应用目录。
- `rollback` 只清理导出目录，不撤销 NekoDrop 侧已经完成的导入动作。

## 说明

这个样板只用通用应用名，不绑定任何第三方项目。它的目标是让真实 adapter 的最小实现有一个稳定参照，不是新增一套协议。
