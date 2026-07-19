# NekoDrop 代码安全审计

审计日期：2026-07-18<br>
审计分支：`dev/hisakazu`  目前 HEAD：`126a7a4`<br>
审计范围：Rust workspace、Tauri desktop、local bridge、sidecar、bundle/storage、CI 与依赖审计脚本。
状态基线：`docs/STATUS.md`（2026-06-25）；sidecar/adapter 入口按“实验中/部分接入”边界单独标注。

## 结论

当前代码不能按“安全问题已清零”发布。已确认的问题包括：

- **P1：** crafted `bundle_id` 可清空整个 staged-bundle 根目录。
- **P1：** encrypted control 的 replay counter 未被 AEAD 绑定，可改计数绕过 replay window。
- **P1：** LAN 接收循环对单个连接无读取超时，恶意连接可以阻塞整个收件器。
- **P1：** local bridge 授权只信任调用方自报的 `client_id` / `app_kind`，可被本机进程冒用。
- **P1：** local bridge 撤销授权不会取消已排队的 mutation，授权过期后 worker 仍可执行 send/import/rollback。
- **P1：** receive 目录已有 symlink 时，相对 manifest path 可将文件写到 receive 目录之外。
- **P1（条件性入口）：** `apps/sidecar` 的 `receive` 命令自动接受明文文件，没有认证或用户确认。
- **P2：** 手工 bundle 固定声明不含 secrets，可能把 token/密钥目录标成可导入。
- **P2：** import receipt 的 rollback 路径未限制在 `import_root`，本地篡改 receipt 可造成 confused-deputy 删除。
- **P2：** Transfer offer 缺少文件数/大小上限和 checked arithmetic，可造成资源消耗并绕过磁盘预检。
- **P2：** 依赖审计脚本无法解析 CVSS/vector severity，未来可能把应阻断的 advisory 当成非阻断项。
- **P2：** macOS 包使用 ad-hoc signing，发布产物没有发布者身份。
- **P2：** Ed25519 signing seed 与 secret seed 以明文 JSON 保存，未使用 Keychain/DPAPI 或文件权限加固。

P1 项应在下一次发布前关闭或明确禁用入口。P2 项不应继续以“已完成安全闭环”对外描述。

## Findings

### NDK-001 — `bundle_id="."` 可删除整个暂存根

**等级：P1 / 远端触发的本地数据破坏**<br>
**状态：已复现**

`BundleManifest::validate()` 只要求 `bundle_id` 非空；协议、storage 和 desktop 的安全校验都只拦截 `..`、分隔符、冒号和 NUL，没有拒绝单独的 `.`：

- [crates/nekolink-protocol/src/lib.rs:2654](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekolink-protocol/src/lib.rs:2654>)
- [crates/nekodrop-storage/src/bundle.rs:1146](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-storage/src/bundle.rs:1146>)
- [apps/desktop/src-tauri/src/commands/staged_bundles.rs:350](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/staged_bundles.rs:350>)

`stage_bundle_directory()` 随后执行 `staging_root.join(bundle_id)`，对 `.` 得到整个 `staging_root`，并在替换前调用 `remove_dir_all`：

- [crates/nekodrop-storage/src/bundle.rs:352](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-storage/src/bundle.rs:352>)

调用链是桌面接收 -> `maybe_stage_received_bundle()` -> `stage_bundle_directory()`。临时 harness 预置已有 staged bundle 后传入 `bundle_id="."`，观察到 `remove_dir_all(staging_root/.)` 删除已有内容，随后 macOS 因根目录本身无法移除而返回 `Invalid argument`。也就是说请求最终失败，但已有 staged bundles 已经丢失。

同一个 id 还会把 import destination 变成 `import_root/.`；临时 harness 在 `skip_conflicts` 策略下观察到 payload 被写到 `import_root/payload`，而不是 `import_root/./payload` 下的独立 bundle 目录。`delete_staged_bundle(".")` 也会先删除 staging root 内容再报错。

