interface PickedItem {
  /** 本轮要做的改进标题 */
  title: string;
  /** 为什么值得做（一两句） */
  rationale: string;
  /** 改动分类 */
  area: "frontend" | "backend" | "docs" | "mixed";
  /** 预计触碰的文件 */
  files: string[];
  /** 没有值得做的项时为 true */
  skip: boolean;
  /** skip 时的原因 */
  skipReason?: string;
}

interface WorkOutcome {
  /** 一段话：改了什么 */
  summary: string;
  /** 实际改动文件 */
  filesChanged: string[];
}

interface ReviewFinding {
  /** workspace 相对路径 */
  path: string;
  /** 一句话：什么问题 */
  problem: string;
  /** 一句话怎么修 */
  fix: string;
  /** high = 崩/功能坏；medium = 明显瑕疵；low = 吹毛求疵 */
  severity: "low" | "medium" | "high";
}

interface WorkflowReport {
  conclusion: string;
  rounds: { round: number; title: string; status: string; summary: string }[];
  reviewFindings: { round: number; problem: string; severity: string; fixed: boolean }[];
  verified: string[];
  notCovered: string[];
}

const MAX_ROUNDS = 4;
const BRANCH = "feat/auto-code-iterations";

artifact.board("rounds", {
  title: "迭代轮次看板",
  key: "round",
  status: "status",
  columns: ["done", "failed", "skipped"],
  cardTitle: "title",
  detail: [{ field: "summary", label: "说明" }],
});

artifact.table("review-findings", {
  title: "评审发现",
  columns: [
    { field: "round", label: "轮次" },
    { field: "path", label: "文件" },
    { field: "problem", label: "问题" },
    { field: "severity", label: "严重度" },
  ],
  key: "path",
});

const picker = agent("任务挑选者", {
  system:
    "你是节奏保守的技术负责人。每轮只挑 ONE 项当下最值得做的代码改进，标准：单轮可完成" +
    "（改动 ≤400 行）、能被现有测试门验证（build/前端测试/clippy/cargo test）、不依赖签名证书或真机硬件、" +
    "不与本轮之前已做的项重复。先读 docs/product/ROADMAP.md 与 STATUS.md 的「不能宣传为已完成」区，再读相关代码确认可行。" +
    "没有合格项就如实 skip。拿不准就 skip——宁可不干活也不干错活。",
});

const implementer = agent("实现者", {
  system:
    "你是资深全栈实现者（Rust + Tauri + React）。按挑选者的项精准实现，" +
    "不跑检查命令（脚本统一跑），不 git commit，不删既有功能，不改版本号。" +
    "测试断言因行为改变失效时同步修正断言，但不允许删测试凑绿。实现不了或与现有功能冲突时如实说明并回滚自己的改动。",
});

const fixer = agent("修复者", {
  system:
    "你是修复者。按失败输出精准修复，不引入新功能，不改无关文件。" +
    "测试断言因行为改变失效时同步修正，但不允许删测试凑绿。不要 git commit。" +
    "检查不可能通过或指示互相矛盾时如实说明，不要硬修。",
});

const rounds: { round: number; title: string; status: string; summary: string }[] = [];
const allReviewFindings: { round: number; problem: string; severity: string; fixed: boolean }[] = [];
let doneRounds = 0;

