#!/usr/bin/env node

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..", "..", "..");
const SAMPLE_ROOTS = {
  "session-summary": path.resolve(REPO_ROOT, "docs/bundle-samples/session-summary"),
};

export const GENERIC_ADAPTER_CLIENT = {
  client_id: "generic.adapter",
  display_name: "Generic Adapter",
  app_kind: "agent",
};

export const ACTION_LIFECYCLE_STATUSES = [
  "queued",
  "running",
  "succeeded",
  "failed",
  "conflict",
  "cancelled",
];

export const BUNDLE_DETAIL_STATUSES = ["saved", "imported"];
export const BRIDGE_RESPONSE_STATUSES = [
  "ok",
  "unsupported",
  "pending_auth",
  "pending_runtime",
];

export function resolveSampleRoot(sampleName = "session-summary") {
  const sampleRoot = SAMPLE_ROOTS[sampleName];
  if (!sampleRoot) {
    throw new Error(`unsupported sample: ${sampleName}`);
  }
  return sampleRoot;
}

export function exportSampleBundle({
  sampleName = "session-summary",
  outputRoot = mkdtempSync(path.join(os.tmpdir(), "generic-adapter-")),
} = {}) {
  const sampleRoot = resolveSampleRoot(sampleName);
  const bundleRoot = path.resolve(outputRoot);
  rmSync(bundleRoot, { recursive: true, force: true });
  mkdirSync(bundleRoot, { recursive: true });
  cpSync(sampleRoot, bundleRoot, { recursive: true });
  return readBundleSnapshot(bundleRoot);
}

export function readBundleSnapshot(bundleRoot) {
  const root = path.resolve(bundleRoot);
  const manifest = readJson(path.join(root, "bundle.json"));
  const checksums = readJson(path.join(root, "checksums.json"));
  const permissions = readJson(path.join(root, "permissions.json"));
  const files = manifest.files.map((file) => readBundleFile(root, file));

  assert.equal(manifest.summary.file_count, files.length);
  assert.equal(
    manifest.summary.total_bytes,
    files.reduce((total, file) => total + file.size, 0),
  );

  return {
    bundle_root: root,
    manifest,
    checksums,
    permissions,
    files,
    import_allowed: !permissions.secrets.contains_secrets,
    staging_status: "saved",
  };
}

export function buildAuthorizationRequest({
  requestId = "adapter-auth-001",
  client = GENERIC_ADAPTER_CLIENT,
  requestedScopes = [
    "device.read",
    "bundle.send",
    "bundle.import.request",
    "transfer.status.read",
  ],
  reason = "Send and import the selected session bundle",
  ttlSeconds = 3600,
} = {}) {
  return {
    kind: "authorization.request",
    payload: {
      request_id: requestId,
      client,
      requested_scopes: requestedScopes,
      reason,
      ttl_seconds: ttlSeconds,
    },
  };
}

export function buildSendRequest({
  requestId = "adapter-send-001",
  client = GENERIC_ADAPTER_CLIENT,
  targetDeviceId = "neko-device-target",
  bundleRoot,
  bundleType = "session",
  requireTrustedDevice = true,
} = {}) {
  assertBundleRoot(bundleRoot);
  return {
    kind: "bundle.send",
    payload: {
      request_id: requestId,
      client,
      target_device_id: targetDeviceId,
      bundle_root: path.resolve(bundleRoot),
      bundle_type: bundleType,
      require_trusted_device: requireTrustedDevice,
    },
  };
}

export function buildBundleDetailRequest({
  requestId = "adapter-detail-001",
  client = GENERIC_ADAPTER_CLIENT,
  stagedBundleId,
} = {}) {
  assertNonEmpty("stagedBundleId", stagedBundleId);
  return {
    kind: "bundle.detail",
    payload: {
      request_id: requestId,
      client,
      staged_bundle_id: stagedBundleId,
    },
  };
}

export function buildEventsPollRequest({
  requestId = "adapter-events-001",
  client = GENERIC_ADAPTER_CLIENT,
  afterEventId = null,
  limit = 20,
  timeoutMs = 15_000,
} = {}) {
  return {
    kind: "events.poll",
    payload: {
      request_id: requestId,
      client,
      after_event_id: afterEventId,
      limit,
      timeout_ms: timeoutMs,
    },
  };
}

export function buildActionResultsRequest({
  requestId = "adapter-results-001",
  client = GENERIC_ADAPTER_CLIENT,
  afterClaimedAtMs = null,
  limit = 20,
} = {}) {
  return {
    kind: "actions.results",
    payload: {
      request_id: requestId,
      client,
      after_claimed_at_ms: afterClaimedAtMs,
      limit,
    },
  };
}

export function buildImportRequest({
  requestId = "adapter-import-001",
  client = GENERIC_ADAPTER_CLIENT,
  stagedBundleId,
  expectedBundleType = "session",
} = {}) {
  assertNonEmpty("stagedBundleId", stagedBundleId);
  return {
    kind: "bundle.import",
    payload: {
      request_id: requestId,
      client,
      staged_bundle_id: stagedBundleId,
      expected_bundle_type: expectedBundleType,
    },
  };
}

