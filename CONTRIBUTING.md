# 贡献指南

感谢你关注 NekoDrop！本文档说明如何参与开发：环境搭建、分支模型、提交规范和 PR 流程。

- 想报告问题或提功能建议：先开 [issue](https://github.com/Hisakazu333/NekoDrop/issues) 讨论。
- 想报告安全漏洞：看[安全策略](docs/dev/SECURITY.md)，不要在公开 issue 里写漏洞细节。
- 参与讨论请遵守[行为准则](CODE_OF_CONDUCT.md)。

## 项目现状

NekoDrop 处于 beta 收口阶段，当前主线是 macOS / Windows 局域网传输。这个仓库不是云盘，也不是远程控制工具；NekoLink 后续承接 bundle、local bridge、transport 和跨设备 Agent 协作，但协议不能写死到某一个第三方应用里。

最适合的 PR 是小的、边界清楚的：

**欢迎**：文档修正、补测试、错误提示改进、打包脚本小修、边界清晰的 protocol / storage 改动、给已有 bug 写复现测试。

**先开 issue 讨论**：bundle 导入冲突策略、adapter 样例与真实导入导出、local bridge 事件流、key rotation、iroh / relay / P2P、手机端互通。

**不要直接提 PR**：大 UI 重写、绕过 NekoLink 协议自加传输方式、自动导入 session / skill / workspace、默认同步密钥或隐私目录、把未完成能力写成已完成（[STATUS.md](docs/product/STATUS.md) 是状态的唯一事实来源，README 不得超出它）。

## 开发环境

| 依赖 | 版本 |
| --- | --- |
| Rust | 最新 stable（`rustup update`） |
| Node.js | ≥ 22 |
| npm | ≥ 10 |
| 平台 | macOS 13+ 或 Windows 10+（Linux 仅供 CI 构建验证） |

```bash
# 1. 克隆并安装依赖（仓库根目录是 npm workspace）
git clone git@github.com:Hisakazu333/NekoDrop.git
cd NekoDrop
npm install

# 2. 桌面端开发模式（Tauri dev 窗口 + 热更新）
npm run tauri:dev          # 或 npm run dev 仅启动前端

# 3. 发布构建
npm run build              # tsc + vite build
npm run tauri:build        # 桌面安装包
```

## 代码结构

```text
apps/
  desktop/        Tauri 2 桌面应用（React 19 + TypeScript + Vite）
    src/          前端：组件、状态、类型
    src-tauri/    Rust 命令层、应用状态、系统集成
  sidecar/        无 UI 的 CLI 接收端（兼容明文模式）
crates/
  nekolink-protocol   协议库：信封、加密会话、bundle、local bridge 模型
  nekodrop-core       领域类型：设备、清单、传输任务、配对
  nekodrop-network    连接码、TCP transport、网络帧
  nekodrop-storage    路径安全、checksum、partial/resume、bundle 暂存
  nekodrop-service    收发编排：把 protocol / storage / network 串起来
docs/              按读者分组：product/ dev/ testing/ examples/ archive/
scripts/           打包与审计脚本
```

**模块边界**（改代码时不要跨层调用）：

- `nekolink-protocol` 不依赖 Tauri、React、桌面 UI。
- `nekodrop-storage` 不做网络连接，不做 UI 状态。
- `nekodrop-network` 不写入最终文件。
- 桌面前端只展示状态和发命令；文件扫描、hash、落盘、信任策略不写在前端。
- UI 改动与协议 / Rust 改动分 PR 提交。

## 分支模型

自 2026-10 起使用简化的三段式模型，替代旧的 `main / develop / desktop-develop / docs-develop / 个人长期分支` 多分支模型：

```text
main      发布主线（受保护：只接受 PR、squash merge、必须过 CI）
develop   集成分支，所有日常 PR 的目标
feat|fix|docs|chore/<topic>   从 develop 切出的短生命周期分支
```

日常流程：

```bash
# 从 develop 开出主题分支
git checkout develop && git pull --ff-only
git checkout -b fix/transfer-timeout

# 开发、提交、推送
git push -u origin fix/transfer-timeout

# 开 PR 到 develop；CI 通过后 squash merge 并删除远端分支
```

发布流程：定期从 `develop` 向 `main` 发 release PR（squash merge），从 `main` 打 tag、出安装包。

> 注意：release PR squash 合并后，`develop` 必须立刻重置到 `main`（`git checkout develop && git reset --hard origin/main && git push --force-with-lease origin develop`），否则 squash 产生的平行历史会让下一个 release PR 显示冲突。

规则：

- 一个 PR 只做一件事；不要混合文档、UI、协议、安全、打包。
- 分支名用 `feat/<topic>`、`fix/<topic>`、`docs/<topic>`、`chore/<topic>`，不使用个人长期分支。
- 主题分支生命周期尽量短（理想 < 1 周），长期不合并的分支会被清理（重要 WIP 会先打 `archive/<name>` 标签保留）。

## 提交规范

使用 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/)，格式 `type(scope): subject`：