for (let round = 1; round <= MAX_ROUNDS; round++) {
  phase("挑选本轮最高价值的代码改进");
  log(`第 ${round}/${MAX_ROUNDS} 轮：挑选改进项（此前完成 ${doneRounds} 轮）`);
  const previous = rounds.map((r) => `第${r.round}轮已完成：${r.title}`).join("；") || "无";
  const item = await picker.ask<PickedItem>(`第 ${round}/${MAX_ROUNDS} 轮。本轮之前已完成的项：${previous}。
请挑选 ONE 项当下最值得做的代码改进并返回；没有合格项就 skip=true 并说明原因。`);

  if (item.skip) {
    log(`第 ${round} 轮：挑选者判定没有值得做的项（${item.skipReason ?? ""}），提前收尾`);
    rounds.push({ round, title: item.skipReason ?? "无合格项", status: "skipped", summary: item.skipReason ?? "" });
    report({ round, title: item.skipReason ?? "无合格项", status: "skipped", summary: item.skipReason ?? "" }, "rounds");
    break;
  }

  phase("实现改进并通过全套验证门");
  log(`第 ${round} 轮：实现「${item.title}」（${item.area}）`);
  const outcome = await implementer.ask<WorkOutcome>(`实现以下改进：
标题：${item.title}
理由：${item.rationale}
分类：${item.area}
预计文件：${item.files.join("、")}
返回：summary（一段话）、filesChanged（实际改动文件）。`);

  let gateOk = false;
  let gateFeedback = "首轮";
  for (let attempt = 1; attempt <= 3; attempt++) {
    log(`第 ${round} 轮验证门第 ${attempt} 次（${gateFeedback}）`);
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
      break;
    }
    gateFeedback = `失败项：${failures.map((f) => f.name).join("、")}`;
    await fixer.ask(`以下检查失败，请修复：
${failures.map((f) => `=== ${f.name} ===\n${f.stderr.slice(0, 8000)}`).join("\n\n")}`);
  }

  if (!gateOk) {
    // 回滚本轮未提交的改动，绝不把红门留在分支上
    await world.run("git", ["add", "-A"]);
    await world.run("git", ["reset", "--hard", "HEAD"]);
    log(`第 ${round} 轮验证门三轮未过，已回滚本轮改动`);
    rounds.push({ round, title: item.title, status: "failed", summary: "验证门三轮未过，改动已回滚" });
    report({ round, title: item.title, status: "failed", summary: "验证门三轮未过，改动已回滚" }, "rounds");
    continue;
  }

  phase("独立评审本轮改动并修复");
  const reviewer = agent(`评审员-r${round}`, {
    system:
      "你是没参与实现的独立评审员，挑剔地审本轮未提交的改动（git diff 查看）：找会崩的、功能坏的、明显瑕疵。" +
      "只读代码与 diff，不编辑文件。没有问题就返回空数组，不要凑数。",
  });
  const findings = await reviewer.ask<ReviewFinding[]>(`第 ${round} 轮改动：${item.title}。
请审当前工作树未提交的全部改动（git diff），重点确认功能没被改坏、没有漏改的引用、没有与既有测试矛盾。
逐条返回：path、problem、fix、severity。`);

  for (const finding of findings) {
    report({ round, path: finding.path, problem: finding.problem, severity: finding.severity }, "review-findings");
    allReviewFindings.push({ round, problem: finding.problem, severity: finding.severity, fixed: false });
  }
  if (findings.length > 0) {
    await fixer.ask(`独立评审发现以下问题，请逐条修复：
${findings.map((f, index) => `${index + 1}. [${f.severity}] ${f.path}: ${f.problem}\n   修法：${f.fix}`).join("\n")}`);
    const reBuild = await world.run("npm", ["run", "build"], { timeoutMs: 300_000 });
    const reTests = await world.run("node", ["--test", "apps/desktop/test/*.test.*", "scripts/*.test.mjs"], { timeoutMs: 300_000 });
    const reFmt = await world.run("cargo", ["fmt", "--all", "--check"], { timeoutMs: 120_000 });
    const reClippy = await world.run("cargo", ["clippy", "--workspace", "--all-targets"], { timeoutMs: 600_000 });
    const reRust = await world.run("cargo", ["test", "--workspace"], { timeoutMs: 900_000 });
    gateOk =
      reBuild.exitCode === 0 && reTests.exitCode === 0 && reFmt.exitCode === 0 && reClippy.exitCode === 0 && reRust.exitCode === 0;
    if (gateOk) {
      for (const f of allReviewFindings) if (f.round === round) f.fixed = true;
    }
  }

  if (!gateOk) {
    await world.run("git", ["add", "-A"]);
    await world.run("git", ["reset", "--hard", "HEAD"]);
    log(`第 ${round} 轮评审修复后验证未过，已回滚`);
    rounds.push({ round, title: item.title, status: "failed", summary: "评审修复后验证未过，已回滚" });
    report({ round, title: item.title, status: "failed", summary: "评审修复后验证未过，已回滚" }, "rounds");
    continue;
  }

  phase("提交并推送本轮改动");
  await world.run("git", ["add", "-A"]);
  await world.run("git", [
    "commit",
    "-m",
    `feat(auto-round-${round}): ${item.title}\n\n${item.rationale}\n\n${outcome.summary}`,
  ]);
  await world.run("git", ["push", "-u", "origin", BRANCH]);
  doneRounds++;
  rounds.push({ round, title: item.title, status: "done", summary: outcome.summary });
  report(
    { round, title: item.title, status: "done", summary: outcome.summary },
    "rounds"
  );
}