**修复方向：** 所有 bundle id 校验统一拒绝 `.`、空段和保留目录名；在删除/替换前要求 `staging_path.parent()` 正好是 canonicalized `staging_root`，禁止目标等于根目录；为协议、storage、desktop 和远端接收增加回归测试。

### NDK-002 — encrypted control 的 replay counter 未进入 AEAD AAD

**等级：P1 / 加密完整性与重放保护失效**<br>
**状态：已复现**

控制帧加密时的 AAD 只绑定 protocol、session、message 和 inner kind，没有绑定 traffic header 的 counter、nonce、direction 或 cipher：

- [crates/nekolink-protocol/src/lib.rs:748](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekolink-protocol/src/lib.rs:748>)
- [crates/nekolink-protocol/src/lib.rs:2911](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekolink-protocol/src/lib.rs:2911>)

解密只检查 nonce 长度，不检查 nonce 是否由该 counter 派生：

- [crates/nekolink-protocol/src/lib.rs:2982](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekolink-protocol/src/lib.rs:2982>)
- [crates/nekolink-protocol/src/lib.rs:3027](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekolink-protocol/src/lib.rs:3027>)

`open_control_once()` 在解密后才把 header.counter 交给 replay window：

- [crates/nekolink-protocol/src/lib.rs:769](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekolink-protocol/src/lib.rs:769>)

临时 harness 对一个合法 `counter=9` 报文只修改 `payload.header.counter=10`，保持原 nonce/ciphertext/message_id，结果为 `counter-tampered replay accepted: true`。因此观察者可以把旧的合法控制帧改成窗口内的新计数，绕过现有重复计数检查。

**修复方向：** 将完整 canonical traffic header 放入 AAD；接收端验证 nonce 必须等于 `session_frame_nonce(cipher, counter)`，并验证方向、kind、cipher 与当前会话状态一致；replay window 应在认证通过前不推进，并增加“改 counter/nonce/direction 必须拒绝”的测试。

### NDK-003 — LAN 接收器可被单个半连接永久阻塞

**等级：P1 / 可用性拒绝服务**<br>
**状态：代码路径确认**

桌面默认绑定 `0.0.0.0`：

- [apps/desktop/src-tauri/src/commands/mod.rs:1184](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/mod.rs:1184>)

accept loop 将 listener 设为 nonblocking，但每次 accept 后把 stream 改回 blocking，并在同一接收线程中同步调用 service：

- [apps/desktop/src-tauri/src/commands/mod.rs:1343](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/mod.rs:1343>)
- [apps/desktop/src-tauri/src/commands/mod.rs:1387](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/mod.rs:1387>)

协议层 `read_json_frame()` 对 4 字节长度和后续 payload 使用 `read_exact()`，没有 socket read deadline：

- [crates/nekodrop-network/src/tcp_file.rs:1545](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-network/src/tcp_file.rs:1545>)

LAN 主机只需连上端口后不发送完整 frame，即可阻塞该接收线程，后续合法设备无法被 accept；`stop/cancel` 也无法打断正在阻塞的 read。

**修复方向：** 在握手、offer、decision、file frame 各阶段设置有限 read/write timeout；将每条连接隔离到受限 worker，并设置并发连接上限；超时必须清理 pending offer 和 active cancel 状态。

### NDK-004 — local bridge 授权可被本机进程冒用

**等级：P1 / 本地授权绕过**<br>
**状态：代码路径确认**

local bridge 只校验 peer 是 loopback：

- [apps/desktop/src-tauri/src/local_bridge_runtime.rs:92](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/local_bridge_runtime.rs:92>)

持久授权记录只包含 `client_id`、`app_kind`、scope 和时间，没有 bearer secret、签名公钥或 OS process identity：

- [apps/desktop/src-tauri/src/local_bridge_authorizations.rs:12](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/local_bridge_authorizations.rs:12>)
- [apps/desktop/src-tauri/src/local_bridge_authorizations.rs:61](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/local_bridge_authorizations.rs:61>)

授权判断只比较调用方自报的两个字符串：