export function buildBundleDetailPreview(snapshot, { status = "saved" } = {}) {
  assertStatus(status, BUNDLE_DETAIL_STATUSES, "bundle detail status");
  return {
    bundle_id: snapshot.manifest.bundle_id,
    bundle_type: bundleTypeLabel(snapshot.manifest.bundle_type),
    display_name: snapshot.manifest.display_name,
    source_app: snapshot.manifest.source_app,
    file_count: snapshot.manifest.summary.file_count,
    total_bytes: snapshot.manifest.summary.total_bytes,
    staging_path: snapshot.bundle_root,
    import_allowed: snapshot.import_allowed,
    staging_status: status,
    can_import_now: snapshot.import_allowed,
    import_path: null,
  };
}

export function buildReceipt(snapshot, {
  detailStatus = "imported",
  actionLifecycleStatus = "succeeded",
  eventLifecycleStatus = "succeeded",
} = {}) {
  assertStatus(detailStatus, BUNDLE_DETAIL_STATUSES, "receipt staging status");
  assertStatus(actionLifecycleStatus, ACTION_LIFECYCLE_STATUSES, "action lifecycle status");
  assertStatus(eventLifecycleStatus, ACTION_LIFECYCLE_STATUSES, "event lifecycle status");
  return {
    receipt_id: `receipt_${snapshot.manifest.bundle_id}`,
    bundle_id: snapshot.manifest.bundle_id,
    bundle_type: bundleTypeLabel(snapshot.manifest.bundle_type),
    display_name: snapshot.manifest.display_name,
    source_app: snapshot.manifest.source_app,
    bundle_detail_staging_status: detailStatus,
    action_lifecycle_status: actionLifecycleStatus,
    event_lifecycle_status: eventLifecycleStatus,
    bundle_detail_status_words: BUNDLE_DETAIL_STATUSES,
    action_status_words: ACTION_LIFECYCLE_STATUSES,
  };
}

export function buildWorkflowPlan({
  bundleRoot,
  sampleName = "session-summary",
  targetDeviceId = "neko-device-target",
  stagedBundleId,
  bundleType = "session",
  requestIds = {},
} = {}) {
  const snapshot = bundleRoot
    ? readBundleSnapshot(bundleRoot)
    : exportSampleBundle({ sampleName });
  const bundleId = stagedBundleId ?? snapshot.manifest.bundle_id;
  const requestClient = GENERIC_ADAPTER_CLIENT;

  return {
    sample_name: sampleName,
    bundle: {
      bundle_root: snapshot.bundle_root,
      bundle_id: snapshot.manifest.bundle_id,
      bundle_type: bundleTypeLabel(snapshot.manifest.bundle_type),
      display_name: snapshot.manifest.display_name,
      source_app: snapshot.manifest.source_app,
      file_count: snapshot.manifest.summary.file_count,
      total_bytes: snapshot.manifest.summary.total_bytes,
      import_allowed: snapshot.import_allowed,
      staging_status: snapshot.staging_status,
    },
    requests: {
      authorization: buildAuthorizationRequest({
        requestId: requestIds.authorization ?? "adapter-auth-001",
        client: requestClient,
      }),
      send: buildSendRequest({
        requestId: requestIds.send ?? "adapter-send-001",
        client: requestClient,
        targetDeviceId,
        bundleRoot: snapshot.bundle_root,
        bundleType,
      }),
      detail: buildBundleDetailRequest({
        requestId: requestIds.detail ?? "adapter-detail-001",
        client: requestClient,
        stagedBundleId: bundleId,
      }),
      events: buildEventsPollRequest({
        requestId: requestIds.events ?? "adapter-events-001",
        client: requestClient,
      }),
      results: buildActionResultsRequest({
        requestId: requestIds.results ?? "adapter-results-001",
        client: requestClient,
      }),
      import: buildImportRequest({
        requestId: requestIds.import ?? "adapter-import-001",
        client: requestClient,
        stagedBundleId: bundleId,
        expectedBundleType: bundleType,
      }),
    },
    preview: buildBundleDetailPreview(snapshot, { status: "saved" }),
    receipt: buildReceipt(snapshot, { detailStatus: "imported" }),
    rollback: {
      kind: "bundle.rollback",
      bundle_root: snapshot.bundle_root,
      action: "delete_export_root",
    },
    status_words: {
      bundle_detail: BUNDLE_DETAIL_STATUSES,
      action_lifecycle: ACTION_LIFECYCLE_STATUSES,
      bridge_response: BRIDGE_RESPONSE_STATUSES,
    },
  };
}

export function writeReceipt(bundleRoot, receiptPath, options = {}) {
  const snapshot = readBundleSnapshot(bundleRoot);
  const receipt = buildReceipt(snapshot, options);
  if (receiptPath) {
    writeJsonFile(path.resolve(receiptPath), receipt);
  }
  return receipt;
}

