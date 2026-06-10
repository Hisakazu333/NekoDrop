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
- 接收目录持久化
- 网络/传输错误提示和目标地址预检
- 传输历史持久化
- 历史记录打开位置、重发、删除、清空
- NekoLink transport 抽象和 TCP 实现

当前还没有接入 iroh 真实运行时、Relay / P2P、加密 session、失败后自动重试 / 完整断点续传 UI 流程、手机端互传主流程和 OpenNeko 支撑层。界面和文档应将这些能力标记为规划中或实验中，不应把占位数据描述为真实桌面能力。

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

## 实现顺序

1. 保持 `nekodrop-core` 作为产品模型源头。
2. 在 `nekodrop-storage` 中实现文件 / 文件夹 manifest 扫描。
3. 完善发现状态、错误恢复和跨 Mac / Windows 真实验证。
4. 继续打磨可信设备发送和连接码兜底的操作路径。
5. 把 NekoLink transport 抽象用实，为 iroh / Relay 做技术验证。
6. 增加加密会话。
7. 增加断点续传和最终体验打磨。

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
