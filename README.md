# NekoDrop

NekoDrop 是一个 macOS / Windows 局域网文件互传桌面应用。当前仓库同时包含 NekoLink 协议雏形；NekoDrop 是第一个使用它的产品。

当前主线目标很窄：两台桌面电脑在同一局域网里发现、配对、发送文件或文件夹，接收端确认后写入文件，并完成 SHA-256 校验和历史记录。

## 当前状态

可用能力以 [docs/STATUS.md](docs/STATUS.md) 为准。当前已经接入：

- Tauri 2 桌面端，React/Vite 界面，Rust workspace
- 文件和文件夹选择
- 发送前 manifest 扫描、文件数/大小/当前路径状态
- TCP 文件传输
- transfer offer / accept / decline
- 传输进度、速度、ETA、当前文件
- SHA-256 完整性校验
- 发送中取消、接收中取消
- 接收端磁盘空间预检
- partial/resume 基础和接收端 resume 明细
- 短暂网络失败自动重试一次
- 连接码和 `IP:端口` 兜底
- mDNS / DNS-SD 附近设备发现
- 发现状态、接收端口、局域网地址诊断
- Windows 中文路径编码防护
- 稳定设备 ID 和 fingerprint
- 可信配对基础、可信设备管理
- 持久化传输历史，支持打开位置、重发、继续发送、删除、清空
- 独立设置页，管理收件目录、端口、接收策略和收件开关
- macOS DMG 打包脚本
- Windows NSIS / MSI 打包脚本

当前不能当作已完成能力宣传：

- iroh 真实运行时
- Relay / P2P / NAT 打洞
- 加密 session
- 手机端互传主流程
- OpenNeko Agent 指令通道
- NekoState 状态同步
- 系统级 Windows 防火墙自动配置
- 云账号、云盘或中心化文件存储

## 使用方式

两台电脑都打开 NekoDrop。

正常流程：

```text
附近设备出现
选择文件或文件夹
点击目标设备
对方确认接收
传输完成后校验并写入历史
```

自动发现失败时：

```text
接收端复制连接码
发送端粘贴连接码
选择文件或文件夹
发送
```

手动输入框也支持 `IP:端口`，用于排查局域网、VPN、代理、虚拟网卡或 Windows 防火墙问题。

## 本地开发

安装依赖：

```bash
npm install
```

前端构建：

```bash
npm run build
```

Rust 测试：

```bash
cargo test --workspace
```

运行桌面开发模式：

```bash
npm --workspace apps/desktop run tauri:dev
```

不要只打开 Vite 浏览器页面验证功能。文件选择、后台接收、系统托盘、Tauri 命令和安装包行为都必须在桌面运行时里测。

## 打包

macOS：

```bash
./scripts/package-desktop.sh --skip-tests --dmg
```

输出目录：

```text
release/desktop/<timestamp>/
```

Windows 11：

```powershell
npm run package:windows -- -SkipTests -Bundles nsis
```

输出目录：

```text
release\desktop\<timestamp>\
```

发布安装包时必须记录 SHA256。不要从临时脏工作区打包 release。

## 发布前验证

合并前至少跑：

```bash
cargo fmt --all -- --check
cargo test --workspace
npm run build
npm audit --omit=dev
npm run security:audit
git diff --check
```

安装包发布前按 [docs/testing/LARGE_FILE_TRANSFER_MATRIX.md](docs/testing/LARGE_FILE_TRANSFER_MATRIX.md) 记录结果。内测版至少要覆盖 P0；公开 beta 要覆盖 P0/P1，并写出安装包路径、SHA256、系统版本、网络状态和失败说明。

## 安全审计

`npm audit` 当前检查 npm 依赖。`npm run security:audit` 会额外检查支持发布的 Rust 目标：

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `x86_64-pc-windows-msvc`

GitHub Dependabot 会扫描完整 `Cargo.lock`，其中包含 Linux GTK/WebKit 依赖。当前已知的 `glib 0.18.5` moderate 告警来自 Tauri 的 Linux WebKit 链路，不在 macOS/Windows 发布目标图里。处理规则见 [docs/SECURITY.md](docs/SECURITY.md)。

## 仓库结构

```text
apps/
  desktop/              Tauri 桌面端和 React UI
  sidecar/              后台进程实验入口

crates/
  nekolink-protocol/    NekoLink 消息、能力、设备身份、配对、文件 offer
  nekodrop-core/        产品领域模型、manifest、pairing、transfer
  nekodrop-network/     mDNS、连接码、TCP、transport 抽象
  nekodrop-service/     文件发送和接收流程
  nekodrop-storage/     文件写入、checksum、partial/resume

docs/                   产品、架构、协议、安全、路线图、测试矩阵
scripts/                macOS / Windows 打包和审计脚本
```

## 文档

- [当前状态](docs/STATUS.md)
- [开发说明](docs/DEVELOPMENT.md)
- [安全模型](docs/SECURITY.md)
- [架构](docs/ARCHITECTURE.md)
- [Roadmap](docs/ROADMAP.md)
- [Future Iteration Plan](docs/FUTURE_ITERATION_PLAN.md)
- [测试矩阵](docs/testing/LARGE_FILE_TRANSFER_MATRIX.md)
- [测试结果模板](docs/testing/RESULT_TEMPLATE.md)
- [模块边界](docs/MODULES.md)

## 常见问题

### 附近设备不出现

先查这些：

- 两台设备是否在同一局域网
- Windows 是否允许 NekoDrop 访问专用网络
- macOS 是否允许本地网络访问
- VPN、代理、虚拟网卡是否影响了局域网地址
- 公司、校园或访客网络是否屏蔽 mDNS
- 有线和无线是否在不同网段

临时路径是连接码或 `IP:端口`。

### 看到 `198.18.x.x`

`198.18.x.x` 常见于代理、测试网络或虚拟网卡，不适合做局域网互传地址。关闭 VPN、代理或虚拟网卡后重启应用。

### 现在用了 iroh 吗

没有。当前真实传输主线是 TCP。iroh / Relay / P2P 只有 transport 抽象和明确未接入错误。

## 许可

本仓库使用 [Apache License 2.0](LICENSE)。

第三方依赖和声明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
