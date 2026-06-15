# 贡献代码

NekoDrop 还在 beta 收口。先别从大功能下手。

最适合的 PR 是小的：文档、测试、错误提示、打包脚本、边界清楚的 protocol / storage 改动。

这个仓库不是云盘，也不是远程控制工具。NekoDrop 是 NekoLink 的第一个桌面落地项目；当前 beta 可用主线是 macOS / Windows 桌面传输。NekoLink 后续承接 bundle、local bridge、transport 和跨设备 Agent 协作，但协议不能写死到某一个第三方应用里。

## 先看这几个文件

第一次进仓库，按这个顺序看：

1. [README.md](README.md)
2. [docs/STATUS.md](docs/STATUS.md)
3. [docs/MODULES.md](docs/MODULES.md)
4. [docs/BUNDLE_SPEC.md](docs/BUNDLE_SPEC.md)
5. [docs/NEXT_PHASE_ANALYSIS.md](docs/NEXT_PHASE_ANALYSIS.md)

`STATUS.md` 说真实完成了什么。README 不能写超过 `STATUS.md` 的能力。

## 适合先做的事

这些任务适合新贡献者：

- 修文档里的错字、过期状态、命令错误
- 补测试，尤其是 `nekolink-protocol` 和 `nekodrop-storage`
- 改小的 UI 文案，不改整体布局
- 补 Windows / macOS 错误提示
- 补打包脚本的小问题
- 给已有 bug 写复现测试

这些任务先开 issue 讨论：

- 文件流加密
- replay protection
- 长期设备身份密钥
- local bridge 鉴权
- bundle 导入流程
- iroh / relay / P2P
- 手机端互通
- Agent 远程调用

这些事情不要直接提 PR：

- 大 UI 重写
- 自己加一个新的传输协议并绕过 NekoLink
- 自动导入 session / skill / workspace
- 默认同步 token、密钥、隐私目录
- 把当前未完成能力写成已完成

## 模块边界

按边界改代码。不要为了方便跨层调用。

```text
crates/nekolink-protocol
  协议类型、消息、能力、bundle manifest、session 控制模型。
  不依赖 Tauri、React、桌面 UI。

crates/nekodrop-storage
  路径安全、checksum、partial 文件、resume、bundle staging。
  不做网络连接，不做 UI 状态。

crates/nekodrop-network
  mDNS、连接码、TCP transport、网络帧。
  不写入最终文件。

crates/nekodrop-service
  发送和接收流程，把 protocol / storage / network 串起来。

apps/desktop
  Tauri 命令、桌面窗口、React UI、系统集成。
```

UI 只展示状态和发命令。文件扫描、hash、接收落盘、信任策略都不应该写在前端。

## 分支和 PR

本仓库使用 `main / develop / desktop-develop / topic branch` 流程。

```text
main
  发布主线。只接收 release / rollup PR。

develop
  核心功能集成分支。主要承接 Rust workspace、协议、存储、网络、服务、安全、bundle、bridge 等底层功能。

desktop-develop
  桌面端集成分支。主要承接 Tauri、React UI、桌面 IPC、设置页、安装包脚本和 macOS / Windows 体验。

topic branch
  每个人自己的短分支。所有实际开发都在 topic branch 里完成。
```

`develop` 和 `desktop-develop` 都不是个人开发分支。不要把没完成的代码直接推到这两个分支。

常规路径：

```text
Rust / 协议 / 存储 / 网络 / 服务 / 安全 / bundle / bridge
  -> 从 develop 开 topic branch
  -> PR 到 develop

桌面端 UI / Tauri / IPC / 设置页 / 安装包 / 平台体验
  -> 从 desktop-develop 开 topic branch
  -> PR 到 desktop-develop

发版收口
  -> desktop-develop 先同步到 develop
  -> develop 再通过 release / rollup PR 合到 main
  -> 从 main 打 tag 和安装包
```

跨层改动要拆开。比如协议字段和桌面 UI 都要改时，先把协议 / Rust 能力合到 `develop`，再把桌面消费逻辑合到 `desktop-develop`。不要把协议、安全、UI 和打包塞进一个 PR。

