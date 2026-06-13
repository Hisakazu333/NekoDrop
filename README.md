# NekoDrop

[![CI](https://github.com/Hisakazu333/NekoDrop/actions/workflows/ci.yml/badge.svg)](https://github.com/Hisakazu333/NekoDrop/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

NekoDrop 是一个给 macOS 和 Windows 用的本地文件互传工具。

两台电脑在同一个网络里打开应用，选择文件或文件夹，点一下附近设备，对方确认后开始传输。文件不经过云盘，也不需要先发到聊天软件里。

它也是 NekoLink 的第一个桌面实现：今天先用 TCP 把同网段传输跑稳，后面把 iroh、Relay 和 P2P 接到同一套设备身份、配对、会话加密和传输抽象里。NekoDrop 是用户能直接使用的产品，NekoLink 是它下面那层跨设备连接能力。

长期看，NekoLink 要解决的不是“再做一个传文件按钮”，而是让不同设备、不同网络里的应用节点可以安全互通。文件互传只是第一个闭环；同一套底座后面可以承载应用多端协同、Agent 节点调用、跨设备状态同步、远程任务执行，甚至游戏联机这类需要稳定点对点连接的上层场景。

## 现在能做什么

- 在同一局域网里发现附近的 NekoDrop 设备
- 发送单个文件、多个文件或整个文件夹
- 接收端确认后再写入本地目录
- 显示传输进度、速度、ETA 和当前文件
- 传输完成后做 SHA-256 校验
- 记录传输历史，支持重发和继续发送
- 自动发现失败时，用连接码或 `IP:端口` 兜底
- 支持可信设备配对和基础设备管理
- 支持 macOS DMG、Windows NSIS / MSI 打包

完整状态看 [docs/STATUS.md](docs/STATUS.md)。

## 下载安装

发布包会放在 GitHub Releases：

- [Releases](https://github.com/Hisakazu333/NekoDrop/releases)

下载后建议核对 SHA256。项目每次打包时都会记录安装包路径和哈希值，例如：

```bash
shasum -a 256 NekoDrop_0.1.0_aarch64.dmg
```

Windows 可以用 PowerShell：

```powershell
Get-FileHash .\NekoDrop_0.1.0_x64-setup.exe -Algorithm SHA256
```

## 快速使用

1. 在两台电脑上打开 NekoDrop。
2. 确认两台电脑在同一个局域网里。
3. 选择文件或文件夹。
4. 在附近设备里选择目标电脑。
5. 接收端确认。
6. 等待传输完成和校验通过。

如果附近设备没有出现，接收端复制连接码，发送端粘贴连接码后也可以发送。

## 常见问题

### 附近设备不出现

优先检查这几件事：

- 两台设备是否在同一个局域网
- Windows 是否允许应用访问“专用网络”
- macOS 是否允许本地网络访问
- VPN、代理、虚拟网卡是否改写了本机局域网地址
- 公司、校园、访客网络是否屏蔽 mDNS / DNS-SD
- 有线和无线是否被路由器隔在不同网段

临时解决方式是用连接码，或者直接输入 `IP:端口`。

### Windows 中文路径乱码

桌面端会尽量保护 Windows 文件选择输出的 UTF-8 编码。遇到类似 `I:\�ļ�\...` 的路径时，不要继续发送这个路径；重新选择文件，并检查系统区域、终端或脚本输出是否破坏了路径编码。

### 传大文件可以吗

可以传大文件和文件夹。当前已有扫描状态、传输进度、历史记录和 partial/resume 基础。失败恢复入口还会继续打磨，例如更明确的重试、继续发送和备用码路径。

### 现在支持跨网络 P2P 吗

不支持。当前真实主线是同局域网 TCP 传输。

NekoLink 里已经有 transport 抽象和 iroh / Relay / P2P 的位置，但还没有接入真实运行时。等这层打通后，NekoDrop 才能从“同网段传文件”继续走向“不同网络、不同设备之间的连接”；游戏联机、跨网络应用节点互通、Agent 跨设备调用都属于这层能力之上的场景。

## 本地开发

准备依赖：

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

桌面开发模式：

```bash
npm --workspace apps/desktop run tauri:dev
```

不要只打开 Vite 浏览器页面验证功能。文件选择、后台接收、系统托盘、Tauri 命令、安装包和本地网络行为都必须在桌面运行时里测。

## 打包

macOS DMG：

```bash
./scripts/package-desktop.sh --dmg
```

Windows 安装包：

```powershell
npm run package:windows -- -Bundles nsis
```

输出目录：

```text
release/desktop/<timestamp>/
```

发布前至少记录：

- 安装包路径
- SHA256
- 操作系统版本
- 代码分支和 commit
- 实机传输结果

## 合并前检查

```bash
cargo fmt --all -- --check
cargo test --workspace
npm run build
npm audit --omit=dev
npm run security:audit
git diff --check
```

大文件、Mac -> Windows、Windows -> Mac 的结果记录在 [docs/testing/LARGE_FILE_TRANSFER_MATRIX.md](docs/testing/LARGE_FILE_TRANSFER_MATRIX.md)。

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

## 路线图

当前重点：

- 把桌面端 LAN TCP 传输继续做稳
- 收紧失败后的重试、继续发送和连接码兜底路径
- 改进附近设备、可信设备和历史地址发送体验
- 补齐发布记录、测试矩阵和安装包校验信息

接下来会压的底层能力：

- 加密 session 接入桌面主线
- replay 保护和加密文件流
- iroh / Relay / P2P transport 验证
- 为手机端、OpenHarmony、应用节点互通、Agent 调用和游戏联机这类上层场景留出稳定接口

路线图看 [docs/ROADMAP.md](docs/ROADMAP.md)，真实完成状态以 [docs/STATUS.md](docs/STATUS.md) 为准。

## 文档

- [当前状态](docs/STATUS.md)
- [开发说明](docs/DEVELOPMENT.md)
- [安全模型](docs/SECURITY.md)
- [架构](docs/ARCHITECTURE.md)
- [协议](docs/PROTOCOL.md)
- [Roadmap](docs/ROADMAP.md)
- [模块边界](docs/MODULES.md)
- [测试矩阵](docs/testing/LARGE_FILE_TRANSFER_MATRIX.md)

## 许可

Apache License 2.0。见 [LICENSE](LICENSE)。

第三方依赖和声明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