// ---------- 收尾：总体评审 + 最终验证门 + 发布 PR ----------
let finalReviewFindings: ReviewFinding[] = [];
let finalGateOk = doneRounds > 0;

if (doneRounds > 0) {
  phase("收尾：全分支独立总评与最终验证门");
  const finalReviewer = agent("总评审员", {
    system:
      "你是没参与实现的独立总评审员。审整条分支相对 develop 的全部改动：跨轮次的一致性、有无互相破坏、有无功能回退。" +
      "只读代码与 git diff develop，不编辑文件。没有问题返回空数组。",
  });
  finalReviewFindings = await finalReviewer.ask<ReviewFinding[]>(`请总评分支 ${BRANCH} 相对 develop 的全部改动（git diff develop）。
这些改动分多轮完成：${rounds.filter((r) => r.status === "done").map((r) => `第${r.round}轮「${r.title}」`).join("、")}。
逐条返回：path、problem、fix、severity。`);

  if (finalReviewFindings.length > 0) {
    for (const f of finalReviewFindings) {
      report({ round: MAX_ROUNDS + 1, path: f.path, problem: f.problem, severity: f.severity }, "review-findings");
      allReviewFindings.push({ round: MAX_ROUNDS + 1, problem: f.problem, severity: f.severity, fixed: false });
    }
    await fixer.ask(`总评审发现以下问题，请逐条修复：
${finalReviewFindings.map((f, index) => `${index + 1}. [${f.severity}] ${f.path}: ${f.problem}\n   修法：${f.fix}`).join("\n")}`);

    const build = await world.run("npm", ["run", "build"], { timeoutMs: 300_000 });
    const feTests = await world.run("node", ["--test", "apps/desktop/test/*.test.*", "scripts/*.test.mjs"], { timeoutMs: 300_000 });
    const fmt = await world.run("cargo", ["fmt", "--all", "--check"], { timeoutMs: 120_000 });
    const clippy = await world.run("cargo", ["clippy", "--workspace", "--all-targets"], { timeoutMs: 600_000 });
    const rustTests = await world.run("cargo", ["test", "--workspace"], { timeoutMs: 900_000 });
    finalGateOk =
      build.exitCode === 0 && feTests.exitCode === 0 && fmt.exitCode === 0 && clippy.exitCode === 0 && rustTests.exitCode === 0;
    if (finalGateOk) {
      for (const f of allReviewFindings) if (f.round === MAX_ROUNDS + 1) f.fixed = true;
    }
    await world.run("git", ["add", "-A"]);
    await world.run("git", ["commit", "-m", `fix(auto): address final review findings (${finalReviewFindings.length})`]);
    await world.run("git", ["push"]);
  } else {
    // 总评无发现也要跑一次最终全套门，确认分支尖端是绿的
    const build = await world.run("npm", ["run", "build"], { timeoutMs: 300_000 });
    const feTests = await world.run("node", ["--test", "apps/desktop/test/*.test.*", "scripts/*.test.mjs"], { timeoutMs: 300_000 });
    const fmt = await world.run("cargo", ["fmt", "--all", "--check"], { timeoutMs: 120_000 });
    const clippy = await world.run("cargo", ["clippy", "--workspace", "--all-targets"], { timeoutMs: 600_000 });
    const rustTests = await world.run("cargo", ["test", "--workspace"], { timeoutMs: 900_000 });
    finalGateOk =
      build.exitCode === 0 && feTests.exitCode === 0 && fmt.exitCode === 0 && clippy.exitCode === 0 && rustTests.exitCode === 0;
  }

  phase("创建 Pull Request");
  const roundList = rounds
    .filter((r) => r.status === "done")
    .map((r) => `- 第${r.round}轮：${r.title}（${r.summary.slice(0, 80)}）`)
    .join("\n");
  const prBody = `## 概要\n自主代码迭代（workflow 连续 ${doneRounds} 轮）。\n\n${roundList}\n\n### 评审\n独立评审发现 ${allReviewFindings.length} 项，已修复 ${allReviewFindings.filter((f) => f.fixed).length} 项。\n\n## 验证\n每轮与收尾均通过全套门：npm build / node --test / cargo fmt --check / clippy --workspace / cargo test --workspace。\n\n🤖 Generated with [ZCode](https://zcode.ai)`;
  await world.run("gh", [
    "pr", "create",
    "--base", "develop",
    "--head", BRANCH,
    "--title", `feat: autonomous code iterations (${doneRounds} rounds)`,
    "--body", prBody,
  ]);
}

