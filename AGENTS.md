# NekoDrop 项目迭代约定

本文件适用于整个仓库。每次迭代都要交付可验证的完整流程，并同步维护代码结构、测试和相关文档。行数是维护上限，职责清晰和行为正确也是完成条件。

## 项目事实与职责边界

- 开始工作前读取 `README.md`、`docs/product/STATUS.md`、相关代码和测试，检查 `git status --short`，保留已有变更；按实际调用链确认本次范围。
- 当前技术栈：React 19 + Vite + TypeScript 前端（`apps/desktop/src`）、Tauri 2 / Rust 宿主（`apps/desktop/src-tauri`）、Rust workspace 五 crate（`crates/nekodrop-core|network|service|storage` 与 `nekolink-protocol`）、sidecar CLI（`apps/sidecar`）。沿用既有分层与锁文件，不为局部迭代引入另一套框架。
- crate 职责：`nekodrop-core` 配置与领域类型；`nekolink-protocol` 线格式与会话加密；`nekodrop-network` 传输（TCP / iroh）与帧收发；`nekodrop-service` 传输编排（计划、限速、文本暂存）；`nekodrop-storage` 落盘与历史。`src-tauri/commands` 只做桥接，业务下沉 crate。
- 布局按「任务式」组织：主页即设备列表 + 接收卡 + 底部文本发送条，右栏常驻传输面板（`App.tsx` / `Rail.tsx` / `HomeView.tsx` / `TransferPanel.tsx`）。改界面先读这套结构，不回退到横幅藏状态或居中聊天页式布局。
- 线格式是契约：连接码 `nekodrop-v1;transport=…` 字段、serde snake_case 枚举、存储 schema version，变更必须提供兼容或迁移及对应测试。对外路径（如 `nekolink_protocol::SessionTrafficFrameHeader`）保持稳定，模块拆分不得改变引用方。
- 安全立场：默认零云端零遥测；iroh 跨网收件默认关闭，「直连」纯打洞零第三方，「中继」经 n0 公共中继仅转发密文但元数据可见——涉及中继的界面与文档必须保留这条披露。任何「已接入 / 已完成」的声明必须有一个真实调用点或测试作证，只写不验的死代码按未完成处理。
- 已知平台坑：macOS 上 `-webkit-app-region: drag` 区域会吞掉滚轮事件（滚动容器必须 no-drag）；iroh 端点随最后持有者 drop 会吞掉发送队列（服务端必须活过传输期）；Node 25 的 `node --test` 不接受目录参数，必须用 glob 形式。

## 500 / 800 / 1000 行分级管理

以下数字是**单个手写代码文件的行数上限**，按职责选择档位，不是鼓励写满的目标。新增文件默认采用 500 行档。

| 上限 | 适用职责 | 拆分方向 |
| --- | --- | --- |
| 500 行 | 单一组件、视图、样式、工具函数、DTO、简单命令桥接、单项行为的测试 | 按组件、交互能力、样式区域、DTO 或测试行为拆分 |
| 800 行 | 单一领域的状态管理、传输编排、协议某域的实现、同域集成测试 | 将持久化、帧收发、决策等待、历史落盘等独立职责提取到对应模块 |
| 1000 行 | 确有必要保持完整的单一复杂流程（如完整传输会话的收发双端） | 仅在职责内聚、继续拆分会破坏流程可读性时使用；记录理由和后续拆分点 |

- 定档依据实际职责。混合多种独立能力的文件应先拆分，不能为了通过检查直接升档。
- 采用 800 / 1000 行档时，在文件头用简短注释说明职责和理由；交付说明记录适用上限。
- 达到本档上限时，在继续增加代码前安排拆分；**任何手写代码文件超过 1000 行，必须在该文件下次被触碰时完成拆分与回归**，不能只登记待办。
- 未超限但已出现多重职责、重复逻辑或难以测试的耦合，也应重构。命令桥接层（`commands/*`）保持组装职责，业务一律下沉 crate。
- 按物理行统计（含空行与注释）；TS/TSX/CSS/Rust/测试/手写脚本都在范围内。锁文件、构建产物与可再生成物排除，不得把手写代码改称生成物绕过规则。
- 不得通过删注释、压超长行、机械切 `part1/part2` 或复制代码来降低行数。测试按行为拆分，大夹具放专用数据文件，保留完整断言。

本仓库可用以下只读命令扫描行数分布：

```bash
python3 - <<'PY'
from pathlib import Path
import subprocess

suffixes = {'.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs', '.css', '.rs', '.py', '.sh'}
paths = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard']).split(b'\0')
rows = []
for raw in set(paths):
    if not raw:
        continue
    path = Path(raw.decode('utf-8'))
    if path.is_file() and path.suffix in suffixes:
        rows.append((len(path.read_text(encoding='utf-8').splitlines()), str(path)))
for count, path in sorted(rows, reverse=True):
    status = '必须重构：超过 1000 行' if count > 1000 else '核对职责档位' if count > 500 else '检查职责内聚'
    print(f'{count:5}  {path}  {status}')
PY
```

### 已有结构整理项

2026-10-04 初次盘点，以下文件已超过 1000 行硬上限。本约定制定只改文档；**这些文件下次被触碰时必须按职责拆分并通过回归**，不能把快照当永久豁免：

