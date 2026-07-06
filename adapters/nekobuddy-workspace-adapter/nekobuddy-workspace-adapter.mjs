#!/usr/bin/env node
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync
} from "node:fs";
import { dirname, extname, join, relative, resolve, sep } from "node:path";

const BUNDLE_SCHEMA = "nekolink.bundle.v1";
const ADAPTER_DESCRIPTOR_SCHEMA = "nekolink.adapter.v1";
const APP_MANIFEST_SCHEMA = "nekolink.adapter.app_manifest.v1";
const RECEIPT_SCHEMA = "nekobuddy.workspace.adapter.import_receipt.v1";
const IMPORT_PLAN_SCHEMA = "nekobuddy.workspace.adapter.import_plan.v1";
const CHECKSUM_ALGORITHM = "sha256";
const BUNDLE_TYPE = "workspace";
const PERMISSION_SCOPE = "workspace.import";
const WRITE_TARGET = "nekobuddy.workspace";
const CLIENT = {
  client_id: "nekobuddy.workspace.adapter",
  display_name: "NekoBuddy Workspace Adapter",
  app_kind: "nekobuddy"
};
const BRIDGE_SCOPES = [
  "bundle.read",
  "bundle.send",
  "bundle.import.request",
  "transfer.status.read"
];
const CONFLICT_STRATEGIES = new Set(["reject", "rename", "skip_conflicts"]);
const IMPORT_PLAN_STATES = ["would_import", "would_conflict", "would_skip", "cannot_import"];
const SECRET_KEY_PARTS = [
  "token",
  "cookie",
  "secret",
  "private_key",
  "privatekey",
  "api_key",
  "apikey",
  "password",
  "credential",
  "ssh_key",
  "access_key",
  "refresh_token"
];
const LOCAL_PATH_KEY_PARTS = ["path", "dir", "directory", "root", "home", "cache", "keychain"];

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});

async function main() {
  const [command, ...args] = process.argv.slice(2);
  const flags = parseFlags(args);
  if (command === "descriptor") {
    printJson(buildDescriptor(flags));
    return;
  }
  if (command === "app-manifest") {
    printJson(buildAppManifest(flags));
    return;
  }
  if (command === "export") {
    printJson(exportWorkspaceBundle(flags));
    return;
  }
  if (command === "import-dry-run") {
    printJson(dryRunWorkspaceImport(flags));
    return;
  }
  if (command === "import-confirm") {
    printJson(confirmWorkspaceImport(flags));
    return;
  }
  if (command === "rollback") {
    printJson(rollbackWorkspaceImport(flags));
    return;
  }
  if (command === "request") {
    const [kind, ...rest] = args;
    printJson(buildBridgeRequest(kind, parseFlags(rest)));
    return;
  }
  if (command === "workflow") {
    printJson(buildWorkflow(flags));
    return;
  }
  usage();
  process.exit(command ? 1 : 0);
}

function buildDescriptor(flags) {
  const descriptor = {
    schema: ADAPTER_DESCRIPTOR_SCHEMA,
    adapter_id: CLIENT.client_id,
    display_name: CLIENT.display_name,
    app_kind: CLIENT.app_kind,
    client: CLIENT,
    bridge: {
      requested_scopes: BRIDGE_SCOPES,
      default_ttl_seconds: Number(flags["ttl-seconds"] ?? 3600)
    },
    runtime: {
      invocation: "argv",
      working_directory: "adapter_root",
      actions: [
        {
          action: "export_bundle",
          command: "nekobuddy-workspace-adapter",
          args: [
            "export",
            "--source",
            "{workspace_dir}",
            "--output",
            "{bundle_output}",
            "--bundle-id",
            "{bundle_id}",
            "--name",
            "{display_name}"
          ]
        },
        {
          action: "import_bundle",
          command: "nekobuddy-workspace-adapter",
          args: [
            "import-confirm",
            "--bundle-root",
            "{bundle_root}",
            "--target-root",
            "{target_root}",
            "--conflict-strategy",
            "{conflict_strategy}"
          ]
        },
        {
          action: "rollback_import",
          command: "nekobuddy-workspace-adapter",
          args: ["rollback", "--receipt", "{adapter_receipt}"]
        }
      ]
    },
    transactions: {
      dry_run_required: true,
      receipt_required: true,
      rollback_supported: true,
      rollback_requires_receipt: true,
      conflict_resolution_required: true,
      migration_policy: flags["migration-policy"] ?? "manual_only"
    },
    bundle_types: [
      {
        bundle_type: BUNDLE_TYPE,
        can_export: true,
        can_import: true,
        permission_scope: PERMISSION_SCOPE,
        write_target: WRITE_TARGET,
        sensitive: true,
        requires_trusted_device: true,
        conflict_strategies: Array.from(CONFLICT_STRATEGIES)
      }
    ],
    security: {
      rejects_contains_secrets: true,
      strips_local_paths: true,
      strips_tokens_cookies_and_private_keys: true,
      requires_authenticated_encrypted_session_for_sensitive_bundles: true,
      refuses_untrusted_sensitive_send: true
    }
  };
  validateNoLocalPaths(descriptor, "descriptor");
  return descriptor;
}

