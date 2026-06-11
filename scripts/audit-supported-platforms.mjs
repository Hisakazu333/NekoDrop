#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { request } from "node:https";
import { pathToFileURL } from "node:url";

export const DEFAULT_SUPPORTED_TARGETS = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-pc-windows-msvc",
];

const DEFAULT_BLOCKING_SEVERITY = "MODERATE";
const OSV_QUERY_BATCH_URL = "https://api.osv.dev/v1/querybatch";
const SEVERITY_RANK = {
  UNKNOWN: 0,
  LOW: 1,
  MODERATE: 2,
  MEDIUM: 2,
  HIGH: 3,
  CRITICAL: 4,
};

export function parseAuditTargets(value) {
  if (!value || !value.trim()) {
    return [...DEFAULT_SUPPORTED_TARGETS];
  }

  return value
    .split(",")
    .map((target) => target.trim())
    .filter(Boolean);
}

export function normalizeSeverity(value) {
  if (!value || typeof value !== "string") {
    return "UNKNOWN";
  }

  const normalized = value.trim().toUpperCase();
  if (normalized === "MEDIUM") {
    return "MODERATE";
  }

  return SEVERITY_RANK[normalized] === undefined ? "UNKNOWN" : normalized;
}

export function severityRank(severity) {
  return SEVERITY_RANK[normalizeSeverity(severity)] ?? SEVERITY_RANK.UNKNOWN;
}

export function selectBlockingFindings(findings, threshold = DEFAULT_BLOCKING_SEVERITY) {
  const minimumRank = severityRank(threshold);
  return findings.filter((finding) => severityRank(finding.severity) >= minimumRank);
}

export function collectResolvedCrates(metadata) {
  if (!metadata?.resolve?.nodes || !Array.isArray(metadata.resolve.nodes)) {
    throw new Error("cargo metadata output is missing resolve.nodes");
  }

  const packagesById = new Map((metadata.packages ?? []).map((pkg) => [pkg.id, pkg]));
  const crates = [];
  const seen = new Set();

  for (const node of metadata.resolve.nodes) {
    const pkg = packagesById.get(node.id);
    if (!pkg?.source?.includes("crates.io")) {
      continue;
    }

    const key = `${pkg.name}@${pkg.version}`;
    if (seen.has(key)) {
      continue;
    }

    seen.add(key);
    crates.push({
      name: pkg.name,
      version: pkg.version,
      purl: `pkg:cargo/${pkg.name}@${pkg.version}`,
    });
  }

  return crates.sort((left, right) => {
    const byName = left.name.localeCompare(right.name);
    return byName === 0 ? left.version.localeCompare(right.version) : byName;
  });
}

export function collectNpmAuditFindings(auditJson) {
  const vulnerabilities = auditJson?.vulnerabilities ?? {};

  return Object.entries(vulnerabilities).flatMap(([name, vulnerability]) => {
    const via = Array.isArray(vulnerability.via) ? vulnerability.via : [];
    const advisories = via.filter((entry) => typeof entry === "object" && entry !== null);

    if (advisories.length === 0) {
      return [
        {
          ecosystem: "npm",
          packageName: name,
          packageVersion: vulnerability.range ?? "unknown",
          id: vulnerability.name ?? name,
          severity: normalizeSeverity(vulnerability.severity),
          summary: `${name} ${vulnerability.range ?? ""}`.trim(),
          target: "npm workspace",
        },
      ];
    }

    return advisories.map((advisory) => ({
      ecosystem: "npm",
      packageName: advisory.name ?? name,
      packageVersion: advisory.range ?? vulnerability.range ?? "unknown",
      id: advisory.url ?? advisory.source ?? advisory.name ?? name,
      severity: normalizeSeverity(advisory.severity ?? vulnerability.severity),
      summary: advisory.title ?? `${name} ${advisory.range ?? ""}`.trim(),
      target: "npm workspace",
    }));
  });
}

function vulnerabilitySeverity(vulnerability) {
  const databaseSeverity = normalizeSeverity(vulnerability?.database_specific?.severity);
  if (databaseSeverity !== "UNKNOWN") {
    return databaseSeverity;
  }

  const severity = vulnerability?.severity;
  if (Array.isArray(severity)) {
    for (const entry of severity) {
      const parsed = normalizeSeverity(entry?.score);
      if (parsed !== "UNKNOWN") {
        return parsed;
      }
    }
  }

  return "UNKNOWN";
}

function cargoFinding(pkg, vulnerability, target) {
  return {
    ecosystem: "crates.io",
    packageName: pkg.name,
    packageVersion: pkg.version,
    id: vulnerability.id,
    severity: vulnerabilitySeverity(vulnerability),
    summary: vulnerability.summary ?? vulnerability.details ?? "",
    target,
  };
}

export function buildOsvQueryBatch(crates) {
  return {
    queries: crates.map((pkg) => ({
      package: {
        ecosystem: "crates.io",
        name: pkg.name,
      },
      version: pkg.version,
    })),
  };
}

