# NekoDrop

[![CI](https://github.com/Hisakazu333/NekoDrop/actions/workflows/ci.yml/badge.svg)](https://github.com/Hisakazu333/NekoDrop/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

NekoDrop 是一个 macOS / Windows 桌面文件互传工具。

两台电脑在同一个局域网里打开 NekoDrop，选择文件或文件夹，点附近设备，对方确认后开始传输。文件不先上传到云盘，也不需要塞进聊天软件。自动发现失败时，可以用连接码或 `IP:端口` 发送。

这个仓库也在实现 NekoLink。NekoLink 是底层通信层，负责设备身份、可信配对、会话控制消息、bundle 包格式和 transport 抽象。NekoDrop 是第一个桌面落地项目，不是唯一目标。

## 当前能用

当前 beta 主线是桌面局域网互传：

- macOS 和 Windows 桌面应用
- 发送单文件、多文件和文件夹
- 附近设备自动发现
- 连接码和 `IP:端口` 兜底
- 接收端确认后再写入本地
- 可信设备配对和设备管理
- 传输进度、速度、预计剩余时间和当前文件
- 大目录 offer 支持
- SHA-256 完整性校验
- 发送和接收取消
- 失败历史、重试和继续发送
- partial/resume 基础
- macOS DMG、Windows NSIS / MSI 打包脚本

完整状态看 [docs/STATUS.md](docs/STATUS.md)。README 只写能从当前代码和文档里验证的能力。

## 已经走到哪一步

NekoDrop 不只是在做“传文件按钮”。最近几轮已经把后续 NekoLink 能力接进仓库，但有些还没有进用户主流程。

| 模块 | 当前状态 |
| --- | --- |
| 桌面 LAN 互传 | 已接入，当前主要可用能力 |
| encrypted `session.control` | 已接入控制消息，`file.offer` / `file.accept` / `file.decline` 已走加密 session |
| replay-aware control reader | 已接入 offer / decision 控制消息读取路径 |
| 文件 payload 加密 | encrypted session 路径已接入加密 file frames；旧 plain 路径仍保留兼容 |
| bundle manifest | 已有协议模型、校验、checksums、permissions 和 staging |
| 手动资料包 | 已有创建、发送入口和收到后的暂存查看 |
| 自动导出 session / skill / workspace | 未接入 |
| 本机接入 local bridge | 已有协议模型、权限 scope、只读 handler 和设置页自测 |
| local bridge runtime / 授权码 / 持久化授权 | 未接入 |
| iroh / relay / P2P | 只有 transport 预留和明确错误，还没有真实运行时 |
| 手机端和 Agent 指令通道 | 未接入 |

这几个边界很重要：现在可以说 NekoLink 的桌面基础在成形，但不能说已经支持跨公网、手机互通或远程 Agent 调用。

## 适合的场景

- Mac 和 Windows 之间传安装包、素材、日志、压缩包或项目目录
- 同一 Wi-Fi 或同一路由器下传大文件
- 不想用 U 盘、网盘、聊天软件中转
- 想在本地网络里保留可确认、可校验、可恢复的传输记录

## 不适合的场景

- 云盘同步
- 远程桌面
- 跨公网 P2P
- 游戏联机隧道
- 手机端互传
- 自动同步全部配置、token、密钥或隐私目录

这些方向会影响 NekoLink 的设计，但还不是当前 beta 功能。

## 下载

发布包放在 [GitHub Releases](https://github.com/Hisakazu333/NekoDrop/releases)。

下载后建议核对 SHA256：

```bash
shasum -a 256 NekoDrop_0.1.0_aarch64.dmg
```

Windows 用 PowerShell：

```powershell
Get-FileHash .\NekoDrop_0.1.0_x64-setup.exe -Algorithm SHA256
```

## 使用

1. 在两台电脑上打开 NekoDrop
2. 确认两台电脑在同一个局域网
3. 在发送页选择文件或文件夹
4. 选择附近设备，或粘贴接收端连接码
5. 接收端确认
6. 等待传输完成和校验通过

附近设备没有出现时，让接收端复制连接码，发送端粘贴后发送。

## 常见问题

### 附近设备不出现

先检查这些地方：

- 两台设备是否在同一个局域网
- Windows 网络是否设为专用网络
- macOS 是否允许本地网络访问
- VPN、代理、虚拟网卡是否改变了局域网地址
- 访客网络、校园网或公司网络是否隔离了设备
- 路由器是否屏蔽 mDNS / DNS-SD

短期处理方式：用连接码，或者输入 `IP:端口`。

### Windows 路径出现乱码

NekoDrop 已经把 Windows 文件选择脚本改成 UTF-8 输出。遇到乱码路径时，不要继续发送那条路径，重新从文件选择器选择文件。

如果问题重复出现，优先检查系统区域设置、终端编码和第三方脚本是否改写了路径字符串。

### 大文件能不能传

可以传大文件和文件夹。当前已有扫描状态、进度、速度、历史记录、取消和 partial/resume 基础。

后续还会继续打磨失败恢复，例如更清楚的重试、继续发送和备用码路径。

### 现在能不能跨网络

不能。当前可用主线是同局域网 TCP。

iroh / relay / P2P 会作为 NekoLink transport 接入，但要排在加密文件流、bundle 和本机接入之后。

## 本地开发

准备依赖：

```bash
npm install
```

构建前端：

```bash
npm run build
```

运行 Rust 测试：

```bash
cargo test --workspace
```

启动桌面开发模式：

```bash
npm --workspace apps/desktop run tauri:dev
```

不要只打开 Vite 浏览器页验证桌面功能。文件选择、接收服务、系统权限、托盘、安装包和本地网络行为都要在 Tauri 桌面运行时里测。

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

发布前记录这些信息：

- 安装包路径
- SHA256
- 操作系统版本
- 分支和 commit
- 实机传输结果

## 仓库结构

```text
apps/
  desktop/              Tauri 桌面端和 React UI
  sidecar/              后台进程实验入口

crates/
  nekolink-protocol/    NekoLink 协议类型、session、bundle、local bridge 模型
  nekodrop-core/        设备、manifest、pairing、transfer 领域模型
  nekodrop-network/     mDNS、连接码、TCP、transport 抽象
  nekodrop-service/     发送和接收流程
  nekodrop-storage/     文件写入、checksum、partial/resume、bundle staging

docs/                   状态、协议、安全、路线图和测试记录
scripts/                打包和审计脚本
```

## 后续路线

短期优先级：

1. 长期身份认证：把可信设备和 session 绑定到长期身份密钥
2. legacy plain 路径策略：明确旧明文兼容路径什么时候迁移或拒绝
3. bundle 闭环：完善 staging 生命周期、预览、导入确认和失败回滚
4. 本机接入：让本机应用通过受控 API 请求发送和导入 bundle

中长期方向：

- iroh / relay / P2P transport
- Android、iOS、OpenHarmony 和 Linux 客户端
- 跨设备 Agent 节点协作
- 应用状态和工作区在多设备之间迁移
- 游戏联机、陪玩或远程协作这类上层能力

这些能力会基于 NekoLink 做，不会写死到某一个第三方应用里。

## 贡献

先读 [CONTRIBUTING.md](CONTRIBUTING.md)。

适合先做的小 PR：

- 修文档过期状态
- 补协议、storage、路径安全测试
- 改平台错误提示
- 修打包脚本
- 给已有 bug 写复现测试

大功能先开 issue 或草案。特别是文件流加密、local bridge 鉴权、bundle 导入、iroh / relay / P2P、手机端和 Agent 指令通道。

合并前常用检查：

```bash
cargo fmt --all -- --check
cargo test --workspace
npm run build
npm audit --omit=dev
npm run security:audit
git diff --check
```

## 文档

- [当前状态](docs/STATUS.md)
- [开发说明](docs/DEVELOPMENT.md)
- [安全模型](docs/SECURITY.md)
- [架构](docs/ARCHITECTURE.md)
- [协议](docs/PROTOCOL.md)
- [Bundle 规格](docs/BUNDLE_SPEC.md)
- [Roadmap](docs/ROADMAP.md)
- [下一阶段分析](docs/NEXT_PHASE_ANALYSIS.md)
- [模块边界](docs/MODULES.md)
- [测试矩阵](docs/testing/LARGE_FILE_TRANSFER_MATRIX.md)

## 许可

Apache License 2.0。见 [LICENSE](LICENSE)。

第三方依赖和声明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