- [apps/desktop/src-tauri/src/commands/mod.rs:4328](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/mod.rs:4328>)
- [apps/desktop/src-tauri/src/commands/mod.rs:4356](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/mod.rs:4356>)

因此任意本机进程只要猜到已授权 adapter 的 identity，就可以调用 `devices.list`、`bundle.detail`、`bundle.send`、`bundle.import` 或 `bundle.rollback`，直到授权 TTL 到期。loopback 不是调用方身份边界。

**修复方向：** 首次授权后为每个 client 生成随机 secret，后续请求用 bearer token 或 challenge-response 签名；macOS/Windows 可进一步绑定应用签名或 OS IPC credential。授权文件本身也应限制权限并避免只保存可猜的 identity。

### NDK-005 — receive 目录中的现有 symlink 可绕出目标目录

**等级：P1 / 远端文件写入路径穿越**<br>
**状态：已复现（需要目标目录预置 symlink）**

`safe_join_receive_path()` 只检查绝对路径、`..`、root 和 Windows prefix，不检查目标路径各级目录是否为 symlink：

- [crates/nekodrop-storage/src/receive_dir.rs:5](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-storage/src/receive_dir.rs:5>)

接收写入先 `create_dir_all(parent)`，然后对 `.nekodrop-part` `File::create`/append，最后 rename：

- [crates/nekodrop-storage/src/received_file.rs:111](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-storage/src/received_file.rs:111>)
- [crates/nekodrop-storage/src/received_file.rs:137](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-storage/src/received_file.rs:137>)
- [crates/nekodrop-storage/src/received_file.rs:149](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-storage/src/received_file.rs:149>)
- [crates/nekodrop-storage/src/received_file.rs:220](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-storage/src/received_file.rs:220>)

临时 harness 在 `receive/link` 预置指向 `outside` 的 symlink，远端只发送合法相对路径 `link/escape.txt`，结果文件落在 `outside/escape.txt`。这绕过了字符串层面的 manifest path 防护。

**修复方向：** 使用不跟随 symlink 的目录句柄/平台安全 open；至少在每一级创建和写入前用 `symlink_metadata` 检查并拒绝 symlink/reparse point，并对 destination 做 canonical containment 检查；补 macOS/Windows 测试。

### NDK-006 — local bridge 撤销后已排队 mutation 仍会执行

**等级：P1 / 授权撤销绕过**<br>
**状态：代码路径确认（撤销语义应即时生效）**

`bundle.send`、`bundle.import` 和 `bundle.rollback` 只在请求入队时检查一次 active authorization。入队后，action 结构只保存 client、请求参数和 `requested_at_ms`，没有授权版本、scope 快照或过期时间：

- [apps/desktop/src-tauri/src/commands/mod.rs:2045](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/mod.rs:2045>)
- [apps/desktop/src-tauri/src/commands/mod.rs:2098](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/mod.rs:2098>)
- [apps/desktop/src-tauri/src/commands/mod.rs:2151](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/mod.rs:2151>)
- [apps/desktop/src-tauri/src/app_state.rs:138](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/app_state.rs:138>)

撤销路径只从 `runtime.authorizations` 删除 scope，不触碰 `pending_actions`；TTL 也只在请求检查时通过 `local_bridge_authorization_is_active()` 生效：

- [apps/desktop/src-tauri/src/commands/mod.rs:4270](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/mod.rs:4270>)
- [apps/desktop/src-tauri/src/commands/mod.rs:4319](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/mod.rs:4319>)

worker 取出 action 后直接调用 send/import/rollback 执行函数，没有重新检查当前授权、TTL 或撤销状态：

- [apps/desktop/src-tauri/src/commands/mod.rs:664](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/mod.rs:664>)
- [apps/desktop/src-tauri/src/commands/mod.rs:2270](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/mod.rs:2270>)

因此，调用方可以在授权有效期内把 mutation 放入队列，随后用户撤销授权或 TTL 到期，等 worker 延迟消费后动作仍会执行。对 `bundle.rollback`，这会让撤销后的调用方仍能触发文件删除；对 send/import，则会继续产生网络发送或本地导入副作用。当前 UI/bridge 提供 revoke 和 pending-action 管理，但没有声明“已排队动作不可撤销”的例外，因此这是授权撤销不具备即时语义的残余风险。

