# 发现与传输层

发现和传输必须拆开。

发现负责回答：

```text
附近有哪些设备？
这些设备是谁？
它们当前支持什么能力？
它们是否在线？
```

传输负责回答：

```text
如何连接？
如何加密？
如何发送消息？
如何传文件？
失败后如何重试？
```

不能把“扫描端口”当成设备发现主方案。

## 1. 当前问题

当前 connection-code 方案存在几个问题：

- 用户必须手动打开收件。
- 用户必须复制和粘贴连接码。
- IP 选择可能拿到代理或虚拟网卡地址，例如 `198.18.0.1`。
- 连接码是一次性流程，不适合做主体验。
- 收件 TCP 监听是传输用途，不能拿来做扫描。

`198.18.0.0/15` 是测试/benchmark 网段，很多代理、虚拟网络、隧道工具会使用类似地址。这个地址不应该出现在普通局域网连接码里。

## 2. 正确架构

```text
Discovery Channel
  UDP multicast / mDNS / LocalSend-style discovery

Transport Channel
  TCP / iroh QUIC / Relay / P2P

Protocol Channel
  NekoLink Envelope
```

三层不能混。

错误做法：

```text
扫描 TCP 传输端口
```

原因：

当前 TCP 收件监听用于真实文件传输，扫描连接会消耗或干扰收件流程。

## 3. 短期方案：LocalSend-style Discovery

短期最推荐参考 LocalSend Protocol。

目标：

- 启动后自动广播设备
- 自动发现附近设备
- 设备列表真实出现
- 选择设备发送
- 连接码作为兜底

建议能力：

```text
UDP multicast beacon
HTTP / HTTPS receive API
prepare-upload
upload
cancel
device info
capability
```

在 NekoDrop 中不一定完全兼容 LocalSend，但流程应该接近：

```text
App start
  -> start receive service
  -> broadcast device info
  -> listen for beacon
  -> update nearby devices
  -> user sends to device
```

## 4. mDNS 方案

mDNS 适合局域网服务发现。

可用于：

- desktop discovery
- service name
- host
- port
- TXT record capabilities

服务名示例：

```text
_nekolink._tcp.local
_nekodrop._tcp.local
```

TXT record 示例：

```text
device_id=neko-device-xxxx
platform=windows
kind=desktop
capabilities=file_receive,file_sha256,device_pairing
fingerprint=sha256:xxxx
```

风险：

- 部分公司/校园网络屏蔽 mDNS。
- 有线和无线可能不在同一广播域。
- Windows 防火墙可能拦截。
- 手机系统权限会影响发现。

所以 mDNS 不能是唯一方案。

## 5. IP 选择规则

连接码和 discovery 广播都必须排除错误地址。

优先：

```text
10.0.0.0/8
172.16.0.0/12
192.168.0.0/16
```

排除：

```text
127.0.0.0/8
169.254.0.0/16
198.18.0.0/15
224.0.0.0/4
0.0.0.0
虚拟网卡
代理网卡
Docker
Hyper-V
VPN 非预期地址
```

如果无法选择可信 LAN IP：

```text
UI 显示网络异常
引导用户使用连接码兜底
或等待 iroh / relay transport
```

不要静默退回 `127.0.0.1` 给另一台电脑。

## 6. 中期方案：NekoLink Transport 抽象

所有 transport 都应该实现同一个抽象。

```text
TcpTransport
LocalLanTransport
IrohTransport
RelayTransport
```

NekoDrop 不应该关心 transport 细节。

发送文件时只做：

```text
select target device
resolve transport
open stream
send NekoLink envelope
stream file chunks
```

## 7. 长期方案：iroh

iroh 适合作为 NekoLink 长期 transport。

原因：

- Rust 生态
- public-key addressing
- QUIC
- relay fallback
- hole punching
- 加密 stream
- 更适合 Agent 和状态同步

iroh 切入点：

```text
NekoLink Identity
  绑定 iroh key

NekoLink Transport
  IrohTransport

NekoDrop
  文件流跑在 iroh stream 上

OpenNeko
  agent.command 跑在 iroh stream 上

NekoState
  state sync 跑在 iroh stream/datagram 上
```

## 8. Relay 策略

Relay 只解决连接，不应该变成云盘。

Relay 不保存最终文件。

Relay 可以做：

- connection rendezvous
- encrypted relay stream
- NAT fallback
- temporary transfer relay

Relay 不应该做：

- 文件永久存储
- 用户账号网盘
- 明文内容查看
- 中心化任务执行

## 9. 手机端考虑

手机端发现问题更复杂。

需要考虑：

- iOS local network permission
- Android nearby / Wi-Fi permissions
- 后台限制
- 文件权限
- 分享面板
- 热点场景
- 不同网段

所以手机端路线：

```text
第一阶段：
  与桌面端同局域网发现

第二阶段：
  扫码 / 连接码兜底

第三阶段：
  iroh / relay
```

## 10. V0.6 验收标准

V0.6 发现层完成标准：

- 两台同局域网电脑打开 NekoDrop 后能互相看到。
- UI 不需要用户粘贴连接码即可选择设备。
- 发现失败时有明确说明。
- 连接码仍然可用。
- 不再生成 `198.18.x.x` 这类错误连接地址。
- Windows 防火墙阻断时有明确错误提示。

## 11. V1.0 验收标准

V1.0 传输层完成标准：

- TCP fallback 可用。
- Local discovery 可用。
- 可信配对可用。
- 加密 session 可用。
- 大文件传输稳定。
- 失败可恢复。

## 12. V1.2 验收标准

V1.2 iroh transport 完成标准：

- iroh transport 能与 NekoLink identity 绑定。
- 同局域网可直连。
- 不同网络可尝试 P2P。
- P2P 失败可 relay。
- NekoDrop 文件流可跑在 iroh stream 上。
- OpenNeko Agent command 可跑在 iroh stream 上。
