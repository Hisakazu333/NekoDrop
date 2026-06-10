# NekoDrop

NekoDrop 是一个跨平台桌面文件互传软件。

当前目标很简单：

```text
Mac 和 Windows 打开 NekoDrop
自动发现附近设备
选择文件
点设备发送
对方确认接收
文件传过去并完成校验
```

它不是网盘，不是聊天软件，也不是浏览器网页。NekoDrop 是一个 Tauri 桌面软件，文件选择、收件目录、设备发现和传输都依赖本地桌面端能力。

## 当前能用什么

已经接入的真实能力：

- macOS / Windows 桌面端工程
- 启动后自动打开后台收件
- mDNS/DNS-SD 自动发现附近设备
- 附近设备列表
- 发现状态诊断
- 离线附近设备自动过期
- 点附近设备发送文件
- 连接码兜底发送
- 文件和文件夹选择
- 真实 manifest 和 SHA-256 校验
- 发送前 transfer offer
- 接收端接受 / 拒绝
- 真实传输进度、速度、ETA
- 接收完成后校验文件
- macOS 打包脚本
- Win11 打包脚本

还没完成：

- 可信配对
- 加密 session
- 系统级 Windows 防火墙自动配置
- 断点续传
- 传输历史
- 手机端
- iroh / Relay / P2P

## 怎么使用

### 1. 两台电脑都打开 NekoDrop

两台电脑需要在同一局域网内。

打开后，NekoDrop 会自动打开后台收件服务，并通过 mDNS 广播自己。

### 2. 等附近设备出现

发送端界面里会显示“附近设备”。

如果另一台电脑出现了：

```text
选择文件或文件夹
点击附近设备
对方接受
开始传输
```

### 3. 自动发现失败时用连接码兜底

如果附近设备没有出现，可以用连接码：

```text
接收端点击查看兜底码
发送端粘贴连接码
选择文件
发送
```

自动发现失败常见原因：

- 不在同一局域网
- Windows 防火墙拦截
- 公司/校园网络屏蔽 mDNS
- 有线和无线不在同一网段
- VPN / 代理 / 虚拟网卡干扰

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

运行桌面端开发模式：

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" npm --workspace apps/desktop run tauri:dev
```

注意：不要只用浏览器打开 Vite 页面。浏览器预览不能代表桌面软件。

## macOS 打包

生成 `.app`：

```bash
./scripts/package-desktop.sh
```

跳过测试生成 `.app`：

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

安装到应用程序：

```bash
cp -R "release/desktop/<时间戳>/bundle/macos/NekoDrop.app" "/Applications/"
```

运行：

```bash
open "/Applications/NekoDrop.app"
```

## Win11 打包

在 Windows 11 PowerShell 里运行：

```powershell
npm install
npm run package:windows -- -SkipTests -Bundles nsis
```

默认打包：

```powershell
npm run package:windows
```

输出目录：

```text
release\desktop\<时间戳>\
```

优先运行脚本最后打印出来的安装器：

```text
bundle\nsis\...\setup.exe
bundle\msi\...\*.msi
```

不要运行开发模式下的 debug exe。
如果看到 `127.0.0.1:1420` 白屏，说明你运行到了开发形态，不是正式安装包。

## 常见问题

### 附近设备不出现

先检查：

- 两台电脑是否在同一局域网
- Windows 是否弹出防火墙提示
- 是否开了 VPN / 代理 / 虚拟网卡
- 公司或校园网络是否屏蔽 mDNS

临时解决：

```text
使用连接码兜底发送
```

### 报错连接到 198.18.x.x

`198.18.x.x` 通常是代理、测试网段或虚拟网卡地址，不应该作为局域网互传地址。

当前版本已经过滤这类地址。
如果仍然出现，优先关闭 VPN / 代理 / 虚拟网卡后重新打开 NekoDrop。

### 发送失败：连接超时

常见原因：

- 接收端没有运行
- 接收端防火墙阻止
- 两台电脑不在同一网段
- 对方设备列表已经过期

当前可以用连接码兜底。界面会显示自动发现状态；如果仍然失败，优先检查 Windows 防火墙、网络隔离和 VPN / 代理 / 虚拟网卡。

### 为什么还没有可信配对

当前阶段先把自动发现和真实传输做顺。

后续版本会接入：

```text
设备身份
可信配对
加密 session
trusted devices
```

## 项目结构

```text
NekoDrop/
  apps/
    desktop/
      src/              # React UI
      src-tauri/        # Tauri/Rust 桌面层

  crates/
    nekolink-protocol/  # NekoLink 协议基础
    nekodrop-core/      # 产品领域模型
    nekodrop-network/   # TCP 传输和连接码
    nekodrop-service/   # 文件发送/接收流程
    nekodrop-storage/   # manifest、checksum、写入文件

  docs/
  scripts/
```

## 文档

- [模块边界](docs/MODULES.md)
- [NekoDrop 产品层](docs/modules/NEKODROP.md)
- [NekoLink 协议层](docs/modules/NEKOLINK.md)
- [发现与传输层](docs/modules/DISCOVERY_AND_TRANSPORT.md)
- [模块化路线图](docs/modules/MODULE_ROADMAP.md)
- [第三方声明](THIRD_PARTY_NOTICES.md)

## 后续路线

当前重点是 V0.6：

```text
自动发现设备
点设备发送
连接码兜底
Windows 防火墙提示
设备离线过期
```

后续：

```text
V0.7  Transport 抽象
V0.8  加密 session
V0.9  断点续传和历史
V1.0  Mac / Win 稳定版
V1.1  手机端
V1.2  iroh / Relay / P2P
```

## 和 NekoLink / OpenNeko 的关系

NekoDrop 是第一个产品，不是最终全部生态。

长期结构：

```text
NekoLink  = 设备通信协议
NekoDrop  = 文件互传产品
NekoState = 状态同步层
OpenNeko  = AI 伴侣 / Agent 入口
```

现在优先把 NekoDrop 做成真正好用的互传软件。其他生态能力会在这个基础上继续迭代。

## 开发原则

- 不写假设备。
- 不写假历史。
- 不写假进度。
- UI 状态必须来自真实服务。
- 未完成能力必须标记为待接入。
- 连接码是兜底，不是最终主流程。
- 自动发现和文件传输分层实现。