export function groupFindingsByIdentity(findings) {
  const groups = new Map();

  for (const finding of findings) {
    const key = [
      finding.ecosystem,
      finding.packageName,
      finding.packageVersion,
      finding.id,
      finding.severity,
      finding.summary,
    ].join("\0");
    const existing = groups.get(key);

    if (existing) {
      existing.targets.push(finding.target);
      continue;
    }

    groups.set(key, {
      ecosystem: finding.ecosystem,
      packageName: finding.packageName,
      packageVersion: finding.packageVersion,
      id: finding.id,
      severity: finding.severity,
      summary: finding.summary,
      targets: [finding.target],
    });
  }

  return [...groups.values()]
    .map((group) => ({
      ...group,
      targets: [...new Set(group.targets)].sort(),
    }))
    .sort((left, right) => {
      const bySeverity = severityRank(right.severity) - severityRank(left.severity);
      if (bySeverity !== 0) {
        return bySeverity;
      }

      const byPackage = `${left.ecosystem}:${left.packageName}`.localeCompare(
        `${right.ecosystem}:${right.packageName}`,
      );
      return byPackage === 0 ? left.id.localeCompare(right.id) : byPackage;
    });
}

function runCommand(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: process.cwd(),
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    stdio: ["ignore", "pipe", "pipe"],
  });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0 && !options.allowFailure) {
    const stderr = result.stderr?.trim();
    const stdout = result.stdout?.trim();
    const output = stderr || stdout || `exit status ${result.status}`;
    throw new Error(`${command} ${args.join(" ")} failed: ${output}`);
  }

  return result;
}

function parseJsonOutput(output, label) {
  try {
    return JSON.parse(output);
  } catch (error) {
    throw new Error(`Could not parse ${label} JSON: ${error.message}`);
  }
}

async function postJson(url, body) {
  return new Promise((resolve, reject) => {
    const payload = JSON.stringify(body);
    const req = request(
      url,
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "content-length": Buffer.byteLength(payload),
          "user-agent": "NekoDrop security audit",
        },
      },
      (res) => {
        let data = "";
        res.setEncoding("utf8");
        res.on("data", (chunk) => {
          data += chunk;
        });
        res.on("end", () => {
          if (!res.statusCode || res.statusCode < 200 || res.statusCode >= 300) {
            reject(new Error(`OSV query failed with HTTP ${res.statusCode}: ${data}`));
            return;
          }

          resolve(parseJsonOutput(data || "{}", "OSV response"));
        });
      },
    );

    req.on("error", reject);
    req.write(payload);
    req.end();
  });
}

async function queryOsvForCrates(crates) {
  if (crates.length === 0) {
    return [];
  }

  const response = await postJson(OSV_QUERY_BATCH_URL, buildOsvQueryBatch(crates));

  return response.results ?? [];
}

async function auditCargoTarget(target) {
  const metadataResult = runCommand("cargo", [
    "metadata",
    "--locked",
    "--filter-platform",
    target,
    "--format-version",
    "1",
  ]);
  const metadata = parseJsonOutput(metadataResult.stdout, `cargo metadata for ${target}`);
  const crates = collectResolvedCrates(metadata);
  const osvResults = await queryOsvForCrates(crates);
  const findings = [];

  osvResults.forEach((result, index) => {
    for (const vulnerability of result.vulns ?? []) {
      findings.push(cargoFinding(crates[index], vulnerability, target));
    }
  });

  return { target, crates, findings };
}

function auditNpm() {
  const result = runCommand("npm", ["audit", "--json"], { allowFailure: true });
  const stdout = result.stdout?.trim();
  if (!stdout) {
    if (result.status === 0) {
      return [];
    }
    throw new Error(result.stderr?.trim() || "npm audit failed without JSON output");
  }

  return collectNpmAuditFindings(parseJsonOutput(stdout, "npm audit"));
}

function printFindings(label, findings) {
  if (findings.length === 0) {
    console.log(`${label}: none`);
    return;
  }

  console.log(`${label}:`);
  for (const finding of groupFindingsByIdentity(findings)) {
    console.log(
      `- [${finding.severity}] ${finding.ecosystem} ${finding.packageName}@${finding.packageVersion} ` +
        `${finding.id} (${finding.targets.join(", ")}) ${finding.summary}`.trimEnd(),
    );
  }
}

async function main() {
  const targets = parseAuditTargets(process.env.NEKODROP_AUDIT_TARGETS);
  const threshold = normalizeSeverity(process.env.NEKODROP_AUDIT_THRESHOLD ?? DEFAULT_BLOCKING_SEVERITY);

  console.log("NekoDrop supported-platform security audit");
  console.log(`Rust targets: ${targets.join(", ")}`);
  console.log(`Blocking severity: ${threshold}`);

  const npmFindings = auditNpm();
  const cargoResults = [];
  for (const target of targets) {
    const result = await auditCargoTarget(target);
    cargoResults.push(result);
    console.log(`Cargo ${target}: ${result.crates.length} resolved crates`);
  }

  const cargoFindings = cargoResults.flatMap((result) => result.findings);
  const allFindings = [...npmFindings, ...cargoFindings];
  const blockingFindings = selectBlockingFindings(allFindings, threshold);
  const informationalFindings = allFindings.filter((finding) => !blockingFindings.includes(finding));

  printFindings("Blocking findings", blockingFindings);
  printFindings("Informational findings", informationalFindings);

  if (blockingFindings.length > 0) {
    throw new Error(`${blockingFindings.length} blocking security finding(s) in supported dependency graph`);
  }

  console.log("No blocking vulnerabilities found in the supported dependency graph.");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
