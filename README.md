# NekoDrop

[![CI](https://github.com/Hisakazu333/NekoDrop/actions/workflows/ci.yml/badge.svg)](https://github.com/Hisakazu333/NekoDrop/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

NekoDrop 是 NekoLink 的第一个桌面落地项目。

NekoLink 想解决的问题不是“再做一个传文件按钮”，而是让多台本地设备之间有一套稳定的通信底座：设备身份、可信配对、加密会话、可校验资料包、本机应用接入和 transport 边界。NekoDrop 先用 macOS / Windows 桌面传输把这套底座跑起来。

当前 beta 能直接使用的是桌面传输：两台电脑在同一个网络里打开 NekoDrop，选择文件、文件夹或资料包目录，对方确认后开始传输。自动发现失败时，可以用连接码或 `IP:端口` 发送。

后续的 session、skill、workspace、agent profile、应用配置迁移，应该走 NekoLink 的 bundle 和 local bridge，而不是把协议写死到某一个第三方应用里。具体应用只做适配层；底层协议保持应用无关。

## 这个项目不是什么

- 不是云盘，也不保存用户文件到中心服务器。
- 不是只服务某一个插件或第三方应用的私有同步器。
- 不是自己重写加密算法、NAT 打洞库或远程桌面协议。
- 不是收到资料包就自动改本机配置的同步工具。

NekoDrop 当前先把桌面端可确认、可校验、可恢复的传输做好。NekoLink 在这个基础上继续承接跨设备资料包、本机应用接入、后续 transport 和 Agent 协作。

## 当前能用

当前 beta 主线是 macOS / Windows 桌面传输：

- macOS 和 Windows 桌面应用
- 发送单文件、多文件和文件夹
- 附近设备自动发现
- 连接码和 `IP:端口` 兜底
- 接收端确认后再写入本地
- 可信设备配对和设备管理
- 可信设备自动接收只在已认证加密 session 且长期公钥匹配时生效
- 传输进度、速度、预计剩余时间和当前文件
- 大目录 offer 支持
- SHA-256 完整性校验
- 发送和接收取消
- 失败历史、重试和继续发送
- partial/resume 基础
- 手动资料包创建、发送和收到后暂存查看
- macOS DMG、Windows NSIS / MSI 打包脚本

完整状态看 [docs/STATUS.md](docs/STATUS.md)。README 只写能从当前代码和文档里验证的能力。

## NekoLink 走到哪一步

仓库里的 NekoLink 代码已经不只是占位，但也还没有到“跨平台 Agent 网络”的阶段。

| 模块 | 当前状态 |
| --- | --- |
| 桌面传输 | 已接入，当前主要可用能力 |
| encrypted `session.control` | 已接入控制消息，`file.offer` / `file.accept` / `file.decline` 已走加密 session |
| replay-aware control reader | 已接入 offer / decision 控制消息读取路径 |
| 文件 payload 加密 | encrypted session 路径已接入加密 file frames；旧 plain 路径仍保留兼容 |
| 可信设备 key pinning | 已接入；如果对方在可信设备里，authenticated session 必须匹配保存的长期 public key |
| 传输安全状态 | 收件结果和历史会显示已认证加密、已加密或兼容明文；旧历史没有该字段时不猜测 |
| bundle manifest | 已有协议模型、校验、checksums、permissions、staging 和导入计划 |
| 手动资料包 | 已有创建、发送入口、收到后的暂存查看、冲突提示、删除、过期清理和手动导入到本机导入区；legacy plain 只按普通文件保存，不进入导入 staging |
| 自动导出 session / skill / workspace | 未接入 |
| 本机接入 local bridge | 已有协议模型、localhost runtime、权限 scope、只读 handler、设置页自测、授权码确认、限时授权持久化、授权列表、撤销、待执行动作和最近结果状态 |
| local bridge 真实发送 / 导入执行 | 部分接入；bundle.send 可进入桌面发送主线，bundle.import 可导入本机导入区；动作状态可通过 `events.poll` 和 `actions.results` 观察 |
| iroh / relay / P2P | 只有 transport 预留和明确错误，还没有真实运行时 |
| 手机端和 Agent 指令通道 | 未接入 |

这几个边界很重要：现在可以说 NekoLink 的桌面基础在成形，但不能说已经支持跨公网、手机互通或远程 Agent 调用。

## 适合的场景

- Mac 和 Windows 之间传安装包、素材、日志、压缩包或项目目录
- 同一 Wi-Fi 或同一路由器下传大文件
- 不想用 U 盘、网盘、聊天软件中转
- 想在本地设备之间保留可确认、可校验、可恢复的传输记录
- 想为后续 session、workspace、skill、agent profile 迁移准备统一的资料包和接入层

## 不适合的场景

- 云盘同步
- 远程桌面
- 跨公网 P2P
- 游戏联机隧道
- 手机端互传
- 自动同步全部配置、token、密钥或隐私目录

这些方向会影响 NekoLink 的接口设计，但还不是当前 beta 功能。

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

iroh / relay / P2P 会作为 NekoLink transport 接入，但不会直接替换当前桌面传输主线。transport 可以换，上层的身份、session、bundle 和 local bridge 语义不能跟着变。

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

Adapter 边界写在 [docs/ADAPTER_SPEC.md](docs/ADAPTER_SPEC.md)。bundle 样例在 [docs/bundle-samples](docs/bundle-samples/)。

## 后续路线

短期优先级：

1. 上层 adapter 真实样例：用通用接口跑 session、skill、workspace 的导出、发送、暂存、导入和回滚
2. bundle 冲突策略：同名资料先拒绝覆盖，后面补重命名、跳过和上层合并
3. local bridge 事件流：现在已有 `events.poll` 短等待和动作生命周期事件，后面再补真正事件订阅

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

大功能先开 issue 或草案。特别是 bundle 导入策略、上层 adapter、local bridge 事件流、iroh / relay / P2P、手机端和 Agent 指令通道。

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
