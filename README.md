# NekoDrop

macOS / Windows 桌面文件互传工具。

当前重点是桌面端互传：发现设备、选择文件或文件夹、对方确认、传输、校验、写入历史。

## 当前状态

详细状态以 [docs/STATUS.md](docs/STATUS.md) 为准。README 只放入口信息。

已接入：

- macOS / Windows Tauri 桌面端
- 文件和文件夹选择
- mDNS / DNS-SD 局域网发现
- 连接码兜底发送
- 传输 offer / accept / decline
- TCP 文件传输
- SHA-256 校验
- 进度、速度、ETA
- 发送中取消、接收中取消
- partial offset 断点续传基础
- 接收目录持久化
- 传输历史
- 设备身份和可信配对基础
- 设备页、历史页、设置/诊断页
- NekoLink envelope、协议 crate、transport 抽象和 TCP transport
- macOS DMG 打包脚本
- Windows NSIS / MSI 打包脚本

未接入：

- iroh / Relay / P2P 真实运行时
- 加密 session
- 手机端互传主流程
- OpenNeko Agent 指令通道
- NekoState 同步
- Apple Developer ID 签名和 notarization
- Windows 防火墙自动配置
- 云账号、云盘、中心化文件存储

## 本地运行

安装依赖：

```bash
npm install
```

构建前端：

```bash
npm run build
```

运行桌面开发模式：

```bash
npm --workspace apps/desktop run tauri:dev
```

运行 Rust 测试：

```bash
cargo test --workspace
```

不要只用浏览器打开 Vite 页面判断可用性。文件选择、后台接收、系统能力和安装包行为都需要 Tauri 桌面运行时。

## 打包

macOS：

```bash
./scripts/package-desktop.sh --skip-tests --dmg
```

输出目录：

```text
release/desktop/<timestamp>/
```

脚本会构建 `.app`，做 ad-hoc 签名，校验签名，再生成 DMG 并执行 `hdiutil verify`。

当前 DMG 不是正式签名发布包。公开分发前还需要 Apple Developer ID 签名和 notarization。

Windows 11：

```powershell
npm run package:windows -- -SkipTests -Bundles nsis
```

输出目录：

```text
release\desktop\<timestamp>\
```

优先使用脚本输出的安装器，不要把开发模式 exe 当正式包。

## 安装包校验

macOS DMG：

```bash
hdiutil verify release/desktop/<timestamp>/bundle/dmg/NekoDrop_0.1.0_aarch64.dmg
codesign --verify --deep --strict release/desktop/<timestamp>/bundle/macos/NekoDrop.app
shasum -a 256 release/desktop/<timestamp>/bundle/dmg/NekoDrop_0.1.0_aarch64.dmg
```

GitHub Release 使用的安装包应该从 tag 对应代码构建。

## 仓库结构

```text
apps/
  desktop/              React + Tauri 桌面端
  sidecar/              后台进程实验入口

crates/
  nekolink-protocol/    协议消息、设备身份、配对、文件 offer
  nekodrop-core/        产品领域模型
  nekodrop-network/     发现、连接码、TCP、transport 抽象
  nekodrop-service/     发送和接收流程
  nekodrop-storage/     文件写入、checksum、partial/resume

docs/                   状态、架构、协议、安全、路线图
scripts/                macOS / Windows 打包脚本
```

## 开发流程

本仓库按 GitHub Flow 走：

```text
main
  -> feature/fix/docs 分支
  -> PR
  -> 检查通过
  -> merge 回 main
  -> 必要时从 tag 打包
```

`main` 应保持可构建、可打包。规则见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 下一步

近期优先级：

1. 合并当前桌面 UI、打包和文档分支。
2. 从 main/tag 重新打 macOS 预览 DMG。
3. 做 Win11 安装和 Mac <-> Win 互传验证。
4. 补大文件传输矩阵：中文路径、文件夹、取消、失败重试、续传。
5. 收口加密 session 设计和实现。
6. iroh / Relay / P2P 先做 spike，不替换 TCP 主线。

## 文档

- [当前状态](docs/STATUS.md)
- [架构](docs/ARCHITECTURE.md)
- [开发说明](docs/DEVELOPMENT.md)
- [协议草案](docs/PROTOCOL.md)
- [安全模型](docs/SECURITY.md)
- [路线图](docs/ROADMAP.md)
- [第三方声明](THIRD_PARTY_NOTICES.md)

## 许可

源代码和仓库内文档使用 [Apache License 2.0](LICENSE)。