```text
feat(desktop): 添加传输邀请接受界面
fix(network): 修复接收线程缺少读超时导致的挂死
docs: 重写贡献指南
refactor(storage): 统一 partial 文件生命周期
test(protocol): 补齐回放窗口边界用例
chore(deps): 升级 postcss
```

常用 type：`feat` `fix` `docs` `test` `refactor` `perf` `chore`。scope 可选（crate 名或模块名）。subject 用一句话说清改动，中文或英文均可但不要混排。

## 测试与质量门

提 PR 前在本地跑完并全绿（CI 会重跑同样的检查）：

```bash
cargo fmt --all                                    # 格式化
cargo clippy --workspace --all-targets             # 静态检查
cargo test --workspace                             # Rust 全部测试
node --test apps/desktop/test/*.test.*             # 前端测试
npm run build                                      # tsc 类型检查 + vite 构建
```

测试要求：

- 新逻辑配测试；修 bug 先写复现测试再修。
- `nekolink-protocol` 和 `nekodrop-storage` 是测试重点区。
- 前端测试请写行为断言，避免只对源码做正则匹配的"文本断言"。

## PR 检查清单

提 PR 前自查：

- [ ] PR 只做一件事，标题符合 Conventional Commits
- [ ] 本地 `cargo test` / `node --test` / `npm run build` 全部通过
- [ ] 新增能力同步更新了 [STATUS.md](docs/product/STATUS.md)（不夸大）
- [ ] 涉及协议 / 安全模型的改动附测试证据
- [ ] 没有引入未讨论过的新依赖
- [ ] UI 改动附截图；跨平台行为说明清楚

## 文档

文档在 [docs/](docs/README.md) 按读者分组：`product/`（产品、状态、路线图）、`dev/`（开发、架构、协议、规范）、`testing/`、`examples/`、`archive/`（历史归档，不代表当前方向）。改动文档时保持索引（docs/README.md）与正文一致：

- 新功能合并后先更新 [STATUS.md](docs/product/STATUS.md)。
- README 只写用户现在能理解和能验证的能力。
- 协议细节写[协议文档](docs/dev/PROTOCOL.md)，安全边界写[安全模型](docs/dev/SECURITY.md)，bundle 规则写 [Bundle 规范](docs/dev/BUNDLE_SPEC.md)。
- 不要把 roadmap 里的东西写成已经完成。

## 发布

维护者从 `develop` 向 `main` 发 release PR；合并后：

```bash
git tag -a v0.x.0 -m "release: v0.x.0"
git push origin v0.x.0
bash scripts/package-desktop.sh        # macOS DMG
```

发布时至少记录：安装包与 SHA256、对应 commit、已验证的系统版本、已知限制。当前还不能叫 stable；文件 payload 加密、replay protection、长期设备身份密钥、跨网络 transport 都还没完成。

变更记录写入 [CHANGELOG.md](CHANGELOG.md)。
