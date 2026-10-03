# NekoDrop 开发说明

## 当前状态

当前项目是一个真实 Tauri 桌面端局域网互传版本：

- Rust workspace 和核心领域 crate
- Tauri 2 桌面端入口
- React/Vite 桌面 WebView 界面
- Tauri IPC 状态读取命令
- 系统文件 / 文件夹选择
- 文件 manifest 扫描和 SHA-256
- mDNS/DNS-SD 自动发现附近设备
- 发现状态诊断和无设备短提示
- 附近设备状态诊断和离线过期
- 本机可信设备记录和配对码
- 双端配对请求 / 接受 / 拒绝
- TCP 连接码收件监听
- 点附近设备发送，连接码作为兜底
- 传输 offer、接收确认、拒绝和超时
- 真实发送 / 接收进度、速度和 ETA
- 接收文件校验和安全落盘
- 发送中取消
- 发送端瞬时网络失败自动重试
- 接收目录持久化
- 接收端 resume 明细 UI
- 网络/传输错误提示和目标地址预检
- 传输历史持久化
- 历史记录打开位置、重发、继续发送、删除、清空
- NekoLink transport 抽象和 TCP 实现
- 桌面传输 offer / accept / decline 走 encrypted `session.control`
- encrypted session 路径的文件 payload 走加密 file frames
- offer / decision 控制消息读取路径带 replay window
- bundle manifest 校验、手动创建、接收后 staging
- 桌面 session identity 签名校验和可信设备 public key pinning
- local bridge localhost runtime、授权码确认、限时授权持久化
- local bridge `bundle.send` / `bundle.import` 待执行队列、后台 worker、动作生命周期事件

当前还没有接入 iroh 真实运行时、Relay / P2P、手机端互传主流程、上层应用自动导出 / 真实导入、local bridge 长连接事件流和 Agent 指令通道。界面和文档应将这些能力标记为规划中或实验中，不应把占位数据描述为真实桌面能力。

## 本地检查

Rust：

```bash
cargo check --workspace
cargo test --workspace
```

前端和桌面端：

```bash
npm install
npm run build
PATH="/opt/homebrew/opt/rustup/bin:$PATH" npm --workspace apps/desktop run tauri:dev
```

不要把 `npm run dev` 的浏览器页面当作软件运行结果。用户要的是桌面软件，验证时必须启动 Tauri 窗口。

## GitHub 开发流程

自 2026-10 起使用简化的三段式分支模型，替代旧的 `main / develop / desktop-develop / docs-develop / 个人长期分支` 多分支模型：

```text
main                          发布主线（受保护：只接受 PR、squash merge、必须过 CI）
develop                       集成分支，所有日常 PR 的目标
feat|fix|docs|chore/<topic>   从 develop 切出的短生命周期分支
```

```text
topic branch -> develop -> main -> tag / release
```

`main` 是发布主线，必须始终保持可构建、可测试、可打包。它只接收从 `develop` 发起的 release PR（squash merge）。

`develop` 是集成分支，承接所有日常改动：Rust workspace、协议、存储、网络、服务、安全、bundle、bridge、桌面 UI 和文档。它不是草稿区，不接收没完成的半成品。

主题分支从 `develop` 切出，做完 PR 回 `develop`，合并后删除。不再使用个人长期分支；长期不合并的分支会被清理，重要 WIP 先打 `archive/<name>` 标签保留。

日常流程：

```bash
git checkout develop
git pull --ff-only
git checkout -b fix/transfer-timeout
# ... 开发、提交 ...
git push -u origin fix/transfer-timeout
# 开 PR 到 develop，CI 通过后 squash merge 并删除远端分支
```

紧急 hotfix 可以从 `main` 开分支，PR 回 `main`，合并后立刻把 `main` 同步回 `develop`。

每个 PR 只做一类改动。不要把 UI 大改、安全修复、大文件传输和打包发布混在一个 PR 里。提交信息使用 Conventional Commits，例如：

```text
fix: preserve windows file picker paths
feat: show large file scan status
security: harden transfer frame validation
docs: add release checklist
```

合并规则：

- 日常 PR 合到 `develop`；发布 PR 从 `develop` 合到 `main`
- 合并前必须通过 CI；默认 squash merge
- 合并后的 topic branch 要删除
- `main` 不允许 force push；`develop` 仅允许在 release squash 合并后做同步重置
- 每周至少检查一次 `develop -> main`，有可发布改动就开 release PR
- release PR squash 合并后立即把 `develop` 重置到 `main`，避免平行历史导致下一个 release PR 冲突

合并前至少跑：

```bash
cargo fmt --all -- --check
cargo test --workspace
node --test apps/desktop/test/*.test.*
npm run build
npm audit --omit=dev
npm run security:audit
git diff --check
```

