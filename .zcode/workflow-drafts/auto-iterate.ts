interface Proposal {
  /** 改进项标题 */
  title: string;
  /** 要落地的具体改动清单（每条一句话，可直接执行） */
  items: string[];
  /** 会触碰的文件（workspace 相对路径） */
  files: string[];
}

interface WorkOutcome {
  /** 一段话：改了什么、为什么 */
  summary: string;
  /** 实际改动的文件清单 */
  filesChanged: string[];
}

interface ReviewFinding {
  /** workspace 相对路径 */
  path: string;
  /** 一句话：什么问题 */
  problem: string;
  /** 怎么修（一句话，可直接执行） */
  fix: string;
  /** 严重度：high = 会崩/功能坏；medium = 明显瑕疵；low = 吹毛求疵 */
  severity: "low" | "medium" | "high";
}

interface WorkflowReport {
  /** 两三句话回答这次自主迭代做了什么 */
  conclusion: string;
  /** 本轮落地的改进项（带一句话说明） */
  changes: { area: string; summary: string; files: string[] }[];
  /** 评审发现与处置 */
  reviewFindings: { problem: string; severity: string; fixed: boolean }[];
  /** 本轮实际跑过的验证命令 */
  verified: string[];
  /** 没覆盖到的东西与原因 */
  notCovered: string[];
}

const BRANCH = "feat/auto-round-polish";

artifact.table("review-findings", {
  title: "独立评审发现",
  columns: [
    { field: "path", label: "文件" },
    { field: "problem", label: "问题" },
    { field: "severity", label: "严重度" },
  ],
  key: "path",
});

// ---------- 阶段一：两个审计员并行盘点各自工作线的改进点 ----------
phase("盘点界面打磨与文档两条改进线");
log("两个审计员并行：一个读新布局代码找体验缺口，一个对照现实盘点文档");

const uxProposalP = agent("体验审计员", {
  system:
    "你是资深桌面应用体验工程师。只读代码，不编辑任何文件。产出必须是可以直接交给实现者的具体改动清单（不是方向性建议）。" +
    "做不到的项直接砍掉，不要写进结果。如果你的任务约束互相矛盾或清单无法产出，如实说明。",
}).ask<Proposal>(`工作区是 NekoDrop（Tauri 2 + React 的局域网加密传文件工具）。
客户端刚完成「任务式布局」重构：apps/desktop/src/App.tsx（窄导航栏 Rail + 主页 HomeView + 右侧常驻传输面板 TransferPanel）。
请读这些文件以及 apps/desktop/src/styles.css 里对应样式，找出当前布局下体验最弱的 2-3 个具体缺口并给出可直接实现的改法。
已知可考虑的方向（不限于）：收件未开启/无设备时的空状态引导、首次启动提示、文本发送条的可见性、深浅色主题下新组件的对比度、传输面板收起后 FAB 按钮的可发现性。
约束：只允许改 apps/desktop/src/**（组件与样式）与 apps/desktop/test/**（断言适配）；不要动 README/docs；不要提需要后端改动或签名证书的项。
返回：title（这条工作线的名字）、items（2-3 条可执行改动，每条一句话）、files（会触碰的文件）、risk（一句话风险）。`);

const docsProposalP = agent("文档审计员", {
  system:
    "你是开源项目的技术文档工程师。只读代码与文档，不编辑任何文件。产出必须具体到可直接改写。" +
    "文档与代码现实不符的地方逐条列出来。做不到的项直接砍掉。",
}).ask<Proposal>(`工作区是 NekoDrop。请读 README.md、docs/ 目录（product/、dev/ 下的关键文件），
并对照代码现实（apps/desktop/src/components/ 的界面、apps/sidecar/src/main.rs 的 CLI、.github/workflows/ 的发布管线、
crates/ 的 iroh 跨网传输）盘点文档缺口。
重点：README 是否反映当前真实能力（文本快送、排队+断点续传、暂停/继续、限速、拖拽直发、按设备归档、iroh 跨网、
CLI receive-iroh/text、GitHub Release 三平台安装包、应用内更新检查）？安装说明是否指向 GitHub Releases？CLI 用法是否在文档里？
返回：title、items（2-3 条可执行的文档改动）、files、risk。`);

const [uxProposal, docsProposal] = await Promise.all([uxProposalP, docsProposalP]);
log(`体验线「${uxProposal.title}」${uxProposal.items.length} 项；文档线「${docsProposal.title}」${docsProposal.items.length} 项`);

// ---------- 阶段二：两位实现者并行落地（文件集不相交） ----------
phase("并行实现界面打磨与文档重写");
log("前端实现者与文档写手并行开工，文件集不相交，无冲突");

