# OpenNeko 生态层

NekoDrop 不是终点。

长期目标是让 NekoLink 成为 OpenNeko 的个人设备网络底座，让用户的 Mac、Windows、手机、平板、NAS、小主机和 Agent 节点变成一个整体。

## 1. 生态结构

```text
OpenNeko
  AI 桌面伴侣 / Agent 工作空间 / 多设备任务入口

NekoState
  跨设备状态同步

NekoDrop
  文件互传和文件流能力

NekoLink
  可信设备通信协议

Transport
  Local discovery / TCP / iroh / Relay / P2P
```

## 2. 产品关系

### NekoLink

底层协议。

负责：

- device identity
- trusted pairing
- encrypted session
- message envelope
- transport abstraction
- capability negotiation

### NekoDrop

第一个落地产品。

负责：

- 文件互传
- 文件夹传输
- checksum
- resume
- 接收确认
- 传输历史

### NekoState

状态同步层。

负责：

- 设置同步
- 任务状态同步
- OpenNeko 角色偏好
- 轻量记忆索引
- Agent 执行状态
- 离线 replay

### OpenNeko

上层交互和 Agent。

负责：

- Live2D 伴侣
- Agent 工作空间
- 任务理解
- 权限确认
- 跨设备任务编排
- 用户体验

## 3. 典型场景

### 文件互传

```text
用户拖文件到 Windows 设备

OpenNeko 可不参与
NekoDrop 负责文件业务
NekoLink 负责连接和消息
Transport 负责网络
```

### 手机控制电脑

```text
用户在手机说：
把 Mac 下载目录今天的设计图发到 Windows 台式机。

OpenNeko
  解析任务

NekoLink
  定位 Mac 和 Windows

NekoDrop
  负责文件流

NekoState
  同步任务状态
```

### 台式机跑模型

```text
笔记本发起任务
台式机执行模型
手机查看结果
```

模块分工：

```text
OpenNeko Agent
  任务调度

NekoLink
  指令通道

NekoState
  状态同步

NekoDrop
  必要时传输文件输入/输出
```

## 4. 手机端定位

手机端不是 NekoDrop 的附属遥控器。

手机端是个人设备网络节点。

能力阶段：

```text
V1.1
  手机与桌面配对
  手机发文件到电脑
  电脑发文件到手机
  分享面板接入

V1.2
  手机通过 relay / iroh 找到电脑
  不同网络可通信

V1.5
  手机发起 OpenNeko Agent 任务
  手机查看任务状态
  手机接收 Agent 结果
```

## 5. OpenNeko 页面映射

如果未来接入 OpenNeko 主产品，推荐 4 个页面：

```text
陪伴首页
  Live2D 角色
  当前设备状态
  快速投递
  当前任务反馈

Agent 工作空间
  跨设备任务
  Agent 执行
  文件流
  权限确认

世界
  设备网络
  NekoDrop
  NekoState
  玩法功能
  后续生态能力

我的
  身份
  安全
  设备
  设置
```

原则：

- 陪伴首页保留情绪和角色入口。
- Agent 工作空间负责效率。
- 世界承载生态功能，不再拆一堆小页面。
- 我的负责身份和安全。

## 6. Agent 权限边界

Agent 能力必须比文件互传更谨慎。

高风险操作必须确认：

- 删除文件
- 覆盖文件
- 执行脚本
- 发送敏感目录
- 访问浏览器数据
- 控制远程设备
- 长时间后台任务

Agent command 必须带权限 scope：

```text
file.read
file.write
file.transfer
app.open
script.run
system.control
model.run
state.read
state.write
```

默认不允许：

```text
script.run
system.control
敏感目录自动发送
无确认删除
无确认覆盖
```

## 7. NekoState 边界

NekoState 不应该一开始做成复杂分布式数据库。

第一阶段只做轻量状态同步：

```text
namespace
key-value
event log
device cursor
checkpoint
offline replay
conflict policy
```

可同步：

- 设置
- 设备在线状态
- 任务状态
- Agent progress
- OpenNeko 偏好
- 轻量记忆索引

不做：

- 大文件同步
- 强事务
- 多人协作数据库
- 云盘

## 8. 开源和商业边界

建议 open-core：

```text
开源：
  NekoLink
  NekoDrop 基础互传
  NekoLink SDK
  协议测试

可闭源：
  OpenNeko 角色资产
  Live2D 资源
  官方 Agent 编排
  relay 商业服务
  高级自动化能力
```

开源是为了：

- 让设备通信可信
- 让协议有人愿意接入
- 降低手机端和社区适配成本
- 方便安全审计
- 增强生态扩展

闭源是为了：

- 保护角色资产
- 保护品牌
- 保护商业服务
- 保留产品差异化

## 9. 仓库演进

当前：

```text
NekoDrop monorepo
```

中期：

```text
NekoDrop/
  crates/nekolink-*
  apps/desktop
  apps/mobile
```

成熟后：

```text
OpenNeko/
  NekoLink
  NekoDrop
  OpenNeko-Engine
  NekoState
```

不要现在马上拆 NekoLink 仓库。

先在当前仓库里孵化，等协议稳定后再拆。

## 10. 生态成熟标准

生态不是靠页面数量堆出来的。

成熟标准：

- Mac / Windows 文件互传稳定
- 手机端可真实加入设备网络
- NekoLink transport 可切换
- 可信配对和加密 session 稳定
- OpenNeko Agent 能跨设备执行任务
- NekoState 能同步任务状态
- 用户能理解设备网络发生了什么
- 失败能恢复，不靠用户猜

一句话：

```text
NekoLink 是地基
NekoDrop 是第一个产品
NekoState 是状态层
OpenNeko 是最终体验
```
