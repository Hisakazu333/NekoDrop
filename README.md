# NekoDrop

NekoDrop 是 NekoLink 生态的第一个落地产品：一个面向 macOS、Windows 和后续手机端的本地优先文件互传工具。

它的目标不是做网盘、聊天软件或远程桌面，而是从“稳定好用的跨设备文件投递”切入，逐步孵化出 NekoLink 个人设备可信通信协议，为后续 OpenNeko Agent、NekoState 状态同步和多设备 AI 伴侣生态打基础。

```text
NekoLink  = 个人设备可信通信协议
NekoDrop  = 第一个落地产品：文件互传
NekoState = 跨设备状态同步层
OpenNeko  = 上层 AI 桌面伴侣 / Agent 运行时
```

## 产品边界

NekoDrop 负责：

- 附近设备发现
- 文件 / 文件夹互传
- 接收确认
- 传输进度
- SHA-256 校验
- 连接码兜底
- 后续断点续传和历史记录

NekoDrop 不负责：

- 云盘
- 聊天
- 远程桌面
- 账号系统
- OpenNeko Agent 编排
- NekoState 状态同步

这些能力会由 NekoLink、NekoState 和 OpenNeko 分层承担。

## 当前状态

当前版本已经接入真实能力：

- Tauri 2 桌面端
- React + TypeScript + Vite UI
- Rust workspace
- 稳定本机设备身份
- NekoLink Envelope
- 连接码真实 TCP 文件传输
- 传输 offer / accept / decline
- 文件 / 文件夹 manifest
- SHA-256 校验
- 真实传输进度、速度和 ETA
- 启动后自动打开后台收件
- mDNS/DNS-SD 附近设备发现，基于开源库 `mdns-sd`
- 附近设备列表
- 点附近设备发送
- 连接码兜底
- macOS 打包脚本
- Win11 打包脚本

当前仍在迭代：

- 可信配对
- 加密 session
- 设备离线过期
- Windows 防火墙提示
- 断点续传
- 传输历史
- 手机端接入
- iroh transport
- Relay / P2P

## 技术栈

```text
Desktop shell  Tauri 2
Frontend       React + TypeScript + Vite
Core           Rust
Discovery      mdns-sd
Transfer       TCP fallback
Protocol       NekoLink Envelope
Packaging      Tauri bundle
```

## 项目结构

```text
NekoDrop/
  apps/
    desktop/
      src/              # React UI
      src-tauri/        # Tauri / Rust desktop layer

  crates/
    nekolink-protocol/  # NekoLink protocol primitives
    nekodrop-core/      # Product domain model
    nekodrop-network/   # TCP frames, connection ticket, transfer protocol
    nekodrop-service/   # File transfer service flow
    nekodrop-storage/   # Manifest, checksum, receive path, file write

  docs/
    MODULES.md
    modules/
```

## 文档入口

- [模块边界](docs/MODULES.md)
- [NekoLink 协议层](docs/modules/NEKOLINK.md)
- [NekoDrop 产品层](docs/modules/NEKODROP.md)
- [发现与传输层](docs/modules/DISCOVERY_AND_TRANSPORT.md)
- [OpenNeko 生态层](docs/modules/OPENNEKO_ECOSYSTEM.md)
- [模块化路线图](docs/modules/MODULE_ROADMAP.md)
- [后续迭代计划](docs/FUTURE_ITERATION_PLAN.md)
- [第三方声明](THIRD_PARTY_NOTICES.md)

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
PATH="/opt/homebrew/opt/rustup/bin:$PATH" cargo test --workspace
```

桌面开发运行：

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" npm --workspace apps/desktop run tauri:dev
```

注意：NekoDrop 必须运行在 Tauri 桌面端中。浏览器预览不能代替桌面软件，因为文件选择、传输、设备发现和系统目录能力都依赖 Tauri/Rust。

## macOS 打包

生成 `.app`：

```bash
./scripts/package-desktop.sh
```

生成 `.app` 和 DMG：

```bash
./scripts/package-desktop.sh --skip-tests --dmg
```

输出目录：

```text
release/desktop/<时间戳>/
```

安装到本机应用程序：

```bash
cp -R "release/desktop/<时间戳>/bundle/macos/NekoDrop.app" "/Applications/"
```

## Win11 打包

在 Windows 11 PowerShell 中运行：

```powershell
npm install
npm run package:windows -- -SkipTests -Bundles nsis
```

默认脚本：

```powershell
npm run package:windows
```

输出目录：

```text
release\desktop\<时间戳>\
```

优先运行脚本最后打印的安装器：

```text
bundle\nsis\...\setup.exe
bundle\msi\...\*.msi
```

不要把开发模式里的 `127.0.0.1:1420` 当成正式安装包。正式 Tauri build 应该加载内置前端资源。

## 当前主流程

推荐主流程：

```text
两台电脑打开 NekoDrop
  -> 后台自动打开收件
  -> mDNS 自动发现附近设备
  -> 选择文件或文件夹
  -> 点击附近设备发送
  -> 接收端确认
  -> 传输并校验
```

兜底流程：

```text
自动发现失败
  -> 打开兜底连接码
  -> 对方粘贴连接码发送
```

## 路线图

当前重点是 V0.6：

```text
V0.6 Auto Discovery
  mDNS 发现
  附近设备列表
  点设备发送
  连接码兜底
  Windows 防火墙提示
  设备离线过期
```

后续：

```text
V0.7 Transport Abstraction
V0.8 Encrypted Session
V0.9 Transfer Reliability
V1.0 Desktop Stable
V1.1 Mobile Companion
V1.2 iroh Transport
V1.4 NekoState
V1.5 OpenNeko Agent Integration
```

## 开源策略

建议采用 open-core：

```text
开源：
  NekoLink protocol
  NekoLink Rust SDK
  NekoDrop 基础互传产品

保留产品壁垒：
  OpenNeko Live2D 资产
  Agent 产品编排
  官方 Relay 服务
  商业化插件和角色
```

推荐许可证方向：

```text
NekoLink: MIT OR Apache-2.0
NekoDrop: Apache-2.0 或 MIT OR Apache-2.0
```

## 开发原则

- 不写 mock 设备。
- 不写 fake 历史。
- 不写 fake 进度。
- UI 状态必须来自真实服务。
- 未完成能力必须标记为待接入。
- 连接码是兜底，不是最终主流程。
- 发现和传输分层，不用扫描传输端口冒充发现。
- OpenNeko 不直接依赖 NekoDrop 内部实现，而是通过 NekoLink 调用能力。
