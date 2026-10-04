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
  /** 上一工作流 PR 的接管结果 */
  takeover: string;
  rounds: { round: number; title: string; status: string; summary: string }[];
  reviewFindings: { round: string; problem: string; severity: string; fixed: boolean }[];
  verified: string[];
  notCovered: string[];
}

const PREV_BRANCH = "feat/auto-code-iterations";
const BRANCH = "feat/auto-code-iterations-2";
const MAX_ROUNDS = 6;


const result = {
  conclusion:
    "人工已完成接管（PR 经 develop 直连发布线合并，main=develop），本工作流的等待前提失效，按计划立即结束。",
  takeover: "人工接管完成",
  rounds: [],
  reviewFindings: [],
  verified: [],
  notCovered: ["原计划的 6 轮迭代改由后续工作流执行"],
};
return result;