**修复方向：** action 保存授权代次或不可伪造的授权句柄；revoke/prune 时取消匹配队列并写入 cancelled lifecycle result；worker claim 和真正执行前再次验证 client、scope、TTL 与授权版本；补“入队 -> revoke/expiry -> worker 必须拒绝”的回归测试。

### NDK-007 — `apps/sidecar receive` 自动接受未认证明文

**等级：P1（条件性入口）**<br>
**状态：代码路径确认**

sidecar CLI 直接把用户给出的 bind address 绑定到 TCP，并调用 `accept_transfer()`：

- [apps/sidecar/src/main.rs:91](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/sidecar/src/main.rs:91>)
- [apps/sidecar/src/main.rs:97](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/sidecar/src/main.rs:97>)
- [apps/sidecar/src/main.rs:115](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/sidecar/src/main.rs:115>)

`accept_transfer()` 的默认 decision 是 `|_| true`，接收路径是 plain offer/file flow：

- [crates/nekodrop-service/src/lib.rs:392](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-service/src/lib.rs:392>)
- [crates/nekodrop-service/src/lib.rs:423](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-service/src/lib.rs:423>)

如果该 workspace binary 被用户或发布包暴露，并按 usage 绑定 `0.0.0.0`，任意可达主机无需配对、加密或用户确认即可向指定目录写入文件，同时也继承无 timeout 的 DoS 问题。

**修复方向：** 将该命令保持为明确的实验工具并默认只绑定 loopback；或强制 authenticated session + trusted-device verification + explicit decision，并复用桌面接收器的 timeout/size limits。不要把当前入口作为可发布接收面。

### NDK-008 — transfer offer 缺少一致的文件数/大小上限

**等级：P2 / 资源消耗与磁盘预检绕过**<br>
**状态：已复现**

网络层只在真正读取 file frames 时限制 `MAX_FILE_FRAME_COUNT=10_000`：

- [crates/nekodrop-network/src/tcp_file.rs:23](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-network/src/tcp_file.rs:23>)
- [crates/nekodrop-network/src/tcp_file.rs:1510](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-network/src/tcp_file.rs:1510>)

但 `TransferOffer::validate()` 没有对应 count 上限、单文件大小上限或总和的 checked arithmetic：

- [crates/nekolink-protocol/src/lib.rs:2455](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekolink-protocol/src/lib.rs:2455>)
- [crates/nekolink-protocol/src/lib.rs:2478](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekolink-protocol/src/lib.rs:2478>)

离线 harness 构造 10,001 个合法文件条目，`offer.validate()` 仍返回成功。接收端在 `decide`、resume 和磁盘预检阶段先遍历整个 offer，直到 file frame 阶段才可能因 10,000 上限拒绝：

- [crates/nekodrop-service/src/lib.rs:910](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-service/src/lib.rs:910>)
- [crates/nekodrop-service/src/lib.rs:921](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-service/src/lib.rs:921>)
- [crates/nekodrop-service/src/lib.rs:937](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-service/src/lib.rs:937>)

另外，`u64` 文件大小求和在 release 下会 wrap。攻击者可让 `total_bytes` 变小，绕过基于总量的 space preflight，再按每个 file header 的大尺寸写入：

- [crates/nekodrop-service/src/lib.rs:977](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-service/src/lib.rs:977>)

**修复方向：** 在协议校验阶段统一限制文件数、路径长度、单文件大小和总大小；用 `checked_add`，溢出直接拒绝；让 offer 上限与 frame reader 共用常量，并在 UI/resume/space 之前拒绝超限 offer。

### NDK-009 — 手工 bundle 固定声明“不含 secrets”

**等级：P2 / 敏感数据误导性导入**<br>
**状态：代码路径确认**

手工 bundle 可从用户选择的任意目录创建，但 `manual_bundle_permissions()` 无扫描、脱敏或用户声明步骤，始终写入 `contains_secrets: false`：

