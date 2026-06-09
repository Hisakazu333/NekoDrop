# NekoDrop

NekoDrop 是一个 macOS / Windows 桌面端局域网文件互传软件。目标不是网盘、聊天工具或远程控制，而是做一个电脑之间私有、快速、可信的文件投递工具。

第一版目标：

- 自动发现同一局域网内的电脑
- 两台电脑首次配对确认
- 可信设备列表
- 选择文件或文件夹发送
- 接收端确认
- 分块传输和进度显示
- 完成后校验文件
- 默认保存到本机接收目录

## 范围

MVP 包含：

- macOS 和 Windows 桌面客户端
- 局域网设备发现
- 首次配对和信任关系
- 文件 / 文件夹发送
- 接收确认
- 分块传输
- 进度和历史记录
- 校验和验证

MVP 不包含：

- 账号系统
- 云同步
- 公网中继
- 移动端
- 浏览器接收端
- 双向文件夹同步

## 技术栈

- 桌面壳：Tauri 2
- 核心语言：Rust
- 前端界面：React + TypeScript + Vite
- 传输：局域网 TCP
- 发现：mDNS，后续可补 UDP broadcast fallback
- 安全：设备配对、可信设备、传输会话加密

## 项目结构

```text
NekoDrop/
  apps/
    desktop/
      src/
      src-tauri/
  crates/
    nekodrop-core/
    nekodrop-network/
    nekodrop-storage/
  docs/
    PRODUCT.md
    ARCHITECTURE.md
    PROTOCOL.md
    SECURITY.md
    ROADMAP.md
```

## 文档

- [Product Definition](docs/PRODUCT.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Protocol](docs/PROTOCOL.md)
- [Security](docs/SECURITY.md)
- [Roadmap](docs/ROADMAP.md)
- [Development Notes](docs/DEVELOPMENT.md)

## 当前工程状态

当前已经进入连接码真实传输版本：

- Rust workspace 核心 crate
- Tauri 桌面端源码
- React/Vite 桌面 WebView 界面
- Tauri IPC 命令读取真实应用状态
- 选择文件 / 文件夹后生成真实 manifest 和 SHA-256
- 接收端打开真实 TCP 监听并生成连接码
- 发送端粘贴连接码后发起真实传输 offer
- 接收端在 App 内接受 / 拒绝
- 传输过程中展示真实进度、速度和 ETA
- 接收完成后按 SHA-256 校验文件
- 收件监听可以手动关闭

当前还没有接入设备发现、可信配对、历史记录和 OpenNeko 支撑层。界面只保留这些后续入口为“待接入”，不会生成假设备、假 Windows 电脑或假传输记录。

检查命令：

```bash
cargo check --workspace
cargo test --workspace
npm run build
PATH="/opt/homebrew/opt/rustup/bin:$PATH" npm --workspace apps/desktop run tauri:dev
```
