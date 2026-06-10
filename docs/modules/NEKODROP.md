# NekoDrop 产品层

NekoDrop 是 NekoLink 的第一个落地产品。

它的职责是把“个人设备可信网络”先落到一个用户能理解、能真实使用的场景：Mac、Windows、手机之间稳定传文件。

## 1. 产品定位

```text
NekoDrop
  = 跨设备文件互传产品
  + NekoLink 的第一个真实使用场景
```

NekoDrop 不等于 NekoLink。

NekoDrop 不应该承载所有生态功能，也不应该把 OpenNeko Agent、NekoState、手机控制电脑等能力全部塞进文件互传 UI。

## 2. 主流程

正确主流程：

```text
打开 NekoDrop
  -> 自动发现附近设备
  -> 选择文件或拖入文件
  -> 选择目标设备
  -> 对方确认接收
  -> 传输
  -> 校验
  -> 打开接收目录
```

连接码流程只作为兜底：

```text
打开收件
  -> 复制连接码
  -> 对方粘贴连接码
  -> 发送
```

连接码不应该是主流程。

## 3. 当前状态

已接入：

- Tauri desktop app
- React UI
- Rust service
- 启动后自动后台接收
- 文件选择
- 文件夹选择
- 手动路径输入
- manifest
- SHA-256
- TCP connection code
- mDNS / DNS-SD 自动发现
- 附近设备列表
- 设备离线过期
- 点附近设备发送
- 设备身份
- 可信配对基础
- 可信设备管理
- transfer offer
- accept / decline
- 真实传输进度
- 发送中取消
- 传输历史持久化
- 历史打开位置 / 重发 / 删除 / 清空
- 接收目录
- macOS DMG 打包
- Win11 NSIS / MSI 打包脚本

待接入：

- 加密 session
- 断点续传完整产品流程
- 手机端接入
- iroh / Relay / P2P
- OpenNeko Agent 指令通道
- NekoState 状态同步

## 4. UI 原则

### 不做

- 不堆一堆装饰卡片。
- 不展示 fake nearby devices。
- 不展示 fake history。
- 不展示 fake pairing。
- 不展示 fake progress。
- 不把待接入功能伪装成已可用。

### 必须做

- 主操作必须明显。
- 当前状态必须来自真实服务。
- 失败原因必须能看懂。
- 收件和发送不要拆成用户难理解的流程。
- 自动发现成功时，用户不需要看连接码。
- 自动发现失败时，连接码作为兜底入口。

## 5. 推荐页面结构

短期 NekoDrop 桌面端建议保留一个工作台，不继续扩散多页面。

```text
工作台
  附近设备
  发送队列
  当前传输
  接收状态
  连接码兜底

历史
  已完成
  失败
  可重试
  可续传

设置
  设备名
  接收目录
  启动后自动收件
  发现开关
  日志导出
```

如果未来接入 OpenNeko 主产品，可映射为：

```text
陪伴首页
  当前设备状态、快速投递、Live2D 反馈

Agent 工作空间
  跨设备任务、Agent 执行、文件流

世界
  NekoDrop、NekoState、设备网络、玩法能力

我的
  身份、安全、设备、设置
```

NekoDrop 自己不要提前做成复杂生态页面。

## 6. 发送模块

职责：

- 文件选择
- 文件夹选择
- 拖拽路径
- manifest 生成
- 目标设备选择
- transfer offer
- 发送进度
- 失败提示
- 完成报告

不负责：

- 设备信任判断的协议细节
- transport 路由
- Agent 任务调度
- 状态同步

发送前校验：

```text
是否有文件
是否有目标设备
目标是否在线
目标是否支持 file_receive
目标是否需要配对
目标是否有可用 transport
```

## 7. 接收模块

职责：

- 后台监听
- 接收目录
- incoming offer
- 接受 / 拒绝
- 写入 partial
- checksum verify
- 完成后打开目录

短期应该从“一次性收件”改为“后台常驻接收”：

```text
应用启动
  -> 自动打开接收服务
  -> 广播当前可收件状态
  -> 收到 offer 后弹出确认
```

连接码模式仍可手动打开，但不是默认主入口。

## 8. 设备模块

职责：

- nearby devices
- trusted devices
- device identity
- fingerprint
- trust state
- online / offline

设备列表只能显示真实发现到的设备。

状态示例：

```text
在线，未配对
在线，已信任
离线
等待确认
被阻止
```

## 9. 传输历史

历史必须来自真实数据。

存储内容：

```text
transfer_id
direction
peer_device_id
root_name
file_count
total_bytes
status
created_at
completed_at
receive_dir
error
resume_state
```

不能写假历史。

## 10. 错误处理

错误必须变成人能理解的话。

示例：

```text
198.18.0.1
  当前选择到了代理或虚拟网卡地址，请切换到真实局域网或使用连接码兜底。

connection timeout
  对方电脑没有响应，可能未打开收件、防火墙阻止、或不在同一网络。

checksum mismatch
  文件校验失败，已拒绝写入最终文件。

unsupported capability
  对方版本太旧，不支持此功能。
```

## 11. NekoDrop 与 NekoLink 的接口

NekoDrop 应该只调用 NekoLink 暴露的能力：

```text
list_devices()
send_file_offer()
accept_file_offer()
open_file_stream()
send_file_chunks()
receive_file_chunks()
close_transfer()
```

不要在 UI 里解析协议细节。

不要在 NekoDrop service 里硬编码未来 Agent / state message。

## 12. V1.0 验收标准

V1.0 前必须做到：

- Mac -> Windows 真实互传
- Windows -> Mac 真实互传
- 自动发现设备
- 连接码兜底
- 接收确认
- SHA-256 校验
- 文件夹传输
- 大文件稳定
- 失败有明确提示
- Win11 安装包可用
- macOS `.app` / DMG 可用

V1.0 不做：

- 完整 Agent 执行
- NekoState 分布式状态库
- 复杂云账号
- 大规模多人协作
- 网盘化
