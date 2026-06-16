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
- local bridge 协议模型和内部只读 handler skeleton

当前还没有接入长期身份密钥认证、iroh 真实运行时、Relay / P2P、手机端互传主流程、公开 local bridge runtime 和 Agent 指令通道。界面和文档应将这些能力标记为规划中或实验中，不应把占位数据描述为真实桌面能力。

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

本项目使用 `main / develop / desktop-develop / personal dev branch / topic branch` 的开发流程。

```text
Rust / 核心能力 -> dev/<name> -> develop
桌面端能力 -> dev/<name> 或短分支 -> desktop-develop
desktop-develop -> develop -> main -> tag / release
```

`main` 是发布主线，必须始终保持可构建、可测试、可打包。它只接收从 `develop` 发起的 release / rollup PR。

`develop` 是核心功能集成分支，主要放 Rust workspace、协议、存储、网络、服务、安全、bundle 和 bridge 这些底层功能。它不是个人开发分支，不接收没完成的半成品。

`desktop-develop` 是桌面端集成分支，主要放 Tauri、React UI、桌面 IPC、设置页、安装包脚本和 macOS / Windows 体验。桌面端后续开发都从这个分支开短分支，做完后 PR 回 `desktop-develop`。

`dev/<name>` 是个人长期开发分支。当前维护者使用 `dev/hisakazu`。日常代码先在个人分支提交，再通过 PR 合进 `develop` 或 `desktop-develop`。个人分支可以长期保留，不能在 PR 合并时删除。

不要把普通功能分支直接合到 `main`。紧急 hotfix 如果必须从 `main` 开，合并后要同步回 `develop`，涉及桌面端的还要同步回 `desktop-develop`。

每周至少检查一次 `develop -> main`。有可发布改动就开 release / rollup PR；没有可发布改动就跳过，并在项目记录里写清楚。桌面端要发版时，先把 `desktop-develop` 同步进 `develop`，再走 `develop -> main`。

分支命名示例：

```text
dev/hisakazu
dev/<name>
fix/hisakazu/windows-path-encoding
hardening/hisakazu/security-reliability
feat/hisakazu/large-file-scan-status
ui/hisakazu/desktop-style-refresh
bridge/hisakazu/localhost-runtime
bundle/hisakazu/import-rollback
docs/hisakazu/release-checklist
```

第一次创建个人开发分支：

```bash
git checkout develop
git pull --ff-only
git checkout -b dev/hisakazu
git push -u origin dev/hisakazu
```

日常继续写 Rust / 核心功能：

```bash
git checkout dev/hisakazu
git pull --ff-only
git merge --ff-only origin/develop
```

桌面端功能可以从 `desktop-develop` 开短分支，也可以在个人分支里做完后拆 PR。跨层改动要拆成两段：Rust / 协议先进 `develop`，桌面 UI 再进 `desktop-develop`。

每个 PR 只做一类改动。不要把 UI 大改、安全修复、大文件传输和打包发布混在一个 PR 里。提交信息使用 Conventional Commits，例如：

```text
fix: preserve windows file picker paths
feat: show large file scan status
security: harden transfer frame validation
docs: add release checklist
```

合并规则：

- 日常 PR 合到 `develop`
- 发布 PR 从 `develop` 合到 `main`
- 合并前必须通过 CI
- 日常 PR 默认 squash merge
- 合并后的 topic branch 要删除
- 个人长期分支和集成分支不能删除，尤其是 `dev/<name>`、`develop` 和 `desktop-develop`
- 不允许 force push 到 `main` 或 `develop`
- 如果 `develop -> main` 使用 rebase 合并，合并后把 `develop` 同步到 `main` 的发布点

合并前至少跑：

```bash
cargo fmt --all -- --check
cargo test --workspace
npm run build
npm audit --omit=dev
npm run security:audit
git diff --check
```

Release 安装包必须从 tag 对应代码构建，不从临时工作区随手打包。预览版 tag 使用 `v0.1.0-preview.N` 形式，发布资产需要同时写出 DMG / Windows 安装包的 SHA256。完整规范见仓库根目录 `CONTRIBUTING.md`。

## 实现顺序

1. 保持 `nekodrop-core` 作为产品模型源头。
2. 在 `nekodrop-storage` 中实现文件 / 文件夹 manifest 扫描。
3. 保持发现状态、错误恢复和跨 Mac / Windows 真实验证。
4. 继续打磨可信设备发送和连接码兜底的操作路径。
5. 收口 encrypted session 后续安全项：长期身份密钥认证、legacy plain 路径策略。
6. 完善 NekoLink bundle staging、导入确认和失败回滚。
7. 接本机 local bridge runtime 和授权，再做 iroh / Relay / P2P transport 验证。

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
