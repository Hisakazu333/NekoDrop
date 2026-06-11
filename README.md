# NekoDrop

> 本地优先的跨设备文件互传桌面软件，也是 NekoLink 个人设备网络的第一个落地产品。

NekoDrop 的目标很明确：让 Mac、Windows，后续再加手机、平板、NAS 和 OpenNeko Agent 节点，能在一个可信设备网络里传文件、同步状态、下发任务。

当前版本先把第一件事做实：桌面端之间的真实文件互传。

```text
打开 NekoDrop
  -> 自动发现附近设备
  -> 选择文件或文件夹
  -> 点设备发送
  -> 对方确认接收
  -> 传输、显示进度、完成校验
  -> 进入历史记录
```

NekoDrop 不是网盘，不是聊天软件，也不是浏览器页面。它是一个 Tauri 桌面应用，依赖本地文件选择、后台接收、局域网发现、桌面安装包和系统能力。

## 当前状态

当前阶段：桌面互传 MVP + NekoLink 协议雏形。

已经真实接入：

- macOS / Windows 桌面端工程
- 启动后自动打开后台接收服务
- mDNS / DNS-SD 局域网自动发现
- 发现状态诊断和无设备短提示
- 附近设备列表和离线过期
- 连接码兜底发送
- 文件和文件夹选择
- transfer offer / accept / decline
- 真实传输进度、速度、ETA
- 发送中取消
- 接收中取消
- TCP partial offset 断点续传基础
- 接收端 resume 明细 UI
- 接收目录持久化
- 网络/传输错误提示和目标地址预检
- manifest 和 SHA-256 校验
- 持久化传输历史
- 历史记录打开位置、重发、继续发送、删除、清空
- 稳定设备 ID 和设备指纹
- 可信配对基础：配对请求、配对码、接受、拒绝、忘记设备
- 设备页：附近设备、可信设备
- macOS DMG 打包脚本
- Win11 NSIS / MSI 打包脚本
- NekoLink 协议 crate
- NekoLink transport 抽象和 TCP 实现

还没有接入：

- iroh / Relay / P2P 真实运行时
- 加密 session
- 失败后自动重试
- 手机端主流程
- OpenNeko Agent 指令通道
- NekoState 跨设备状态同步
- 系统级 Windows 防火墙自动配置
- 云账号、云盘、中心化文件存储

## NekoDrop 是什么

NekoDrop 是产品层。

它负责用户看得见、摸得着的文件互传流程：

- 选择文件
- 发现设备
- 配对设备
- 发送文件
- 接收确认
- 展示进度
- 管理历史
- 打包成桌面软件

NekoDrop 不应该吞掉所有能力。长期结构是：

```text
NekoLink   = 可信设备通信协议
NekoDrop   = 第一个落地产品：文件互传
NekoState  = 后续跨设备状态同步层
OpenNeko   = AI 伴侣 / Agent 入口
```

现在这个仓库先用 NekoDrop 孵化 NekoLink。等协议、传输抽象、加密会话、手机端和 OpenNeko 接入点稳定后，再考虑把 NekoLink 拆成独立仓库。

## 为什么做它

普通互传工具只解决“把文件发过去”。

NekoDrop / NekoLink 要解决的是更底层的设备关系：

- 这台电脑是谁
- 哪些设备可信
- 文件该走局域网、Relay 还是 P2P
- 传输是否可校验、可恢复、可追踪
- 手机能不能把任务发给电脑
- OpenNeko Agent 能不能跨设备调用能力

短期目标是一个好用的 Mac / Win 文件互传工具。长期目标是个人设备网络的通信底座。

## 快速开始

### 使用安装包

macOS：

```bash
./scripts/package-desktop.sh --skip-tests --dmg
```

打包完成后，打开 `release/desktop/<时间戳>/bundle/dmg/` 里的 DMG，把 `NekoDrop.app` 拖到 `Applications`。

Windows 11：

```powershell
npm run package:windows -- -SkipTests -Bundles nsis
```

打包完成后，运行脚本输出的 `setup.exe` 安装器。不要直接运行开发模式下的 debug exe。

### 发送文件

两台设备都打开 NekoDrop，尽量处在同一局域网。

正常流程：

```text
附近设备出现
  -> 选择文件或文件夹
  -> 点击目标设备
  -> 对方接受
  -> 开始传输
```

自动发现失败时，用连接码兜底：

```text
接收端查看连接码
  -> 发送端粘贴连接码
  -> 选择文件或文件夹
  -> 发送
```

手动连接框也支持输入 `IP:端口`，用于调试 Mac / Windows 局域网互通。

连接码是兜底，不是最终主流程。主流程应该是自动扫描附近设备、点设备发送。

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

运行桌面开发模式：

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" npm --workspace apps/desktop run tauri:dev
```

注意：不要只用浏览器打开 Vite 页面。浏览器预览不能代表桌面软件，因为文件选择、后台接收、系统托盘、安装包和 Tauri 命令都依赖桌面运行时。

## 打包

### macOS

生成 `.app`：

```bash
./scripts/package-desktop.sh
```

跳过测试：

```bash
./scripts/package-desktop.sh --skip-tests
```

生成 DMG：

```bash
./scripts/package-desktop.sh --skip-tests --dmg
```

输出目录：

```text
release/desktop/<时间戳>/
```

DMG 是安装物。使用 `--dmg` 时，脚本会保留 DMG，清理额外复制出来的 `.app`，避免 Launchpad 里出现重复应用。

### Windows 11

默认打包：

```powershell
npm run package:windows
```

跳过测试并只生成 NSIS 安装器：

```powershell
npm run package:windows -- -SkipTests -Bundles nsis
```

输出目录：

```text
release\desktop\<时间戳>\
```

优先安装脚本最后打印出来的安装器：

```text
bundle\nsis\...\setup.exe
bundle\msi\...\*.msi
```

如果 Windows 上看到 `127.0.0.1:1420` 白屏，通常说明运行到了开发形态，不是正式安装包。

## 仓库结构

```text
NekoDrop/
  apps/
    desktop/
      src/              React UI
      src-tauri/        Tauri / Rust 桌面层
    sidecar/            后台进程实验入口

  crates/
    nekolink-protocol/  NekoLink 消息、能力、设备身份、配对、文件 offer
    nekodrop-core/      产品领域模型、manifest、pairing、transfer
    nekodrop-network/   mDNS、连接码、TCP、transport 抽象
    nekodrop-service/   文件发送和接收流程
    nekodrop-storage/   文件写入、checksum、received files、partial/resume 基础

  docs/                 产品、架构、协议、安全、路线图
  scripts/              macOS / Windows 打包脚本
