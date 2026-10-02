# 变更日志

本项目的显著变更记录遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### Added

- 收件箱「允许 / 拒绝」真正区分语义：新增 `respond_local_bridge_pending_action` 命令，允许立即执行动作，拒绝记录 declined 结果（2026-10-03）。

### Fixed

- 传输期间 UI 冻结：58 个 Tauri 命令全部改为异步执行（blocking 线程池），不再占用主线程（2026-10-03）。
- 网络读写无超时导致的线程挂死与取消失效：连接后默认 60 秒停滞超时，等待对方人工决定时放宽到 330 秒（2026-10-03）。
- 断点残留 partial 文件永久阻塞同路径接收；校验和不匹配保留损坏 partial 毒化续传（2026-10-03）。
- 有待处理请求时轮询每 1.2 秒强制切换页面劫持用户输入（2026-10-03）。
- 接收会话并发启动竞态（泄漏监听端口）、传输列表与扫描乱序覆盖、离线设备仍作为发送目标、TitleBar 监听器每渲染重挂（2026-10-03）。
- 本地桥接授权码改为 CSPRNG 生成，不再可由请求方推导（2026-10-03）。
- bundle JSON 持久化改为 temp + fsync + rename 原子写（2026-10-03）。

### Changed

- 分支模型简化为 `main / develop / <type>-<topic>` 三段式；旧分支（desktop-develop、docs-develop、dev/hisakazu）已合并或归档为 `archive/adapter-workspace-20260716` 标签（2026-10-03）。
- 文档目录按读者重组为 `docs/product`、`docs/dev`、`docs/archive` 等分组；贡献指南全面重写（2026-10-03）。

## 早期里程碑

- 2026-07：首个 release 快照（图标、打包脚本、generic-adapter 样例迭代）。
- 2026-08~09：桌面端全新工具化 UI 框架；传输体验与错误提示完善；macOS 应用签名；发现在线状态稳定化。
- 2026-06：NekoLink 协议核心（X25519 + HKDF + AEAD 会话、ed25519 身份绑定、回放窗口）、bundle 体系与 local bridge 地基。