export function rollbackExportRoot(bundleRoot) {
  const resolved = path.resolve(bundleRoot);
  rmSync(resolved, { recursive: true, force: true });
  return {
    status: "rolled_back",
    bundle_root: resolved,
  };
}

function readBundleFile(root, file) {
  assertSafeBundlePath(file.path);
  const filePath = path.join(root, file.path);
  const bytes = readFileSync(filePath);
  const sha256 = sha256Hex(bytes);
  assert.equal(bytes.length, file.size);
  assert.equal(sha256, file.sha256);
  return {
    path: file.path,
    size: file.size,
    sha256,
    role: file.role,
  };
}

function bundleTypeLabel(bundleType) {
  return String(bundleType);
}

function assertBundleRoot(bundleRoot) {
  assertNonEmpty("bundleRoot", bundleRoot);
  if (!existsSync(bundleRoot)) {
    throw new Error(`bundle root does not exist: ${bundleRoot}`);
  }
}

function assertNonEmpty(name, value) {
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(`${name} must be a non-empty string`);
  }
}

function assertStatus(status, allowed, label) {
  if (!allowed.includes(status)) {
    throw new Error(`${label} must be one of: ${allowed.join(", ")}`);
  }
}

function assertSafeBundlePath(relativePath) {
  assertNonEmpty("bundle file path", relativePath);
  const normalized = relativePath.replaceAll("\\", "/");
  if (
    normalized.startsWith("/") ||
    /^[A-Za-z]:/.test(normalized) ||
    normalized.includes("\0") ||
    normalized.split("/").some((segment) => segment === ".." || segment === ".")
  ) {
    throw new Error(`unsafe bundle file path: ${relativePath}`);
  }
}

function readJson(filePath) {
  return JSON.parse(readFileSync(filePath, "utf8"));
}

function writeJsonFile(filePath, value) {
  writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function sha256Hex(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function parseArgs(argv) {
  const result = { _: [] };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (!token.startsWith("--")) {
      result._.push(token);
      continue;
    }
    const key = token.slice(2).replaceAll("-", "_");
    const next = argv[index + 1];
    if (next === undefined || next.startsWith("--")) {
      result[key] = true;
      continue;
    }
    result[key] = next;
    index += 1;
  }
  return result;
}

function printJson(value) {
  process.stdout.write(`${JSON.stringify(value, null, 2)}\n`);
}

function usage() {
  process.stderr.write(
    [
      "Usage:",
      "  node docs/examples/generic-adapter/generic-adapter.mjs export --out /tmp/generic-adapter-bundle",
      "  node docs/examples/generic-adapter/generic-adapter.mjs plan --bundle /tmp/generic-adapter-bundle",
      "  node docs/examples/generic-adapter/generic-adapter.mjs receipt --bundle /tmp/generic-adapter-bundle --out /tmp/receipt.json",
      "  node docs/examples/generic-adapter/generic-adapter.mjs rollback --bundle /tmp/generic-adapter-bundle",
    ].join("\n"),
  );
}

function main(argv) {
  const [command, ...rest] = argv;
  if (!command || command === "help" || command === "--help" || command === "-h") {
    usage();
    return 0;
  }

  const args = parseArgs(rest);
  if (command === "export") {
    const outputRoot = args.out
      ? path.resolve(String(args.out))
      : mkdtempSync(path.join(os.tmpdir(), "generic-adapter-"));
    const snapshot = exportSampleBundle({
      sampleName: String(args.sample ?? "session-summary"),
      outputRoot,
    });
    printJson(snapshot);
    return 0;
  }

  if (command === "plan") {
    const bundleRoot = args.bundle ?? args.out;
    const plan = buildWorkflowPlan({
      bundleRoot: bundleRoot ? path.resolve(String(bundleRoot)) : undefined,
      sampleName: String(args.sample ?? "session-summary"),
      targetDeviceId: String(args.target_device_id ?? "neko-device-target"),
      stagedBundleId: args.staged_bundle_id ? String(args.staged_bundle_id) : undefined,
      bundleType: String(args.bundle_type ?? "session"),
    });
    printJson(plan);
    return 0;
  }

  if (command === "receipt") {
    const bundleRoot = args.bundle ?? args.out;
    if (!bundleRoot) {
      throw new Error("receipt requires --bundle");
    }
    const receipt = writeReceipt(path.resolve(String(bundleRoot)), args.receipt_out, {
      detailStatus: String(args.detail_status ?? "imported"),
      actionLifecycleStatus: String(args.action_status ?? "succeeded"),
      eventLifecycleStatus: String(args.event_status ?? "succeeded"),
    });
    printJson(receipt);
    return 0;
  }

  if (command === "rollback") {
    const bundleRoot = args.bundle ?? args.out;
    if (!bundleRoot) {
      throw new Error("rollback requires --bundle");
    }
    printJson(rollbackExportRoot(path.resolve(String(bundleRoot))));
    return 0;
  }

  throw new Error(`unknown command: ${command}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = main(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}