| 文件 | 当前行数 | 拆分方向 |
| --- | --- | --- |
| `apps/desktop/src-tauri/src/commands/tests.rs` | 6205 | 按被测域拆为多个测试文件（send / receive / pairing / local_bridge / device） |
| `crates/nekolink-protocol/src/lib.rs` | 5219 | session traffic 已拆出；继续按 control frame / identity / pairing / cipher 拆 |
| `crates/nekodrop-service/src/lib.rs` | 3262 | 按发送编排 / 接收编排 / 限速 / 文本暂存 / 决策等待拆分 |
| `crates/nekodrop-network/src/tcp_file.rs` | 3111 | 按帧读写、加密会话收发、resume 计划分文件 |
| `apps/desktop/test/genericAdapterSample.test.mjs` | 2286 | 按被测行为拆分测试文件 |
| `docs/examples/generic-adapter/generic-adapter.mjs` | 2268 | 示例按能力分节拆文件 |
| `crates/nekodrop-storage/src/bundle.rs` | 2157 | 按暂存 / 导入 / 回滚 / 清理拆分 |
| `apps/desktop/src-tauri/src/commands/local_bridge/queue.rs` | 1550 | 按请求队列 / 授权 / 事件拆分 |
| `apps/desktop/src-tauri/src/commands/receive.rs` | 1327 | 接受循环已抽 `ReceiveLoopCtx`；继续把决策等待与历史落盘拆出 |
| `apps/desktop/src/context/AppContext.tsx` | 1229 | 继续按域抽 hook（设置/收件箱/桥已拆；发送队列与更新检查下一步） |
| `apps/desktop/src/styles.css` | 1224 | 已按区块注释组织；继续按组件拆分或引入构建期拆分 |

## Git 与发布流程

- 分支模型：`main` 受保护（只接受 PR + squash，禁 merge commit）；日常开发在 `develop`；发布线是 `develop → main` 的 PR。合并后 `develop` 必须 `reset --hard origin/main` 并 force push 同步。
- 所有 git/gh 命令必须检查退出码；PR「已创建 / 已合并」的声明必须以命令成功为据，不得虚报。
- 发版：更新三处版本号（workspace `Cargo.toml`、`apps/desktop/src-tauri/tauri.conf.json`、`apps/desktop/package.json`）→ 写 `docs/release-notes/v<版本>.md`（中英双语，模板见 v0.1.1.md）→ 打 `v*` 标签推送 → Release 工作流自动产出三平台安装包与 SHA256SUMS。
- 自主迭代（workflow）轮次：实现者不跑门禁（脚本统一跑）；独立评审不得为凑数报问题；评审发现逐条带证据进看板；验证门失败的轮次回滚，不带红门交付。

## 重构与迭代流程

1. **明确验收结果。** 从用户操作出发写清输入、可观察结果、失败与恢复方式；发送与接收是双端流程，改动任一端必须验证对端体验。
2. **核实基线。** 读调用链与现有测试，统计相关文件行数；修改前先跑相关测试，区分已有失败和新增问题。
3. **先定拆分边界。** 按域选择文件位置；触发上限先拆分再加功能；不预建没有使用者的抽象。
4. **小步实现并验证。** 重构保持行为不变（先迁移职责再改行为）；保留连接码格式、存储键、serde 命名、事件含义与取消/焦点交互；契约变更必须带兼容迁移和测试。
5. **移除旧路径。** 更新调用方与测试，不留两套处理路径或循环依赖；「只写不验」的死代码与失效的调用链一并清除。
6. **完成回归。** 验证新行为、失败与边界（对端拒绝、断线、磁盘满、恶意帧），再跑全套检查。失败必须修复或明确阻塞，不得把未通过的迭代标记为完成。
7. **复核并交付。** 复核行数、职责与差异，更新文档；报告结果、验证证据和剩余限制。提交与发布按仓库流程走 PR。

## 测试与验证要求

- 优先复用现有测试；新增测试验证用户行为、数据契约或协议不变量（乱序、重放、截断、超长、跨版本迁移），不写只复刻实现的断言。
- 前端大量测试是**源码文本断言**：组件行为改变时同步修正断言以反映新行为，但不允许删断言或删功能凑绿。
- 前端变更：`npm run build`（含 tsc）+ `node --test "apps/desktop/test/*.test.*" "scripts/*.test.mjs"`（Node 25 必须用 glob，不能传目录）。
- Rust 变更：`cargo fmt --all --check`、`cargo clippy --workspace --all-targets`、`cargo test --workspace`；三者任一失败都算迭代未完成。
- 界面与交互变更：浏览器验证真实入口、键盘、取消、空状态、深浅色主题；窗口、拖放、交通灯、原生行为必须在 Tauri 宿主（`npm run tauri dev`）中核验，浏览器预览不算数。
- 传输协议变更：至少一个回环端到端测试（真实 TCP 或 iroh 直连）+ SHA-256 校验断言。
- 纯文档变更检查内容、链接、命令与 `git diff --check`。不得以历史测试记录代替本轮结果。
- 验证使用独立测试数据，避免覆盖用户本地数据；未执行的验证层次明确标注。

常用命令：

```bash
npm run build
node --test "apps/desktop/test/*.test.*" "scripts/*.test.mjs"
git diff --check
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
# 桌面手动验收
npm run tauri dev
```

## 每轮完成条件

- 本次用户流程可实际使用，成功、失败、取消与恢复符合验收范围；发送与接收双端体验都已核对。
- 新增与修改文件符合行数档位；触碰存量超限文件时已完成对应拆分。
- 本轮全套检查通过；实际运行的命令、测试数量和验证层次有据可查，阻塞与未验证项明确。
- 文档与实现一致；「已接入」类声明有真实调用点；`git diff --check` 通过，生成物、凭据、用户数据和无关变更不混入交付。
