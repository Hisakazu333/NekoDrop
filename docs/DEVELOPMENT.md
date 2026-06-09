# NekoDrop 开发说明

## 当前状态

当前项目是一个真实 Tauri 桌面端连接码传输版本：

- Rust workspace 和核心领域 crate
- Tauri 2 桌面端入口
- React/Vite 桌面 WebView 界面
- Tauri IPC 状态读取命令
- 系统文件 / 文件夹选择
- 文件 manifest 扫描和 SHA-256
- TCP 连接码收件监听
- 传输 offer、接收确认、拒绝和超时
- 真实发送 / 接收进度、速度和 ETA
- 接收文件校验和安全落盘

当前还没有接入设备发现、可信配对、历史记录持久化和 OpenNeko 支撑层。界面必须把这些能力保留为“待接入”，不允许用假设备、假 Windows 电脑、假传输记录或浏览器预览来冒充桌面软件能力。

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
3. 完善连接码传输状态机和错误恢复。
4. 接入设备发现。
5. 实现配对和可信设备持久化。
6. 把连接码传输升级为可信设备传输。
7. 增加加密会话。
8. 增加取消、断点续传和最终体验打磨。

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
