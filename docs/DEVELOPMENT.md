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

当前还没有接入长期身份密钥认证、接收端 streaming 解密、iroh 真实运行时、Relay / P2P、手机端互传主流程、公开 local bridge runtime 和 Agent 指令通道。界面和文档应将这些能力标记为规划中或实验中，不应把占位数据描述为真实桌面能力。

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

本项目使用 GitHub Flow。`main` 必须始终保持可构建、可测试、可打包；功能、修复、安全收口、UI 改动和发布打包都必须开短分支，通过 PR 合并。

分支命名示例：

```text
fix/windows-path-encoding
hardening/security-reliability
feat/large-file-scan-status
ui/desktop-style-refresh
docs/release-checklist
```

每个 PR 只做一类改动。不要把 UI 大改、安全修复、大文件传输和打包发布混在一个 PR 里。提交信息使用 Conventional Commits，例如：

```text
fix: preserve windows file picker paths
feat: show large file scan status
security: harden transfer frame validation
docs: add release checklist
```

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
5. 收口 encrypted session 后续安全项：接收端 streaming 解密、长期身份密钥认证、legacy plain 路径策略。
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