function buildAppManifest(flags) {
  const manifest = {
    schema: APP_MANIFEST_SCHEMA,
    app_id: flags["app-id"] ?? "nekobuddy.app",
    display_name: flags.name ?? "NekoBuddy",
    app_kind: CLIENT.app_kind,
    adapter_id: CLIENT.client_id,
    resources: [
      {
        resource_id: flags["resource-id"] ?? "workspace.default",
        bundle_type: BUNDLE_TYPE,
        display_name: flags["resource-name"] ?? "Workspace",
        direction: "both",
        logical_source: "nekobuddy.workspace.selected",
        logical_target: WRITE_TARGET,
        permission_scope: PERMISSION_SCOPE,
        export_action: "export_bundle",
        import_action: "import_bundle",
        rollback_action: "rollback_import",
        sensitive: true,
        requires_trusted_device: true,
        conflict_strategies: Array.from(CONFLICT_STRATEGIES),
        migration_policy: flags["migration-policy"] ?? "manual_only"
      }
    ],
    safety: {
      never_include: [
        "provider tokens",
        "cookies",
        "private keys",
        "machine local absolute paths",
        "keychain or credential-manager references"
      ],
      require_user_selected_source: true,
      require_dry_run_before_import: true,
      require_receipt_for_import: true,
      require_authenticated_encrypted_session_for_sensitive_bundles: true
    }
  };
  validateNoLocalPaths(manifest, "app manifest");
  return manifest;
}

function exportWorkspaceBundle(flags) {
  const source = requireFlag(flags, "source");
  const output = requireFlag(flags, "output");
  const bundleId = requireFlag(flags, "bundle-id");
  const displayName = requireFlag(flags, "name");
  const containsSecrets = flags["contains-secrets"] === "true";
  assertSafeBundleId(bundleId);
  if (!existsSync(source) || !statSync(source).isDirectory()) {
    throw new Error(`--source must be a workspace directory: ${source}`);
  }

  const bundleRoot = join(output, bundleId);
  rmSync(bundleRoot, { recursive: true, force: true });
  mkdirSync(join(bundleRoot, "files"), { recursive: true });

  const redactedFields = new Set();
  const payloadFiles = [];
  for (const sourcePath of listFiles(source)) {
    const sourceRelative = normalizePath(relative(source, sourcePath));
    const destinationRelative = `files/${sourceRelative}`;
    assertSafeBundlePath(destinationRelative);
    const destination = join(bundleRoot, destinationRelative);
    mkdirSync(dirname(destination), { recursive: true });
    copySanitizedWorkspaceFile(sourcePath, destination, redactedFields);
    const bytes = readFileSync(destination);
    payloadFiles.push({
      path: destinationRelative,
      size: bytes.byteLength,
      sha256: sha256(bytes),
      role: workspaceFileRole(sourceRelative)
    });
  }
  if (payloadFiles.length === 0) {
    throw new Error("--source must contain at least one workspace file");
  }
  payloadFiles.sort((left, right) => left.path.localeCompare(right.path));

  const manifest = {
    schema: BUNDLE_SCHEMA,
    bundle_id: bundleId,
    bundle_type: BUNDLE_TYPE,
    display_name: displayName,
    source_app: "NekoBuddy",
    created_at: new Date().toISOString(),
    sender: {
      device_id: "nekobuddy-workspace-adapter",
      device_name: CLIENT.display_name,
      fingerprint: "sha256:adapter-local"
    },
    compatibility: {
      min_nekolink_version: 1,
      required_capabilities: ["bundle_transfer", "authenticated_encrypted_session"]
    },
    summary: {
      file_count: payloadFiles.length,
      total_bytes: payloadFiles.reduce((sum, file) => sum + file.size, 0)
    },
    files: payloadFiles
  };
  const checksums = {
    algorithm: CHECKSUM_ALGORITHM,
    files: Object.fromEntries(payloadFiles.map((file) => [file.path, file.sha256]))
  };
  const permissions = {
    requested_scopes: [PERMISSION_SCOPE],
    writes: [
      {
        target: WRITE_TARGET,
        mode: "manual_import",
        requires_user_confirmation: true
      }
    ],
    transport: {
      requires_trusted_device: true,
      requires_authenticated_encrypted_session: true
    },
    secrets: {
      contains_secrets: containsSecrets,
      redacted_fields: Array.from(redactedFields).sort()
    }
  };

  writeJson(join(bundleRoot, "bundle.json"), manifest);
  writeJson(join(bundleRoot, "checksums.json"), checksums);
  writeJson(join(bundleRoot, "permissions.json"), permissions);

  return {
    bundle_root: bundleRoot,
    bundle_id: bundleId,
    bundle_type: BUNDLE_TYPE,
    file_count: payloadFiles.length,
    total_bytes: manifest.summary.total_bytes,
    redacted_fields: permissions.secrets.redacted_fields,
    requires_trusted_device: true
  };
}