- [apps/desktop/src-tauri/src/commands/mod.rs:413](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/mod.rs:413>)
- [apps/desktop/src-tauri/src/commands/bundle_helpers.rs:68](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/bundle_helpers.rs:68>)

协议以该字段决定 bundle 是否可导入，`false` 会进入 `ImportAllowed`；而规范明确 `contains_secrets=true` 必须拒绝导入：

- [crates/nekodrop-storage/src/bundle.rs:332](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-storage/src/bundle.rs:332>)
- [docs/BUNDLE_SPEC.md:211](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/docs/BUNDLE_SPEC.md:211>)

用户选择含 `.env`、token、私钥或凭据的目录时，生成的 metadata 会错误地把它标成可导入；接收方之后按 bundle 权限导入，安全边界实际依赖发送方诚实声明。

**修复方向：** 对敏感键名/文件类型做保守扫描并默认 `SaveOnly`；允许用户逐项确认脱敏结果后才生成可导入 permissions；敏感 bundle 继续强制 authenticated trusted session。

### NDK-010 — rollback receipt 可被篡改为任意绝对删除路径

**等级：P2 / 本地 confused deputy**<br>
**状态：代码路径确认**

receipt 校验只检查 schema、bundle id、destination 非空和 manifest path 形态，不检查 `destination_path` 是否位于当前 `import_root`：

- [crates/nekodrop-storage/src/bundle.rs:891](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-storage/src/bundle.rs:891>)

rollback 直接把 receipt 的绝对字符串转成 `PathBuf`，再对列出的文件调用 `remove_file`：

- [crates/nekodrop-storage/src/bundle.rs:777](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-storage/src/bundle.rs:777>)
- [crates/nekodrop-storage/src/bundle.rs:812](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/crates/nekodrop-storage/src/bundle.rs:812>)
- [apps/desktop/src-tauri/src/commands/staged_bundles.rs:183](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/commands/staged_bundles.rs:183>)

能写入 `.nekodrop_import_receipts` 的本地进程可以伪造 receipt；当用户或已授权 bridge 触发 rollback 时，桌面端可能代替该进程删除 `import_root` 之外的普通文件。该问题依赖本地篡改能力，属于 sandbox/同用户边界的 confused deputy。

**修复方向：** receipt 只保存相对 bundle id 和受控 import root；读取时 canonicalize 并强制 containment；使用 MAC/签名或原子内部 registry 防止 receipt 篡改；rollback 前再次校验每个目标属于本次 import 目录。

### NDK-011 — Cargo OSV CVSS/vector 未知严重度被当成非阻断

**等级：P2 / 供应链审计 false negative**<br>
**状态：已复现**

`normalizeSeverity()` 只接受 `LOW/MODERATE/MEDIUM/HIGH/CRITICAL`，CVSS vector 会被归为 `UNKNOWN`；`UNKNOWN` 的 rank 为 0，低于默认 `MODERATE` 阈值：

- [scripts/audit-supported-platforms.mjs:15](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/scripts/audit-supported-platforms.mjs:15>)
- [scripts/audit-supported-platforms.mjs:35](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/scripts/audit-supported-platforms.mjs:35>)
- [scripts/audit-supported-platforms.mjs:52](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/scripts/audit-supported-platforms.mjs:52>)
- [scripts/audit-supported-platforms.mjs:124](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/scripts/audit-supported-platforms.mjs:124>)

复现：`normalizeSeverity("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H")` 返回 `UNKNOWN`，`selectBlockingFindings()` 返回 0 个阻断项。

本次真实执行 `npm run security:audit` 时，Cargo OSV 输出了 8 个 `UNKNOWN` findings（`anyhow`、`quick-xml`、`unic-*`），但审计仍打印 `Blocking findings: none` 并退出成功。这个清单本身不等于已经确认的高危漏洞；风险在于未来任何只返回 CVSS vector 的 advisory（包括本应达到阻断阈值的 advisory）都会被降为 informational。当前 `npm audit --omit=dev` 为 0 个 npm runtime vulnerabilities；这不能抵消 Cargo 审计的 fail-open 问题。