Release 安装包必须从 tag 对应代码构建，不从临时工作区随手打包。预览版 tag 使用 `v0.1.0-preview.N` 形式，发布资产需要同时写出 DMG / Windows 安装包的 SHA256。完整规范见仓库根目录 `CONTRIBUTING.md`。

## 结构治理

现在仓库不是重写阶段，但已经有几个明显的热点文件。后续开发不能继续把所有能力塞进这些文件：

```text
apps/desktop/src-tauri/src/commands/mod.rs
crates/nekolink-protocol/src/lib.rs
apps/desktop/src/App.tsx
apps/desktop/src/styles.css
crates/nekodrop-network/src/tcp_file.rs
crates/nekodrop-service/src/lib.rs
```

规则：

- 小 bug 可以就地修。
- 新命令族不要继续塞进 `commands/mod.rs`，先拆到 `commands/<area>.rs`。
- 新页面状态不要继续塞进 `App.tsx`，先拆到 `views/<ViewName>.tsx`。
- 新样式不要继续把全局 CSS 变成杂物间，能按 layout / view / component 拆就拆。
- 新协议模型不要和桌面业务混在一起，先放 `nekolink-protocol`，再由 service/network 调用。
- 新上层应用能力不能写死某个第三方应用，先走 bundle / adapter / local bridge。

拆分不单独追求文件数量。只有在下面情况出现时才拆：

- 新功能会让一个热点文件继续明显变大；
- 一个文件里已经混了两个以上不同职责；
- 测试很难只覆盖这次改动；
- 贡献者必须读无关流程才能改当前功能。

推荐下一阶段先做渐进拆分：

```text
commands/mod.rs
  -> commands/transfer.rs
  -> commands/devices.rs
  -> commands/bundles.rs
  -> commands/bridge.rs
  -> commands/settings.rs
  -> commands/security.rs

App.tsx
  -> views/OverviewView.tsx
  -> views/SendView.tsx
  -> views/ReceiveView.tsx
  -> views/DevicesView.tsx
  -> views/TransfersView.tsx
  -> views/SettingsView.tsx

nekolink-protocol/src/lib.rs
  -> envelope.rs
  -> identity.rs
  -> session.rs
  -> bundle.rs
  -> bridge.rs
  -> crypto.rs
```

功能 PR 可以顺手做小范围拆分，但不要把一次 PR 变成大搬家。大拆分要单独开 `refactor/...` 分支，保证行为不变、测试先跑通。

## 实现顺序

1. 保持 `nekodrop-core` 作为产品模型源头。
2. 在 `nekodrop-storage` 中实现文件 / 文件夹 manifest 扫描。
3. 保持发现状态、错误恢复和跨 Mac / Windows 真实验证。
4. 继续打磨可信设备发送和连接码兜底的操作路径。
5. 收口 legacy plain 路径策略，继续补 authenticated session 的异常和兼容测试。
6. 完善 NekoLink bundle 导入计划、冲突处理、真实上层 adapter 样例。
7. 打磨 local bridge 事件订阅和动作 UI，再做 iroh / Relay / P2P transport 验证。

## UI 边界

前端应该：

- 渲染状态
- 接收拖拽
- 调用 Tauri 命令
- 订阅传输事件

前端不应该：

- 扫描文件夹
- 计算文件 hash
- 实现传输协议
- 写入接收文件
- 决定信任策略

## Rust 边界

`nekodrop-core`:

- device model
- pairing model
- transfer model
- app config
- manifest model

`nekodrop-storage`:

- safe path handling
- chunk planning
- checksum implementation
- partial file and resume state

`nekodrop-network`:

- discovery
- protocol messages
- client/server transport
- transfer session framing

`apps/desktop/src-tauri`:

- command bridge
- tray/window behavior
- platform-specific integration
- service lifecycle

## Sidecar CLI（`apps/sidecar`）

不依赖桌面的命令行收发，适合脚本与远程机器：

```bash
cargo run -p nekodrop-sidecar -- plan <path> [path...]        # 预演：列出文件/大小/SHA-256
cargo run -p nekodrop-sidecar -- receive 0.0.0.0:0 <目录>      # 监听并打印连接码，收一个传输
cargo run -p nekodrop-sidecar -- send <host:port|连接码> <路径> # 发送文件/目录
cargo run -p nekodrop-sidecar -- text <host:port|连接码> <文本> # 文本快送（暂存 .txt 后走加密通道）
```

`text` 与桌面端共用 `nekodrop-service::stage_text_snippet`（2 MB 上限、同秒防覆盖），发送完成后自动清理暂存文件。