```

## NekoLink 协议

NekoLink 当前是应用层设备通信协议雏形，不是已经完成的底层网络协议栈。

现在已经有：

- `Envelope`
- 协议名和版本
- `session_id`
- `message_id`
- 消息类型
- 能力声明
- 设备身份
- `device.hello`
- `pairing.request`
- `pairing.accept`
- `pairing.reject`
- `file.offer`
- `file.accept`
- `file.decline`
- Agent / companion / state sync 的保留消息类型

当前真实传输方式：

```text
NekoLink message
  -> TCP transport
  -> NekoDrop file transfer
```

后续目标：

```text
NekoLink message
  -> TCP / iroh / Relay / P2P
  -> file / state / Agent command
```

iroh 还没有作为运行时依赖接入。现在仓库里只有 transport 抽象和 `iroh` transport 的预留错误。后续接入 iroh 时，NekoDrop 不应该把 iroh 写死在 UI 里，而应该通过 NekoLink transport 层选择通道。

## 路线图

近期优先级：

```text
V0.6  桌面互传体验打磨
      自动发现、点设备发送、连接码兜底、Windows 防火墙提示、错误恢复

V0.7  NekoLink transport 抽象落地
      TCP 主线稳定，iroh / Relay / P2P 做技术验证，不影响现有互传

V0.8  加密 session
      设备身份、可信配对、会话密钥、传输加密

V0.9  大文件可靠性
      断点续传、失败重试、partial 文件、历史重发

V1.0  Mac / Windows 稳定版
      安装、发现、配对、发送、接收、历史、错误恢复完整可用

V1.1  手机端接入
      手机作为可信设备，能收发文件和发起桌面任务

V1.2  OpenNeko / NekoState 试点
      Agent 指令通道、伴侣状态同步、跨设备任务状态
```

更详细路线见：

- [Roadmap](docs/ROADMAP.md)
- [Future Iteration Plan](docs/FUTURE_ITERATION_PLAN.md)
- [模块化路线图](docs/modules/MODULE_ROADMAP.md)

## 文档

- [文档入口](docs/README.md)
- [产品定义](docs/PRODUCT.md)
- [架构](docs/ARCHITECTURE.md)
- [开发说明](docs/DEVELOPMENT.md)
- [当前状态](docs/STATUS.md)
- [协议草案](docs/PROTOCOL.md)
- [安全模型](docs/SECURITY.md)
- [模块边界](docs/MODULES.md)
- [NekoDrop 产品层](docs/modules/NEKODROP.md)
- [NekoLink 协议层](docs/modules/NEKOLINK.md)
- [发现与传输层](docs/modules/DISCOVERY_AND_TRANSPORT.md)
- [OpenNeko 生态层](docs/modules/OPENNEKO_ECOSYSTEM.md)
- [第三方声明](THIRD_PARTY_NOTICES.md)

## 常见问题

### 为什么不用浏览器打开

NekoDrop 是桌面软件。浏览器页面不能代表真实运行状态，也不能完整测试 Tauri 命令、文件选择、后台接收、系统托盘和安装包。

### 附近设备不出现怎么办

先检查：

- 两台设备是否在同一局域网
- Windows 是否弹出防火墙提示
- 是否开启 VPN、代理或虚拟网卡
- 公司、校园或公共网络是否屏蔽 mDNS
- 有线和无线是否处在不同网段

临时解决方式：使用连接码兜底发送。

### 为什么出现 `198.18.x.x`

`198.18.x.x` 常见于代理、测试网络或虚拟网卡，不应该作为局域网互传地址。优先关闭 VPN、代理和虚拟网卡后重启 NekoDrop。

### 现在是不是已经用了 iroh

没有。

当前版本有 NekoLink transport 抽象，真实可用传输是 TCP。iroh / Relay / P2P 是后续接入方向，不是当前已完成能力。

## 贡献原则

- UI 展示的设备、历史、进度、配对和扫描状态必须来自真实服务。
- 未完成能力需要在文档和界面中标记为规划中或实验中。
- 连接码是自动发现失败时的兜底方案，主流程仍然是发现附近设备后直接发送。
- NekoDrop 负责文件互传产品体验，NekoLink 负责通信协议和消息模型。
- OpenNeko / NekoState 相关接口可以预留，但不能描述成当前已完成的用户功能。

## 第三方与许可

第三方依赖和声明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

本仓库基于 [Apache License 2.0](LICENSE) 开源发布。

Apache-2.0 适用于本仓库中提交的 NekoDrop / NekoLink 源代码和文档。它不等于商标授权，也不自动覆盖仓库外的 OpenNeko 商业客户端、Live2D 模型、人设资产、品牌素材、签名证书、云 Relay 服务密钥或其他未提交到本仓库的商业资源。