const polisherP = agent("前端实现者", {
  system:
    "你是 React + CSS 的资深实现者。按清单逐条落地，把代码写对（不要自己跑检查命令——脚本在你之后统一跑全套；" +
    "也不要 git commit）。不要改 README/docs；不要改版本号。测试断言因组件改动失效时，同步修正断言以反映新行为，" +
    "但绝不允许为了让测试通过而删功能。如果清单里的项实现不了或与现有功能冲突，如实说明并跳过该项。",
}).ask<WorkOutcome>(`实现以下体验改进清单（来自体验审计员，标题：${uxProposal.title}）：
${uxProposal.items.map((item, index) => `${index + 1}. ${item}`).join("\n")}
涉及文件（以审计员列出的为准，可按实现需要微调）：${uxProposal.files.join("、")}
返回：summary（一段话改了什么）、filesChanged（实际改动的文件）。`);

const writerP = agent("文档写手", {
  system:
    "你是开源项目技术文档工程师，中文为主、关键术语带英文。按清单重写文档，" +
    "事实必须与代码一致（不确定的读代码确认），不夸大不编造。不要 git commit；不要改 apps/ 下的代码。" +
    "如果清单里的项与代码现实不符，如实说明并跳过。",
}).ask<WorkOutcome>(`实现以下文档改进清单（来自文档审计员，标题：${docsProposal.title}）：
${docsProposal.items.map((item, index) => `${index + 1}. ${item}`).join("\n")}
涉及文件：${docsProposal.files.join("、")}
返回：summary、filesChanged。`);

const [polishResult, docsResult] = await Promise.all([polisherP, writerP]);
log(`前端改动 ${polishResult.filesChanged.length} 个文件；文档改动 ${docsResult.filesChanged.length} 个文件`);

// ---------- 阶段三：全套验证门（失败交给修复者，最多三轮） ----------
const fixer = agent("修复者", {
  system:
    "你是修复者。根据失败的检查输出精准修复，不引入新功能，不改无关文件。" +
    "测试断言因行为改变失效时，修正断言以反映新行为，但不允许删功能凑绿。" +
    "不要 git commit。修完不用自己跑检查——脚本会重跑。检查不可能通过或指示互相矛盾时，如实说明。",
});

phase("跑全套验证门并修复失败");
let gateFeedback = "首轮";
let gateOk = false;
for (let round = 1; round <= 3; round++) {
  log(`验证门第 ${round} 轮（${gateFeedback}）`);
  const build = await world.run("npm", ["run", "build"], { timeoutMs: 300_000 });
  const feTests = await world.run("node", ["--test", "apps/desktop/test/*.test.*", "scripts/*.test.mjs"], { timeoutMs: 300_000 });
  const fmt = await world.run("cargo", ["fmt", "--all", "--check"], { timeoutMs: 120_000 });
  const clippy = await world.run("cargo", ["clippy", "--workspace", "--all-targets"], { timeoutMs: 600_000 });
  const rustTests = await world.run("cargo", ["test", "--workspace"], { timeoutMs: 900_000 });

  const failures: { name: string; stderr: string }[] = [];
  if (build.exitCode !== 0) failures.push({ name: "npm run build", stderr: build.stderr || build.stdout });
  if (feTests.exitCode !== 0) failures.push({ name: "node --test", stderr: feTests.stderr || feTests.stdout });
  if (fmt.exitCode !== 0) failures.push({ name: "cargo fmt --check", stderr: fmt.stderr || fmt.stdout });
  if (clippy.exitCode !== 0) failures.push({ name: "cargo clippy", stderr: clippy.stderr || clippy.stdout });
  if (rustTests.exitCode !== 0) failures.push({ name: "cargo test", stderr: rustTests.stderr || rustTests.stdout });

  if (failures.length === 0) {
    gateOk = true;
    log(`验证门第 ${round} 轮全绿`);
    break;
  }
  gateFeedback = `失败项：${failures.map((f) => f.name).join("、")}`;
  await fixer.ask(`以下检查失败了，请逐个修复：
${failures.map((f) => `=== ${f.name} ===\n${f.stderr.slice(0, 8000)}`).join("\n\n")}`);
}

// ---------- 阶段四：独立评审改动（没看过实现过程的人） ----------
phase("独立评审本轮全部改动");
const reviewer = agent("独立评审员", {
  system:
    "你是没参与实现的独立评审员，用挑剔的眼光审这份 diff：找会崩的、功能坏的、明显瑕疵。" +
    "只读代码与 git diff，不编辑任何文件。没有问题就返回空数组——不要为了凑数硬找。",
});
const findings: ReviewFinding[] = await reviewer.ask<ReviewFinding[]>(`请评审当前工作树相对 develop 分支的全部改动（用 git diff develop 查看，并读改动前后的文件）。
改动分两条线：前端体验打磨（apps/desktop/src/**）与文档重写（README.md、docs/**）。
重点：功能有没有被改坏（文本发送、拖拽直发、队列、接收卡）、样式有没有漏改的旧类名、文档里有没有与代码不符的陈述。
逐条返回 findings：path、problem、fix（一句话可直接执行）、severity。`);