**修复方向：** 解析 OSV CVSS v3/v4 base score，或对 `UNKNOWN` 默认按阻断处理；对每种 OSV severity shape 增加测试，并在输出中区分“无漏洞”和“无法判定”。

### NDK-012 — macOS 产物使用 ad-hoc signing

**等级：P2 / 发布供应链信任缺口**<br>
**状态：配置确认**

Tauri 配置明确使用 `signingIdentity: "-"`：

- [apps/desktop/src-tauri/tauri.conf.json:35](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/tauri.conf.json:35>)

README 将安装包发布到 GitHub Releases，并只建议用户自行核对 SHA-256：

- [README.md:86](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/README.md:86>)

SHA-256 只能验证下载内容未被传输篡改，不能证明产物来自项目发布者。ad-hoc 包缺少 Developer ID/notarization 信任链，若发布账户、release asset 或镜像被替换，用户缺少发布者级验证。

**修复方向：** macOS 使用 Developer ID Application + notarization；Windows 配置代码签名；为 release/update metadata 提供签名和构建 provenance，并在发布文档中要求验证签名而不仅是 hash。

### NDK-013 — 设备私钥以明文 JSON 保存

**等级：P2 / 本地密钥保护不足**<br>
**状态：代码路径确认**

`device_identity.json` 同时保存 `secret_seed_hex` 与 Ed25519 `signing_seed_hex`，写入时使用普通 `fs::write`，没有 Keychain/DPAPI、文件权限设置或加密封装：

- [apps/desktop/src-tauri/src/device_identity.rs:91](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/device_identity.rs:91>)
- [apps/desktop/src-tauri/src/device_identity.rs:233](</Users/hisakazu/Projects/nekobuddy/ NekoDrop/apps/desktop/src-tauri/src/device_identity.rs:233>)

拿到该文件的本地进程可以复制设备签名身份，伪造后续 authenticated session identity。`docs/SECURITY.md` 已把 Keychain/Credential Manager 列为 future，但当前发布能力仍依赖明文文件。

**修复方向：** macOS Keychain、Windows Credential Manager/DPAPI；Linux 使用 Secret Service 或明确权限加固；迁移期间对旧明文文件做权限检查并提供 rotation/re-pair 流程。

## 已检查且当前未发现阻断问题的边界

- npm runtime 依赖：`npm audit --omit=dev` 返回 0 个漏洞。
- Rust/desktop/frontend 当前测试：`cargo test --workspace --quiet`（workspace 全部测试通过）、`node --test apps/desktop/test/*.test.*`（145/145）、`npm run test:security-audit`（5/5）、`npm run build` 均通过。
- `npm run security:audit` 退出码为 0，但报告 8 个 `UNKNOWN` Cargo OSV informational findings，并打印 `Blocking findings: none`；这正是 NDK-011 的审计覆盖风险，不应解读为 Rust 依赖“无漏洞”。
- 常规 manifest 的 absolute path、`..`、Windows unsafe segment、checksum mismatch 和 bundle payload symlink 有现有拒绝逻辑；这些保护不能覆盖 NDK-001 的 `.` 根别名或 NDK-005 的 receive 目录现有 symlink。
- local bridge 已限制到 loopback、请求体大小和 header 大小；问题在于调用方身份仍是自报字符串，而非 loopback 绑定本身。

## 建议修复顺序

1. 立即关闭/修复 NDK-001、NDK-002、NDK-003、NDK-004、NDK-005、NDK-006；sidecar `receive` 在修复前不要作为发布入口。
2. 增加协议与 storage 的负向回归测试，覆盖 `bundle_id="."`、counter/nonce/header 篡改、socket timeout、symlink/reparse point、授权 revoke/expiry 后的 queued mutation。
3. 修复 NDK-008 的 offer 资源边界与 checked arithmetic，再修复 bundle secrets metadata、receipt containment 和 dependency audit fail-open。
4. 在发布流水线补签名、notarization、provenance 和私钥安全存储迁移；完成前把 release trust 和 device key protection 标为未完成。
