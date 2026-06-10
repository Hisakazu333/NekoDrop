# NekoDrop / NekoLink 模块边界

这份文档用于防止项目后续糊成一团。

NekoDrop 现在不是单纯做一个文件互传界面，而是在孵化一套个人设备网络底座。后续必须把协议、传输、产品、UI、OpenNeko 接入、手机端和同步层拆清楚。

## 1. 总体分层

```text
OpenNeko
  AI 伴侣 / Agent / 多设备任务入口

NekoState
  跨设备状态同步层

NekoDrop
  文件互传产品

NekoLink
  个人设备可信通信协议

Transport
  TCP / Local discovery / iroh / Relay / P2P / QUIC
```

核心原则：

- NekoLink 是底座，不属于某一个 UI。
- NekoDrop 是第一个产品，不应该吞掉所有协议概念。
- OpenNeko 是上层体验，不应该直接依赖 NekoDrop 的内部文件传输实现。
- NekoState 是后续状态同步层，不要提前塞进文件传输代码里。
- UI 只展示真实能力，未完成能力必须标记为待接入。

## 2. 当前仓库策略

当前不单独开 NekoLink 仓库。

先在 NekoDrop monorepo 内孵化：

```text
NekoDrop/
  apps/
    desktop/

  crates/
    nekolink-protocol/
    nekodrop-core/
    nekodrop-network/
    nekodrop-service/
    nekodrop-storage/

  docs/
```

等 NekoLink 协议、传输抽象、SDK 和 OpenNeko 接入点稳定后，再拆独立仓库。

拆仓库条件：

- `nekolink-protocol` 可独立使用
- `nekolink-transport` 至少支持 TCP 和 iroh
- 设备身份、可信配对、加密会话有稳定格式
- NekoDrop 桌面端和手机端都真实使用 NekoLink
- OpenNeko Agent 有实际跨设备调用
- 文档、测试和版本兼容策略稳定

## 3. 模块文档

建议按下面几份文档维护：

- [NekoLink 协议层](modules/NEKOLINK.md)
- [NekoDrop 产品层](modules/NEKODROP.md)
- [发现与传输层](modules/DISCOVERY_AND_TRANSPORT.md)
- [OpenNeko 生态层](modules/OPENNEKO_ECOSYSTEM.md)
- [模块化路线图](modules/MODULE_ROADMAP.md)

## 4. 状态标记

所有文档和 UI 都统一用这几种状态：

```text
已接入
  真实代码存在，当前版本可以运行。

实验中
  有代码或技术验证，但还不能作为主流程。

待接入
  只允许出现在 roadmap / 文档 / 明确的 UI 预告里。

不做
  当前阶段明确不进入范围。
```

禁止状态：

```text
看起来可用但其实是 mock
假设备
假历史
假扫描
假进度
假配对
假 Agent
```

## 5. 模块责任表

| 模块 | 职责 | 不负责 |
| --- | --- | --- |
| NekoLink Protocol | Envelope、消息类型、能力协商、版本 | UI、文件系统、Tauri |
| NekoLink Identity | device_id、fingerprint、密钥、平台身份 | 发送文件、页面展示 |
| NekoLink Pairing | 可信配对、撤销、trusted device | LAN 扫描、文件写入 |
| NekoLink Transport | TCP、iroh、Relay、P2P 抽象 | 产品流程、按钮逻辑 |
| NekoDrop Service | 文件发送/接收业务流程 | 协议长期演进 |
| NekoDrop Storage | manifest、checksum、partial、resume | 网络连接 |
| Desktop UI | 用户操作、状态展示、错误恢复 | 协议判断、传输实现 |
| NekoState | 跨设备状态同步 | 大文件传输 |
| OpenNeko | Agent、Live2D、任务编排 | 底层传输细节 |

## 6. 当前主流程目标

短期主流程必须变成：

```text
打开 NekoDrop
  -> 自动发现附近设备
  -> 选择设备或拖文件到设备
  -> 对方确认接收
  -> 传输
  -> 校验
  -> 打开接收目录
```

连接码只保留为兜底：

```text
自动发现失败
不同网段
防火墙阻断
Relay/P2P 未接入
用户手动配对
```

## 7. 技术选型原则

优先借成熟开源项目缩短周期：

- 局域网发现和互传流程参考 LocalSend Protocol
- Rust 发现库可考虑 `mdns-sd`
- 长期 P2P / Relay / QUIC 传输选 iroh
- 文件夹同步和状态同步参考 Syncthing，不直接吞进当前 MVP
- 手机桌面生态参考 KDE Connect，不直接集成

## 8. 开源策略

建议采用 open-core：

```text
开源：
  NekoLink protocol
  NekoLink Rust SDK
  NekoDrop 基础互传产品
  协议文档和测试

暂不开源或延后：
  OpenNeko Live2D 角色资产
  Agent 产品编排
  官方 relay 运维配置
  商业化服务
  品牌视觉资源
```

推荐许可证：

```text
NekoLink:
  MIT OR Apache-2.0

NekoDrop:
  Apache-2.0 或 MIT OR Apache-2.0
```

不要用 GPL 作为主许可证，避免影响后续商业桌面端、手机端和 OpenNeko 集成。

## 9. 文档维护规则

- 协议字段改动必须同步 `modules/NEKOLINK.md`。
- 传输方式改动必须同步 `modules/DISCOVERY_AND_TRANSPORT.md`。
- UI 主流程改动必须同步 `modules/NEKODROP.md`。
- OpenNeko 接入方案改动必须同步 `modules/OPENNEKO_ECOSYSTEM.md`。
- 每个版本完成后更新 `modules/MODULE_ROADMAP.md`。
- 不允许只改代码不改模块文档。