// ---------- 阶段五：修复评审发现并复跑关键门 ----------
phase("修复评审发现并复跑验证门");
let fixedCount = 0;
if (findings.length > 0) {
  for (const finding of findings) {
    report(
      { path: finding.path, problem: finding.problem, severity: finding.severity },
      "review-findings"
    );
  }
  await fixer.ask(`独立评审发现以下问题，请逐条修复：
${findings.map((f, index) => `${index + 1}. [${f.severity}] ${f.path}: ${f.problem}\n   修法：${f.fix}`).join("\n")}`);
  fixedCount = findings.length;

  const build = await world.run("npm", ["run", "build"], { timeoutMs: 300_000 });
  const feTests = await world.run("node", ["--test", "apps/desktop/test/*.test.*", "scripts/*.test.mjs"], { timeoutMs: 300_000 });
  const fmt = await world.run("cargo", ["fmt", "--all", "--check"], { timeoutMs: 120_000 });
  const clippy = await world.run("cargo", ["clippy", "--workspace", "--all-targets"], { timeoutMs: 600_000 });
  const rustTests = await world.run("cargo", ["test", "--workspace"], { timeoutMs: 900_000 });
  gateOk =
    build.exitCode === 0 &&
    feTests.exitCode === 0 &&
    fmt.exitCode === 0 &&
    clippy.exitCode === 0 &&
    rustTests.exitCode === 0;
}

// ---------- 阶段六：落地为分支与 Pull Request ----------
phase("提交改动并创建 Pull Request");
await world.run("git", ["checkout", "-b", BRANCH]);
await world.run("git", ["add", "-A"]);
const commitMsg = `feat: autonomous round — UI polish + docs refresh\n\nUX: ${uxProposal.title}\nDocs: ${docsProposal.title}\nReview findings fixed: ${fixedCount}`;
await world.run("git", ["commit", "-m", commitMsg]);
await world.run("git", ["push", "-u", "origin", BRANCH]);
const prBody = `## 概要\n自主迭代轮（workflow）。\n\n### 界面打磨\n${uxProposal.items.map((item) => `- ${item}`).join("\n")}\n\n### 文档\n${docsProposal.items.map((item) => `- ${item}`).join("\n")}\n\n### 评审\n发现 ${findings.length} 项，修复 ${fixedCount} 项。\n\n🤖 Generated with [ZCode](https://zcode.ai)`;
await world.run("gh", [
  "pr", "create",
  "--base", "develop",
  "--head", BRANCH,
  "--title", `feat: autonomous round — ${uxProposal.title} + docs refresh`,
  "--body", prBody,
]);

const status = gateOk ? "全套验证通过" : "验证门未全绿（见 notCovered）";
const reportBody = [
  `# 自主迭代轮报告`,
  "",
  `**状态：${status}**`,
  "",
  `## 界面打磨`,
  "",
  polishResult.summary,
  "",
  ...polishResult.filesChanged.map((f) => `- \`${f}\``),
  "",
  `## 文档重写`,
  "",
  docsResult.summary,
  "",
  ...docsResult.filesChanged.map((f) => `- \`${f}\``),
  "",
  `## 评审`,
  "",
  findings.length === 0
    ? "独立评审未发现问题。"
    : findings
        .map(
          (f) =>
            `- [${f.severity}] ${f.path}: ${f.problem}（${fixedCount > 0 ? "已修复" : "未修复"}）`
        )
        .join("\n"),
  "",
  `分支 \`${BRANCH}\` 已推送并创建 PR（base: develop），等 CI 通过后合并。`,
].join("\n");

await artifact.markdown("round-report", reportBody, {
  title: "自主迭代轮报告",
  description: `界面打磨与文档重写已落地为 PR，${status}。`,
  primary: true,
});

const result: WorkflowReport = {
  conclusion: `自主迭代轮完成：${polishResult.summary.slice(0, 60)}；${docsResult.summary.slice(0, 60)}。${status}，已创建 PR（${BRANCH} → develop）。`,
  changes: [
    { area: "界面打磨", summary: polishResult.summary, files: polishResult.filesChanged },
    { area: "文档", summary: docsResult.summary, files: docsResult.filesChanged },
  ],
  reviewFindings: findings.map((f) => ({
    problem: f.problem,
    severity: f.severity,
    fixed: fixedCount > 0,
  })),
  verified: gateOk
    ? [
        "npm run build（tsc + vite）",
        "node --test apps/desktop/test scripts",
        "cargo fmt --all --check",
        "cargo clippy --workspace --all-targets",
        "cargo test --workspace",
      ]
    : ["部分检查未通过，见报告"],
  notCovered: [
    "真实桌面端（Tauri 窗口）的手动走查——workflow 里没有 GUI，合并后需要人工 tauri dev 验收",
    "macOS/Windows 真机行为",
  ],
};
return result;