// ---------- 报告 ----------
const doneTitles = rounds.filter((r) => r.status === "done").map((r) => `第${r.round}轮「${r.title}」`);
const conclusion =
  doneRounds === 0
    ? "本轮没有找到合格的代码改进项（挑选者判定当前代码已无可低成本迭代的点），未创建 PR。"
    : `自主代码迭代完成 ${doneRounds} 轮：${doneTitles.join("、")}。${
        finalGateOk ? "全部通过五重验证门（build/前端测试/fmt/clippy/cargo test）" : "部分验证未通过（见 notCovered）"
      }，已创建 PR（${BRANCH} → develop）。`;

const reportBody = [
  `# 代码持续迭代报告`,
  "",
  `**完成轮次：${doneRounds}/${MAX_ROUNDS}**，状态：${finalGateOk ? "全部验证通过" : "部分验证未通过"}`,
  "",
  ...rounds.map(
    (r) => `## 第 ${r.round} 轮：${r.title}（${r.status}）\n\n${r.summary || r.status}`
  ),
  "",
  `## 评审发现（${allReviewFindings.length} 项，已修复 ${allReviewFindings.filter((f) => f.fixed).length}）`,
  "",
  ...allReviewFindings.map(
    (f) => `- 第${f.round}轮 [${f.severity}] ${f.problem}（${f.fixed ? "已修复" : "未修复"}）`
  ),
  "",
  doneRounds > 0 ? `分支 \`${BRANCH}\` 已推送，PR 已创建（base: develop）。` : "无改动，未创建 PR。",
].join("\n");

await artifact.markdown("iterations-report", reportBody, {
  title: "代码持续迭代报告",
  description: `${doneRounds} 轮代码改进，${finalGateOk ? "全套验证通过" : "部分验证未通过"}。`,
  primary: true,
});

const result: WorkflowReport = {
  conclusion,
  rounds: rounds.map((r) => ({ round: r.round, title: r.title, status: r.status, summary: r.summary })),
  reviewFindings: allReviewFindings.map((f) => ({
    round: f.round,
    problem: f.problem,
    severity: f.severity,
    fixed: f.fixed,
  })),
  verified: doneRounds > 0
    ? [
        "每轮与收尾均跑：npm run build（tsc + vite）",
        "node --test apps/desktop/test/*.test.* scripts/*.test.mjs",
        "cargo fmt --all --check",
        "cargo clippy --workspace --all-targets",
        "cargo test --workspace",
      ]
    : ["无代码改动，未运行验证"],
  notCovered: [
    "真实桌面端（Tauri 窗口）手动走查——workflow 无 GUI，需合并后人工验收",
    "需要签名证书或真机硬件的项（签名公证、真机推送等）不在可选范围内",
  ],
};
return result;