`main` 是发布主线，只接收从 `develop` 发起的 release / rollup PR。不要把日常功能 PR 直接合进 `main`。每周至少做一次 `develop -> main` 收口；如果这一周没有可发布改动，可以跳过并在项目记录里写清楚。

Rust / 核心功能从 `develop` 开短分支：

```bash
git checkout develop
git pull --ff-only
git checkout -b security/hisakazu/session-policy
```

桌面端功能从 `desktop-develop` 开短分支：

```bash
git checkout desktop-develop
git pull --ff-only
git checkout -b ui/hisakazu/receive-flow-polish
```

分支名前缀建议：

```text
docs/<name>/<topic>
fix/<name>/<topic>
feat/<name>/<topic>
ui/<name>/<topic>
bridge/<name>/<topic>
bundle/<name>/<topic>
security/<name>/<topic>
hardening/<name>/<topic>
test/<name>/<topic>
```

示例：

```text
security/hisakazu/legacy-plain-policy
bridge/hisakazu/localhost-runtime
bundle/hisakazu/import-rollback
ui/hisakazu/transfer-progress
```

一个 PR 只做一件事。不要把文档、UI、协议、安全、打包混在一起。

PR 目标分支：

- Rust / 协议 / 存储 / 网络 / 服务 / 安全 / bundle / bridge：合到 `develop`
- 桌面 UI / Tauri / IPC / 设置页 / 安装包 / 平台体验：合到 `desktop-develop`
- 发布收口、版本 tag 前的最终汇总：从 `develop` 合到 `main`
- 紧急 hotfix：可以从 `main` 开，但合并后必须回灌到 `develop`

合并规则：

- PR 合并前必须通过 CI
- 默认 squash merge 到 `develop`
- `develop -> main` 用仓库允许的方式合并；如果 GitHub 只能 rebase，就在合并后把 `develop` 同步到 `main` 的发布点
- 合并后的 topic branch 要删除
- 长期分支同步 PR 不删除 head 分支：`develop`、`desktop-develop`、`main` 都不能被 PR 合并按钮顺手删除
- 不允许 force push 到 `main` 或 `develop`

PR 描述写清楚：

- 改了什么
- 为什么改
- 没做什么
- 怎么验证
- 是否需要重新打安装包

## 提交信息

用 Conventional Commits：

```text
docs: clarify bundle staging status
fix: reject unsafe staged bundle ids
feat: add staged bundle cleanup
ui: show receive port warning
security: bind session control identity
test: cover windows path fragments
```

## 本地验证

小改动跑对应测试。合并前至少跑：

```bash
cargo fmt --all -- --check
cargo test --workspace
npm run build
npm audit --omit=dev
git diff --check
```

如果本机 `cargo` 走到旧 Rust，使用 stable toolchain：

```bash
rustup run stable env \
  RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
  RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc \
  cargo test --workspace
```

桌面功能不要只测 Vite 页面。涉及文件选择、接收服务、托盘、系统权限、安装包的改动，要跑 Tauri 桌面端。

```bash
npm --workspace apps/desktop run tauri:dev
```

## 发布相关

不要从临时工作区随手打正式包。发布包必须来自 tag。

预览版 tag：

```text
v0.1.0-preview.1
v0.1.0-preview.2
```

发布时至少记录：

- macOS DMG
- Windows NSIS / MSI
- SHA256
- commit
- 已验证的系统版本
- 已知限制

当前还不能叫 stable。文件 payload 加密、replay protection、长期设备身份密钥、跨网络 transport 都还没完成。

## 文档规则

- 新功能合并后先更新 [docs/STATUS.md](docs/STATUS.md)。
- README 只写用户现在能理解和能验证的能力。
- 协议细节写 [docs/PROTOCOL.md](docs/PROTOCOL.md)。
- 安全边界写 [docs/SECURITY.md](docs/SECURITY.md)。
- bundle 规则写 [docs/BUNDLE_SPEC.md](docs/BUNDLE_SPEC.md)。
- 不要把 roadmap 里的东西写成已经完成。
