# 贡献代码

NekoDrop 还在 beta 收口。先别从大功能下手。

最适合的 PR 是小的：文档、测试、错误提示、打包脚本、边界清楚的 protocol / storage 改动。

这个仓库不是云盘，也不是远程控制工具。当前主线是 macOS / Windows 局域网文件互传。NekoLink 是后面给 OpenNeko、CCS、bundle、P2P 复用的通信层。

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

从 `main` 开短分支：

```bash
git checkout main
git pull --ff-only
git checkout -b docs/fix-status-note
```

分支名前缀建议：

```text
docs/
fix/
feat/
ui/
security/
hardening/
test/
```

一个 PR 只做一件事。不要把文档、UI、协议、安全、打包混在一起。

PR 描述写清楚：

- 改了什么
- 为什么改
- 没做什么
- 怎么验证
- 是否需要重新打安装包

默认 squash merge。

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