function dryRunWorkspaceImport(flags) {
  try {
    return planWorkspaceImport(flags, true);
  } catch (error) {
    const strategy = conflictStrategy(flags["conflict-strategy"] ?? "reject");
    const plan = {
      schema: IMPORT_PLAN_SCHEMA,
      state: "cannot_import",
      next_action: "cancel_import_or_fix_bundle",
      bundle_id: null,
      bundle_type: BUNDLE_TYPE,
      display_name: null,
      conflict_strategy: strategy,
      target_root: flags["target-root"] ?? null,
      target_path: null,
      file_count: 0,
      would_import_file_count: 0,
      would_skip_file_count: 0,
      conflict_count: 0,
      conflicts: [],
      would_import_paths: [],
      would_skip_paths: [],
      reason: error instanceof Error ? error.message : String(error)
    };
    return {
      bundle_id: null,
      bundle_type: BUNDLE_TYPE,
      display_name: null,
      target_root: flags["target-root"] ?? null,
      target_path: null,
      status: "cannot_import",
      dry_run: true,
      conflict_strategy: strategy,
      would_import_file_count: 0,
      would_skip_file_count: 0,
      conflict_count: 0,
      conflicts: [],
      receipt_path: null,
      reason: plan.reason,
      plan
    };
  }
}

function confirmWorkspaceImport(flags) {
  return planWorkspaceImport(flags, false);
}

function planWorkspaceImport(flags, dryRun) {
  const bundleRoot = requireFlag(flags, "bundle-root");
  const targetRoot = requireFlag(flags, "target-root");
  const strategy = conflictStrategy(flags["conflict-strategy"] ?? "reject");
  if (!existsSync(bundleRoot) || !statSync(bundleRoot).isDirectory()) {
    throw new Error(`--bundle-root must be a directory: ${bundleRoot}`);
  }
  const manifest = readJson(join(bundleRoot, "bundle.json"));
  const checksums = readJson(join(bundleRoot, "checksums.json"));
  const permissions = readJson(join(bundleRoot, "permissions.json"));
  validateImportableWorkspaceBundle(manifest, checksums, permissions);

  const target = workspaceTargetPath(targetRoot, manifest, strategy);
  const files = manifest.files.map((file) => {
    assertSafeBundlePath(file.path);
    const source = join(bundleRoot, file.path);
    const destination = join(target, file.path.replace(/^files\//, ""));
    const bytes = readFileSync(source);
    if (bytes.byteLength !== file.size) {
      throw new Error(`size mismatch: ${file.path}`);
    }
    if (sha256(bytes) !== file.sha256 || checksums.files[file.path] !== file.sha256) {
      throw new Error(`checksum mismatch: ${file.path}`);
    }
    return {
      manifest_path: file.path,
      destination,
      size: file.size,
      sha256: file.sha256,
      destination_exists: existsSync(destination)
    };
  });
  const conflicts = files.filter((file) => file.destination_exists);
  if (dryRun) {
    return importPlanResponse(manifest, targetRoot, target, files, conflicts, strategy);
  }
  const targetExists = existsSync(target);
  if ((targetExists || conflicts.length > 0) && strategy === "reject") {
    return {
      bundle_id: manifest.bundle_id,
      bundle_type: BUNDLE_TYPE,
      display_name: manifest.display_name,
      target_root: targetRoot,
      target_path: target,
      status: "conflict",
      conflict_strategy: strategy,
      imported_file_count: 0,
      skipped_file_count: 0,
      conflict_count: Math.max(conflicts.length, targetExists ? 1 : 0),
      conflicts: conflicts.map((file) => file.manifest_path),
      receipt_path: null
    };
  }

  mkdirSync(target, { recursive: true });
  const imported = [];
  const skipped = [];
  for (const file of files) {
    if (file.destination_exists && strategy === "skip_conflicts") {
      skipped.push(file.manifest_path);
      continue;
    }
    mkdirSync(dirname(file.destination), { recursive: true });
    copyFileSync(join(bundleRoot, file.manifest_path), file.destination);
    imported.push(file);
  }

  const receipt = {
    schema: RECEIPT_SCHEMA,
    bundle_id: manifest.bundle_id,
    bundle_type: BUNDLE_TYPE,
    display_name: manifest.display_name,
    source_app: manifest.source_app,
    target_path: target,
    conflict_strategy: strategy,
    imported_manifest_paths: imported.map((file) => file.manifest_path),
    imported_files: imported.map((file) => ({
      manifest_path: file.manifest_path,
      size: file.size,
      sha256: file.sha256
    })),
    skipped_manifest_paths: skipped,
    imported_at: new Date().toISOString()
  };
  const receiptPath = workspaceReceiptPath(target, manifest.bundle_id, strategy);
  writeJson(receiptPath, receipt);
  writeJson(join(target, ".nekobuddy-workspace-latest-import-receipt.json"), receipt);
  return {
    bundle_id: manifest.bundle_id,
    bundle_type: BUNDLE_TYPE,
    display_name: manifest.display_name,
    target_root: targetRoot,
    target_path: target,
    status: "imported",
    conflict_strategy: strategy,
    imported_file_count: imported.length,
    skipped_file_count: skipped.length,
    conflict_count: conflicts.length,
    conflicts: conflicts.map((file) => file.manifest_path),
    receipt_path: receiptPath
  };
}

function importPlanResponse(manifest, targetRoot, target, files, conflicts, strategy) {
  const skipped = strategy === "skip_conflicts"
    ? conflicts.map((file) => file.manifest_path)
    : [];
  const wouldImport = files
    .map((file) => file.manifest_path)
    .filter((manifestPath) => !skipped.includes(manifestPath));
  const conflictCount = Math.max(conflicts.length, existsSync(target) ? 1 : 0);
  const state = importPlanState(strategy, conflictCount, wouldImport.length, skipped.length);
  const plan = {
    schema: IMPORT_PLAN_SCHEMA,
    state,
    next_action: nextActionForImportPlan(state),
    bundle_id: manifest.bundle_id,
    bundle_type: BUNDLE_TYPE,
    display_name: manifest.display_name,
    conflict_strategy: strategy,
    target_root: targetRoot,
    target_path: target,
    file_count: files.length,
    would_import_file_count: wouldImport.length,
    would_skip_file_count: skipped.length,
    conflict_count: conflictCount,
    conflicts: conflicts.map((file) => file.manifest_path),
    would_import_paths: wouldImport,
    would_skip_paths: skipped
  };
  return {
    bundle_id: manifest.bundle_id,
    bundle_type: BUNDLE_TYPE,
    display_name: manifest.display_name,
    target_root: targetRoot,
    target_path: target,
    status: state,
    dry_run: true,
    conflict_strategy: strategy,
    would_import_file_count: wouldImport.length,
    would_skip_file_count: skipped.length,
    conflict_count: conflictCount,
    conflicts: plan.conflicts,
    receipt_path: null,
    plan
  };
}

function rollbackWorkspaceImport(flags) {
  const receiptPath = requireFlag(flags, "receipt");
  if (!existsSync(receiptPath) || !statSync(receiptPath).isFile()) {
    throw new Error(`--receipt must be an import receipt file: ${receiptPath}`);
  }
  const receipt = readJson(receiptPath);
  validateWorkspaceReceipt(receipt);
  const target = receipt.target_path;
  const resolvedTarget = resolve(target);
  if (!pathIsInside(resolve(receiptPath), resolvedTarget)) {
    throw new Error("adapter import receipt must live inside its target_path");
  }
  if (!existsSync(target) || !statSync(target).isDirectory()) {
    return rollbackBlocked(receipt, "target_missing", []);
  }

  const blocked = [];
  for (const file of receipt.imported_files) {
    const destination = join(target, file.manifest_path.replace(/^files\//, ""));
    if (!pathIsInside(resolve(destination), resolvedTarget)) {
      blocked.push(file.manifest_path);
      continue;
    }
    if (!existsSync(destination) || !statSync(destination).isFile()) {
      blocked.push(file.manifest_path);
      continue;
    }
    const bytes = readFileSync(destination);
    if (bytes.byteLength !== file.size || sha256(bytes) !== file.sha256) {
      blocked.push(file.manifest_path);
    }
  }
  if (blocked.length > 0) {
    return rollbackBlocked(receipt, "imported_file_missing_changed_or_not_file", blocked);
  }

  const removed = [];
  for (const file of receipt.imported_files) {
    const destination = join(target, file.manifest_path.replace(/^files\//, ""));
    rmSync(destination);
    removed.push(file.manifest_path);
  }
  writeJson(join(target, ".nekobuddy-workspace-rollback-receipt.json"), {
    ...receipt,
    rolled_back_at: new Date().toISOString(),
    removed_manifest_paths: removed
  });
  return {
    bundle_id: receipt.bundle_id,
    bundle_type: BUNDLE_TYPE,
    target_path: target,
    status: "rolled_back",
    reason: null,
    removed_file_count: removed.length,
    removed_manifest_paths: removed,
    skipped_manifest_paths: receipt.skipped_manifest_paths
  };
}

function rollbackBlocked(receipt, reason, blocked) {
  return {
    bundle_id: receipt.bundle_id,
    bundle_type: BUNDLE_TYPE,
    target_path: receipt.target_path,
    status: "blocked",
    reason,
    removed_file_count: 0,
    removed_manifest_paths: [],
    skipped_manifest_paths: receipt.skipped_manifest_paths,
    blocked_manifest_paths: blocked
  };
}

function buildBridgeRequest(kind, flags) {
  if (kind === "auth") {
    return {
      kind: "authorization.request",
      payload: {
        request_id: flags["request-id"] ?? "nekobuddy-workspace-auth-001",
        client: CLIENT,
        requested_scopes: BRIDGE_SCOPES,
        reason: "Send, inspect, import, and rollback a NekoBuddy workspace bundle",
        ttl_seconds: Number(flags["ttl-seconds"] ?? 3600)
      }
    };
  }
  if (kind === "send") {
    return {
      kind: "bundle.send",
      payload: {
        request_id: flags["request-id"] ?? "nekobuddy-workspace-send-001",
        client: CLIENT,
        target_device_id: requireFlag(flags, "target-device-id"),
        bundle_root: requireFlag(flags, "bundle-root"),
        bundle_type: BUNDLE_TYPE,
        require_trusted_device: true
      }
    };
  }
  if (kind === "events") {
    return {
      kind: "events.poll",
      payload: {
        request_id: flags["request-id"] ?? "nekobuddy-workspace-events-001",
        client: CLIENT,
        after_event_id: flags["after-event-id"] ?? null,
        action_request_id: flags["action-request-id"] ?? null,
        limit: Number(flags.limit ?? 20),
        timeout_ms: Number(flags["timeout-ms"] ?? 15000)
      }
    };
  }
  if (kind === "results") {
    return {
      kind: "actions.results",
      payload: {
        request_id: flags["request-id"] ?? "nekobuddy-workspace-results-001",
        client: CLIENT,
        action_request_id: requireFlag(flags, "action-request-id"),
        after_claimed_at_ms: flags["after-claimed-at-ms"] ? Number(flags["after-claimed-at-ms"]) : null,
        limit: Number(flags.limit ?? 20)
      }
    };
  }
  if (kind === "detail") {
    return {
      kind: "bundle.detail",
      payload: {
        request_id: flags["request-id"] ?? "nekobuddy-workspace-detail-001",
        client: CLIENT,
        staged_bundle_id: requireFlag(flags, "staged-bundle-id")
      }
    };
  }
  if (kind === "import") {
    return {
      kind: "bundle.import",
      payload: {
        request_id: flags["request-id"] ?? "nekobuddy-workspace-import-001",
        client: CLIENT,
        staged_bundle_id: requireFlag(flags, "staged-bundle-id"),
        expected_bundle_type: BUNDLE_TYPE,
        conflict_strategy: conflictStrategy(flags["conflict-strategy"] ?? "reject")
      }
    };
  }
  if (kind === "rollback") {
    return {
      kind: "bundle.rollback",
      payload: {
        request_id: flags["request-id"] ?? "nekobuddy-workspace-rollback-001",
        client: CLIENT,
        bundle_id: requireFlag(flags, "bundle-id")
      }
    };
  }
  throw new Error("request kind must be auth, send, events, results, detail, import, or rollback");
}

function buildWorkflow(flags) {
  const bundleId = requireFlag(flags, "bundle-id");
  const sendRequestId = flags["send-request-id"] ?? `${bundleId}-send`;
  const importRequestId = flags["import-request-id"] ?? `${bundleId}-import`;
  const rollbackRequestId = flags["rollback-request-id"] ?? `${bundleId}-rollback`;
  const bundleRoot = flags["bundle-root"] ?? join(requireFlag(flags, "output"), bundleId);
  const stagedBundleId = flags["staged-bundle-id"] ?? bundleId;
  return {
    schema: "nekobuddy.workspace.adapter.workflow.v1",
    adapter_id: CLIENT.client_id,
    resource_id: "workspace.default",
    bundle_id: bundleId,
    steps: [
      {
        id: "export_workspace",
        owner: "adapter",
        command: [
          "nekobuddy-workspace-adapter",
          "export",
          "--source",
          requireFlag(flags, "source"),
          "--output",
          requireFlag(flags, "output"),
          "--bundle-id",
          bundleId,
          "--name",
          requireFlag(flags, "name")
        ]
      },
      {
        id: "authorize_bridge",
        owner: "local_bridge",
        request: buildBridgeRequest("auth", { "request-id": `${bundleId}-auth` })
      },
      {
        id: "send_bundle",
        owner: "local_bridge",
        request: buildBridgeRequest("send", {
          "request-id": sendRequestId,
          "bundle-root": bundleRoot,
          "target-device-id": requireFlag(flags, "target-device-id")
        })
      },
      {
        id: "observe_send",
        owner: "local_bridge",
        request: buildBridgeRequest("events", {
          "request-id": `${bundleId}-send-events`,
          "action-request-id": sendRequestId
        })
      },
      {
        id: "send_action_result",
        owner: "local_bridge",
        request: buildBridgeRequest("results", {
          "request-id": `${bundleId}-send-results`,
          "action-request-id": sendRequestId
        })
      },
      {
        id: "inspect_received_bundle",
        owner: "local_bridge",
        request: buildBridgeRequest("detail", {
          "request-id": `${bundleId}-detail`,
          "staged-bundle-id": stagedBundleId
        })
      },
      {
        id: "import_dry_run",
        owner: "adapter",
        command: [
          "nekobuddy-workspace-adapter",
          "import-dry-run",
          "--bundle-root",
          bundleRoot,
          "--target-root",
          "{adapter_target_root}",
          "--conflict-strategy",
          "reject"
        ]
      },
      {
        id: "request_import",
        owner: "local_bridge",
        request: buildBridgeRequest("import", {
          "request-id": importRequestId,
          "staged-bundle-id": stagedBundleId,
          "conflict-strategy": "reject"
        })
      },
      {
        id: "import_action_result",
        owner: "local_bridge",
        request: buildBridgeRequest("results", {
          "request-id": `${bundleId}-import-results`,
          "action-request-id": importRequestId
        })
      },
      {
        id: "request_rollback",
        owner: "local_bridge",
        request: buildBridgeRequest("rollback", {
          "request-id": rollbackRequestId,
          "bundle-id": stagedBundleId
        })
      },
      {
        id: "rollback_action_result",
        owner: "local_bridge",
        request: buildBridgeRequest("results", {
          "request-id": `${bundleId}-rollback-results`,
          "action-request-id": rollbackRequestId
        })
      }
    ]
  };
}

function validateImportableWorkspaceBundle(manifest, checksums, permissions) {
  if (manifest.schema !== BUNDLE_SCHEMA) {
    throw new Error(`unsupported bundle schema: ${manifest.schema}`);
  }
  if (manifest.bundle_type !== BUNDLE_TYPE) {
    throw new Error(`bundle type mismatch: expected workspace, got ${manifest.bundle_type}`);
  }
  if (!Array.isArray(manifest.files) || manifest.files.length === 0) {
    throw new Error("workspace bundle must contain files");
  }
  if (!checksums || checksums.algorithm !== CHECKSUM_ALGORITHM || !checksums.files) {
    throw new Error("checksums.json must use sha256");
  }
  if (!permissions || !Array.isArray(permissions.writes)) {
    throw new Error("permissions.json with writes is required");
  }
  if (permissions?.secrets?.contains_secrets === true) {
    throw new Error("bundle contains secrets and must not be imported automatically");
  }
  if (permissions?.transport?.requires_trusted_device !== true) {
    throw new Error("workspace bundle must require a trusted device");
  }
  for (const file of manifest.files) {
    assertSafeBundlePath(file.path);
    if (!/^[a-f0-9]{64}$/.test(file.sha256)) {
      throw new Error(`invalid sha256 for ${file.path}`);
    }
    if (checksums.files[file.path] !== file.sha256) {
      throw new Error(`checksums.json mismatch for ${file.path}`);
    }
  }
}

function validateWorkspaceReceipt(receipt) {
  if (receipt.schema !== RECEIPT_SCHEMA) {
    throw new Error(`unsupported adapter import receipt schema: ${receipt.schema}`);
  }
  assertSafeBundleId(receipt.bundle_id);
  if (receipt.bundle_type !== BUNDLE_TYPE) {
    throw new Error(`receipt bundle_type must be ${BUNDLE_TYPE}`);
  }
  if (typeof receipt.target_path !== "string" || receipt.target_path.trim() === "") {
    throw new Error("receipt target_path is required");
  }
  if (!Array.isArray(receipt.imported_files) || !Array.isArray(receipt.skipped_manifest_paths)) {
    throw new Error("receipt must include imported_files and skipped_manifest_paths");
  }
  for (const file of receipt.imported_files) {
    assertSafeBundlePath(file.manifest_path);
    if (!Number.isFinite(file.size) || !/^[a-f0-9]{64}$/.test(file.sha256)) {
      throw new Error(`invalid receipt file entry: ${file.manifest_path}`);
    }
  }
}

function copySanitizedWorkspaceFile(source, destination, redactedFields) {
  if (extname(source).toLowerCase() === ".json") {
    const sanitized = sanitizeJson(readJson(source), redactedFields);
    writeJson(destination, sanitized);
    return;
  }
  copyFileSync(source, destination);
}

function sanitizeJson(value, redactedFields, path = []) {
  if (Array.isArray(value)) {
    return value.map((entry, index) => sanitizeJson(entry, redactedFields, [...path, String(index)]));
  }
  if (!value || typeof value !== "object") {
    if (typeof value === "string" && looksLikeLocalAbsolutePath(value)) {
      redactedFields.add(path.join("."));
      return "[redacted-local-path]";
    }
    return value;
  }
  const output = {};
  for (const [key, entry] of Object.entries(value)) {
    const currentPath = [...path, key];
    if (isSecretKey(key) || isLocalPathField(key, entry)) {
      redactedFields.add(currentPath.join("."));
      continue;
    }
    output[key] = sanitizeJson(entry, redactedFields, currentPath);
  }
  return output;
}

function isSecretKey(key) {
  const normalized = key.toLowerCase().replace(/[-\s]/g, "_");
  return SECRET_KEY_PARTS.some((part) => normalized.includes(part));
}

function isLocalPathField(key, value) {
  if (typeof value !== "string" || !looksLikeLocalAbsolutePath(value)) {
    return false;
  }
  const normalized = key.toLowerCase();
  return LOCAL_PATH_KEY_PARTS.some((part) => normalized.includes(part));
}

function looksLikeLocalAbsolutePath(value) {
  return value.startsWith("/") || value.startsWith("~") || /^[A-Za-z]:[\\/]/.test(value);
}

function validateNoLocalPaths(value, label, path = []) {
  if (Array.isArray(value)) {
    value.forEach((entry, index) => validateNoLocalPaths(entry, label, [...path, String(index)]));
    return;
  }
  if (!value || typeof value !== "object") {
    if (typeof value === "string" && looksLikeLocalAbsolutePath(value)) {
      throw new Error(`${label} must not contain local paths at ${path.join(".")}`);
    }
    return;
  }
  for (const [key, entry] of Object.entries(value)) {
    validateNoLocalPaths(entry, label, [...path, key]);
  }
}

function listFiles(root) {
  const files = [];
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    if ([".git", "node_modules", "target"].includes(entry.name)) {
      continue;
    }
    const fullPath = join(root, entry.name);
    if (entry.isDirectory()) {
      files.push(...listFiles(fullPath));
    } else if (entry.isFile()) {
      files.push(fullPath);
    }
  }
  return files;
}

function workspaceFileRole(relativePath) {
  if (relativePath === "workspace.json" || relativePath.endsWith("/workspace.json")) {
    return "workspace_metadata";
  }
  if (relativePath.endsWith(".md")) {
    return "workspace_note";
  }
  return "workspace_file";
}

function importPlanState(strategy, conflictCount, wouldImportCount, skippedCount) {
  if (conflictCount === 0) return "would_import";
  if (strategy === "reject") return "would_conflict";
  if (strategy === "skip_conflicts" && wouldImportCount === 0 && skippedCount > 0) return "would_skip";
  if (strategy === "skip_conflicts" || strategy === "rename") return "would_import";
  return "cannot_import";
}

function nextActionForImportPlan(state) {
  if (state === "would_import" || state === "would_skip") {
    return "confirm_import_then_run_import_confirm";
  }
  if (state === "would_conflict") {
    return "choose_rename_or_skip_conflicts_or_cancel";
  }
  return "cancel_import_or_fix_bundle";
}

function workspaceTargetPath(targetRoot, manifest, strategy) {
  const base = join(targetRoot, "workspaces", manifest.bundle_id);
  if (strategy !== "rename" || !existsSync(base)) {
    return base;
  }
  for (let index = 2; index < 1000; index += 1) {
    const candidate = `${base}-${index}`;
    if (!existsSync(candidate)) return candidate;
  }
  throw new Error(`could not choose a renamed target for ${manifest.bundle_id}`);
}

function workspaceReceiptPath(target, bundleId, strategy) {
  const prefix = `.nekobuddy-workspace-import-receipt-${bundleId}-${strategy}-${Date.now()}`;
  let candidate = join(target, `${prefix}.json`);
  for (let index = 2; existsSync(candidate); index += 1) {
    candidate = join(target, `${prefix}-${index}.json`);
  }
  return candidate;
}

function conflictStrategy(value) {
  if (CONFLICT_STRATEGIES.has(value)) return value;
  throw new Error("--conflict-strategy must be reject, rename, or skip_conflicts");
}

function assertSafeBundleId(bundleId) {
  if (!/^[A-Za-z0-9._-]+$/.test(bundleId)) {
    throw new Error(`unsafe bundle id: ${bundleId}`);
  }
}

function assertSafeBundlePath(path) {
  const normalized = normalizePath(path);
  if (
    normalized.startsWith("/") ||
    normalized.includes("..") ||
    !normalized.startsWith("files/") ||
    normalized.endsWith("/")
  ) {
    throw new Error(`unsafe bundle path: ${path}`);
  }
}

function pathIsInside(child, parent) {
  return child === parent || child.startsWith(`${parent}${sep}`);
}

function parseFlags(args) {
  const flags = {};
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (!arg.startsWith("--")) {
      continue;
    }
    const key = arg.slice(2);
    const next = args[index + 1];
    const value = next && !next.startsWith("--") ? next : "true";
    if (Object.prototype.hasOwnProperty.call(flags, key)) {
      flags[key] = Array.isArray(flags[key]) ? [...flags[key], value] : [flags[key], value];
    } else {
      flags[key] = value;
    }
    if (value !== "true") index += 1;
  }
  return flags;
}

function requireFlag(flags, name) {
  const value = flags[name];
  if (typeof value !== "string" || value.trim() === "") {
    throw new Error(`--${name} is required`);
  }
  return value;
}

function normalizePath(path) {
  return path.split(sep).join("/");
}

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function printJson(value) {
  process.stdout.write(`${JSON.stringify(value, null, 2)}\n`);
}

function usage() {
  process.stdout.write(`Usage:
  nekobuddy-workspace-adapter descriptor
  nekobuddy-workspace-adapter app-manifest
  nekobuddy-workspace-adapter export --source <dir> --output <dir> --bundle-id <id> --name <name>
  nekobuddy-workspace-adapter import-dry-run --bundle-root <dir> --target-root <dir>
  nekobuddy-workspace-adapter import-confirm --bundle-root <dir> --target-root <dir>
  nekobuddy-workspace-adapter rollback --receipt <path>
  nekobuddy-workspace-adapter request <auth|send|events|results|detail|import|rollback>
  nekobuddy-workspace-adapter workflow --source <dir> --output <dir> --bundle-id <id> --name <name> --target-device-id <id>
`);
}
