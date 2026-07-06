#!/usr/bin/env node
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync
} from "node:fs";
import { request as httpRequest } from "node:http";
import { homedir } from "node:os";
import { dirname, extname, join, relative, resolve, sep } from "node:path";

const BUNDLE_SCHEMA = "nekolink.bundle.v1";
const ADAPTER_DESCRIPTOR_SCHEMA = "nekolink.adapter.v1";
const APP_MANIFEST_SCHEMA = "nekolink.adapter.app_manifest.v1";
const RECEIPT_SCHEMA = "nekobuddy.workspace.adapter.import_receipt.v1";
const IMPORT_PLAN_SCHEMA = "nekobuddy.workspace.adapter.import_plan.v1";
const IMPORT_TRANSACTION_SCHEMA = "nekobuddy.workspace.adapter.import_transaction.v1";
const CHECKSUM_ALGORITHM = "sha256";
const BUNDLE_TYPE = "workspace";
const PERMISSION_SCOPE = "workspace.import";
const WRITE_TARGET = "nekobuddy.workspace";
const WORKSPACE_SCHEMA_ID = "nekobuddy.workspace";
const WORKSPACE_SCHEMA_VERSION = 1;
const SUPPORTED_WORKSPACE_SCHEMA_VERSIONS = [WORKSPACE_SCHEMA_VERSION];
const RECEIPT_VERSION = 1;
const IMPORT_TRANSACTION_VERSION = 1;
const DEFAULT_BRIDGE_HOST = "127.0.0.1";
const DEFAULT_BRIDGE_PORT = 45921;
const DEFAULT_BRIDGE_REQUEST_PATH = "/bridge/request";
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
const MIGRATION_POLICIES = new Set(["manual_only", "adapter_managed"]);
const CONFLICT_STRATEGIES = new Set(["reject", "rename", "skip_conflicts"]);
const IMPORT_PLAN_STATES = ["would_import", "would_conflict", "would_skip", "cannot_import"];
const IMPORT_TRANSACTION_STATES = ["prepared", "copied", "committed", "failed", "recovered"];
const ROLLBACK_BLOCKING_REASONS = [
  "target_missing",
  "receipt_already_rolled_back",
  "imported_path_unsafe",
  "imported_file_missing",
  "imported_file_changed"
];
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
  if (command === "recover-import") {
    printJson(recoverWorkspaceImport(flags));
    return;
  }
  if (command === "contract") {
    printJson(buildTransactionContract());
    return;
  }
  if (command === "request") {
    const [kind, ...rest] = args;
    printJson(buildBridgeRequest(kind, parseFlags(rest)));
    return;
  }
  if (command === "discover-bridge") {
    printJson(discoverBridgeEndpoint(flags));
    return;
  }
  if (command === "post") {
    const [kind, ...rest] = args;
    printJson(await postBridgeRequest(kind, parseFlags(rest)));
    return;
  }
  if (command === "send-workspace") {
    printJson(await sendWorkspace(flags));
    return;
  }
  if (command === "receive-workspace") {
    printJson(await receiveWorkspace(flags));
    return;
  }
  if (command === "rollback-workspace") {
    printJson(await rollbackWorkspace(flags));
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
      receipt_schema: RECEIPT_SCHEMA,
      receipt_version: RECEIPT_VERSION,
      rollback_supported: true,
      rollback_requires_receipt: true,
      conflict_resolution_required: true,
      migration_policy: migrationPolicy(flags["migration-policy"] ?? "manual_only"),
      transaction_schema: IMPORT_TRANSACTION_SCHEMA,
      transaction_version: IMPORT_TRANSACTION_VERSION,
      failure_recovery_action: "recover-import",
      rollback_blocking_reasons: ROLLBACK_BLOCKING_REASONS
    },
    workspace_schema: workspaceSchemaDescriptor(flags),
    bundle_types: [
      {
        bundle_type: BUNDLE_TYPE,
        can_export: true,
        can_import: true,
        permission_scope: PERMISSION_SCOPE,
        write_target: WRITE_TARGET,
        sensitive: true,
        requires_trusted_device: true,
        conflict_strategies: Array.from(CONFLICT_STRATEGIES),
        workspace_schema_id: WORKSPACE_SCHEMA_ID,
        supported_workspace_schema_versions: SUPPORTED_WORKSPACE_SCHEMA_VERSIONS
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
        migration_policy: migrationPolicy(flags["migration-policy"] ?? "manual_only"),
        workspace_schema_id: WORKSPACE_SCHEMA_ID,
        workspace_schema_version: WORKSPACE_SCHEMA_VERSION,
        supported_workspace_schema_versions: SUPPORTED_WORKSPACE_SCHEMA_VERSIONS
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

function buildTransactionContract() {
  return {
    schema: "nekobuddy.workspace.adapter.transaction_contract.v1",
    adapter_id: CLIENT.client_id,
    workspace_schema: {
      schema_id: WORKSPACE_SCHEMA_ID,
      current_version: WORKSPACE_SCHEMA_VERSION,
      supported_versions: SUPPORTED_WORKSPACE_SCHEMA_VERSIONS,
      default_migration_policy: "manual_only",
      supported_migration_policies: Array.from(MIGRATION_POLICIES),
      unsupported_version_state: "cannot_import"
    },
    import_plan: {
      schema: IMPORT_PLAN_SCHEMA,
      stable_states: IMPORT_PLAN_STATES,
      migration_fields_required: true
    },
    import_transaction: {
      schema: IMPORT_TRANSACTION_SCHEMA,
      version: IMPORT_TRANSACTION_VERSION,
      states: IMPORT_TRANSACTION_STATES,
      failure_recovery_action: "recover-import"
    },
    receipt: {
      schema: RECEIPT_SCHEMA,
      version: RECEIPT_VERSION,
      records_workspace_schema_version: true,
      records_migration_status: true,
      records_transaction_id: true
    },
    rollback: {
      blocking_reasons: ROLLBACK_BLOCKING_REASONS,
      changed_files_block_rollback: true,
      missing_files_block_rollback: true
    }
  };
}

function exportWorkspaceBundle(flags) {
  const source = requireFlag(flags, "source");
  const output = requireFlag(flags, "output");
  const bundleId = requireFlag(flags, "bundle-id");
  const displayName = requireFlag(flags, "name");
  const containsSecrets = flags["contains-secrets"] === "true";
  const workspaceSchema = workspaceSchemaDescriptor(flags);
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
      required_capabilities: ["bundle_transfer", "authenticated_encrypted_session"],
      workspace_schema_id: workspaceSchema.schema_id,
      workspace_schema_version: workspaceSchema.current_version
    },
    application_schema: {
      schema_id: workspaceSchema.schema_id,
      version: workspaceSchema.current_version,
      supported_versions: workspaceSchema.supported_versions,
      migration_policy: workspaceSchema.migration_policy
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
    workspace_schema_id: workspaceSchema.schema_id,
    workspace_schema_version: workspaceSchema.current_version,
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
  const migration = workspaceMigrationPlan(manifest, flags);

  const target = workspaceTargetPath(targetRoot, manifest, strategy);
  if (!migration.can_import) {
    return importSchemaBlockedResponse(manifest, targetRoot, target, strategy, migration, dryRun);
  }
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
    return importPlanResponse(manifest, targetRoot, target, files, conflicts, strategy, migration);
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
      workspace_schema_id: migration.schema_id,
      workspace_schema_version: migration.source_version,
      migration,
      imported_file_count: 0,
      skipped_file_count: 0,
      conflict_count: Math.max(conflicts.length, targetExists ? 1 : 0),
      conflicts: conflicts.map((file) => file.manifest_path),
      receipt_path: null
    };
  }

  const transaction = beginImportTransaction(targetRoot, target, manifest, strategy, migration, files);
  let imported = [];
  let skipped = [];
  try {
    ({ imported, skipped } = commitWorkspaceImportTransaction(bundleRoot, target, files, strategy, transaction, flags));
  } catch (error) {
    return failedImportResponse(manifest, targetRoot, target, strategy, migration, transaction, error);
  }

  const receipt = {
    schema: RECEIPT_SCHEMA,
    receipt_version: RECEIPT_VERSION,
    bundle_id: manifest.bundle_id,
    bundle_type: BUNDLE_TYPE,
    display_name: manifest.display_name,
    source_app: manifest.source_app,
    workspace_schema_id: migration.schema_id,
    workspace_schema_version: migration.target_version,
    source_workspace_schema_version: migration.source_version,
    migration_policy: migration.migration_policy,
    migration_status: migration.status,
    transaction_id: transaction.transaction_id,
    transaction_schema: IMPORT_TRANSACTION_SCHEMA,
    transaction_version: IMPORT_TRANSACTION_VERSION,
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
  updateImportTransaction(transaction, {
    state: "committed",
    receipt_path: receiptPath,
    committed_at: new Date().toISOString()
  });
  return {
    bundle_id: manifest.bundle_id,
    bundle_type: BUNDLE_TYPE,
    display_name: manifest.display_name,
    workspace_schema_id: migration.schema_id,
    workspace_schema_version: migration.target_version,
    migration,
    transaction_id: transaction.transaction_id,
    transaction_path: transaction.transaction_path,
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

function importPlanResponse(manifest, targetRoot, target, files, conflicts, strategy, migration) {
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
    workspace_schema_id: migration.schema_id,
    workspace_schema_version: migration.source_version,
    target_workspace_schema_version: migration.target_version,
    migration,
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
    workspace_schema_id: migration.schema_id,
    workspace_schema_version: migration.source_version,
    target_workspace_schema_version: migration.target_version,
    migration,
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

function importSchemaBlockedResponse(manifest, targetRoot, target, strategy, migration, dryRun) {
  const plan = {
    schema: IMPORT_PLAN_SCHEMA,
    state: "cannot_import",
    next_action: "cancel_import_or_run_manual_migration",
    bundle_id: manifest.bundle_id,
    bundle_type: BUNDLE_TYPE,
    display_name: manifest.display_name,
    workspace_schema_id: migration.schema_id,
    workspace_schema_version: migration.source_version,
    target_workspace_schema_version: migration.target_version,
    migration,
    conflict_strategy: strategy,
    target_root: targetRoot,
    target_path: target,
    file_count: Array.isArray(manifest.files) ? manifest.files.length : 0,
    would_import_file_count: 0,
    would_skip_file_count: 0,
    conflict_count: 0,
    conflicts: [],
    would_import_paths: [],
    would_skip_paths: [],
    reason: migration.blocking_reason
  };
  return {
    bundle_id: manifest.bundle_id,
    bundle_type: BUNDLE_TYPE,
    display_name: manifest.display_name,
    workspace_schema_id: migration.schema_id,
    workspace_schema_version: migration.source_version,
    target_workspace_schema_version: migration.target_version,
    migration,
    target_root: targetRoot,
    target_path: target,
    status: "cannot_import",
    dry_run: dryRun,
    conflict_strategy: strategy,
    would_import_file_count: 0,
    would_skip_file_count: 0,
    conflict_count: 0,
    conflicts: [],
    receipt_path: null,
    reason: migration.blocking_reason,
    plan
  };
}

function failedImportResponse(manifest, targetRoot, target, strategy, migration, transaction, error) {
  const reason = error instanceof Error ? error.message : String(error);
  updateImportTransaction(transaction, {
    state: "failed",
    failed_at: new Date().toISOString(),
    failure_reason: reason,
    recovery_action: "recover-import"
  });
  return {
    bundle_id: manifest.bundle_id,
    bundle_type: BUNDLE_TYPE,
    display_name: manifest.display_name,
    workspace_schema_id: migration.schema_id,
    workspace_schema_version: migration.source_version,
    target_workspace_schema_version: migration.target_version,
    migration,
    target_root: targetRoot,
    target_path: target,
    status: "failed",
    reason,
    recovery_action: "recover-import",
    transaction_id: transaction.transaction_id,
    transaction_path: transaction.transaction_path,
    temp_path: transaction.temp_path,
    conflict_strategy: strategy,
    imported_file_count: 0,
    skipped_file_count: 0,
    conflict_count: 0,
    conflicts: [],
    receipt_path: null
  };
}

function beginImportTransaction(targetRoot, target, manifest, strategy, migration, files) {
  const transactionId = newTransactionId(manifest.bundle_id);
  const transaction = {
    schema: IMPORT_TRANSACTION_SCHEMA,
    transaction_version: IMPORT_TRANSACTION_VERSION,
    transaction_id: transactionId,
    bundle_id: manifest.bundle_id,
    bundle_type: BUNDLE_TYPE,
    workspace_schema_id: migration.schema_id,
    source_workspace_schema_version: migration.source_version,
    target_workspace_schema_version: migration.target_version,
    migration_policy: migration.migration_policy,
    migration_status: migration.status,
    conflict_strategy: strategy,
    state: "prepared",
    target_path: target,
    temp_path: join(workspaceImportTempRoot(targetRoot), transactionId),
    transaction_path: join(workspaceImportTransactionRoot(targetRoot), `${transactionId}.json`),
    manifest_paths: files.map((file) => file.manifest_path),
    files: files.map((file) => ({
      manifest_path: file.manifest_path,
      size: file.size,
      sha256: file.sha256
    })),
    copied_manifest_paths: [],
    committed_manifest_paths: [],
    skipped_manifest_paths: [],
    receipt_path: null,
    recovery_action: "recover-import",
    rollback_blocking_reasons: ROLLBACK_BLOCKING_REASONS,
    created_at: new Date().toISOString()
  };
  mkdirSync(dirname(transaction.transaction_path), { recursive: true });
  mkdirSync(dirname(transaction.temp_path), { recursive: true });
  writeJson(transaction.transaction_path, transaction);
  return transaction;
}

function commitWorkspaceImportTransaction(bundleRoot, target, files, strategy, transaction, flags) {
  const imported = [];
  const skipped = [];
  rmSync(transaction.temp_path, { recursive: true, force: true });
  mkdirSync(transaction.temp_path, { recursive: true });
  for (const file of files) {
    if (file.destination_exists && strategy === "skip_conflicts") {
      skipped.push(file.manifest_path);
      continue;
    }
    const relativePath = file.manifest_path.replace(/^files\//, "");
    const stagedDestination = join(transaction.temp_path, relativePath);
    mkdirSync(dirname(stagedDestination), { recursive: true });
    copyFileSync(join(bundleRoot, file.manifest_path), stagedDestination);
    imported.push(file);
  }
  updateImportTransaction(transaction, {
    state: "copied",
    copied_manifest_paths: imported.map((file) => file.manifest_path),
    skipped_manifest_paths: skipped,
    copied_at: new Date().toISOString()
  });
  if (flags["simulate-fail-after-copy"] === "true") {
    throw new Error("simulated_failure_after_copy");
  }
  if (strategy === "skip_conflicts" && existsSync(target)) {
    const committed = [];
    try {
      for (const file of imported) {
        const relativePath = file.manifest_path.replace(/^files\//, "");
        const destination = join(target, relativePath);
        mkdirSync(dirname(destination), { recursive: true });
        copyFileSync(join(transaction.temp_path, relativePath), destination);
        committed.push(file);
      }
      updateImportTransaction(transaction, {
        committed_manifest_paths: committed.map((file) => file.manifest_path)
      });
      rmSync(transaction.temp_path, { recursive: true, force: true });
    } catch (error) {
      recoverCommittedFiles(target, committed);
      throw error;
    }
  } else {
    renameSync(transaction.temp_path, target);
    updateImportTransaction(transaction, {
      committed_manifest_paths: imported.map((file) => file.manifest_path)
    });
  }
  return { imported, skipped };
}

function recoverCommittedFiles(target, committed) {
  for (const file of [...committed].reverse()) {
    const destination = join(target, file.manifest_path.replace(/^files\//, ""));
    if (!existsSync(destination) || !statSync(destination).isFile()) {
      continue;
    }
    const bytes = readFileSync(destination);
    if (bytes.byteLength === file.size && sha256(bytes) === file.sha256) {
      rmSync(destination);
    }
  }
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
  if (existsSync(join(target, ".nekobuddy-workspace-rollback-receipt.json"))) {
    return rollbackBlocked(receipt, "receipt_already_rolled_back", []);
  }

  const blocked = [];
  for (const file of receipt.imported_files) {
    const destination = join(target, file.manifest_path.replace(/^files\//, ""));
    if (!pathIsInside(resolve(destination), resolvedTarget)) {
      blocked.push({ manifest_path: file.manifest_path, reason: "imported_path_unsafe" });
      continue;
    }
    if (!existsSync(destination) || !statSync(destination).isFile()) {
      blocked.push({ manifest_path: file.manifest_path, reason: "imported_file_missing" });
      continue;
    }
    const bytes = readFileSync(destination);
    if (bytes.byteLength !== file.size || sha256(bytes) !== file.sha256) {
      blocked.push({ manifest_path: file.manifest_path, reason: "imported_file_changed" });
    }
  }
  if (blocked.length > 0) {
    return rollbackBlocked(receipt, blocked[0].reason, blocked.map((file) => file.manifest_path), blocked);
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
    workspace_schema_id: receipt.workspace_schema_id ?? WORKSPACE_SCHEMA_ID,
    workspace_schema_version: receipt.workspace_schema_version ?? WORKSPACE_SCHEMA_VERSION,
    receipt_version: receipt.receipt_version ?? RECEIPT_VERSION,
    transaction_id: receipt.transaction_id ?? null,
    target_path: target,
    status: "rolled_back",
    reason: null,
    rollback_blocking_reason: null,
    removed_file_count: removed.length,
    removed_manifest_paths: removed,
    skipped_manifest_paths: receipt.skipped_manifest_paths
  };
}

function rollbackBlocked(receipt, reason, blocked) {
  return {
    bundle_id: receipt.bundle_id,
    bundle_type: BUNDLE_TYPE,
    workspace_schema_id: receipt.workspace_schema_id ?? WORKSPACE_SCHEMA_ID,
    workspace_schema_version: receipt.workspace_schema_version ?? WORKSPACE_SCHEMA_VERSION,
    receipt_version: receipt.receipt_version ?? RECEIPT_VERSION,
    transaction_id: receipt.transaction_id ?? null,
    target_path: receipt.target_path,
    status: "blocked",
    reason,
    rollback_blocking_reason: reason,
    removed_file_count: 0,
    removed_manifest_paths: [],
    skipped_manifest_paths: receipt.skipped_manifest_paths,
    blocked_manifest_paths: blocked
  };
}

function recoverWorkspaceImport(flags) {
  const transactionPath = requireFlag(flags, "transaction");
  if (!existsSync(transactionPath) || !statSync(transactionPath).isFile()) {
    throw new Error(`--transaction must be an import transaction file: ${transactionPath}`);
  }
  const transaction = readJson(transactionPath);
  validateImportTransaction(transaction);
  const target = transaction.target_path;
  const temp = transaction.temp_path;
  const removedManifestPaths = [];
  if (transaction.state === "committed") {
    return {
      schema: IMPORT_TRANSACTION_SCHEMA,
      transaction_id: transaction.transaction_id,
      bundle_id: transaction.bundle_id,
      status: "not_recovered",
      reason: "transaction_already_committed",
      removed_temp: false,
      removed_file_count: 0,
      removed_manifest_paths: []
    };
  }
  if (Array.isArray(transaction.committed_manifest_paths) && existsSync(target)) {
    for (const manifestPath of [...transaction.committed_manifest_paths].reverse()) {
      assertSafeBundlePath(manifestPath);
      const transactionFile = transaction.files.find((file) => file.manifest_path === manifestPath);
      if (!transactionFile) {
        continue;
      }
      const destination = join(target, manifestPath.replace(/^files\//, ""));
      if (existsSync(destination) && statSync(destination).isFile()) {
        const bytes = readFileSync(destination);
        if (bytes.byteLength === transactionFile.size && sha256(bytes) === transactionFile.sha256) {
          rmSync(destination);
          removedManifestPaths.push(manifestPath);
        }
      }
    }
  }
  const removedTemp = existsSync(temp);
  rmSync(temp, { recursive: true, force: true });
  updateImportTransaction(transaction, {
    state: "recovered",
    recovered_at: new Date().toISOString(),
    removed_manifest_paths: removedManifestPaths
  });
  return {
    schema: IMPORT_TRANSACTION_SCHEMA,
    transaction_id: transaction.transaction_id,
    bundle_id: transaction.bundle_id,
    status: "recovered",
    reason: null,
    removed_temp: removedTemp,
    removed_file_count: removedManifestPaths.length,
    removed_manifest_paths: removedManifestPaths
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

async function postBridgeRequest(kind, flags) {
  const request = buildBridgeRequest(kind, flags);
  const endpoint = discoverBridgeEndpoint(flags);
  const response = await postJson(endpoint, request);
  return {
    endpoint,
    request,
    response
  };
}

async function sendWorkspace(flags) {
  const bundle = exportWorkspaceBundle(flags);
  const bundleId = bundle.bundle_id;
  const sendRequestId = flags["send-request-id"] ?? `${bundleId}-send`;
  const auth = await postBridgeRequest("auth", {
    ...flags,
    "request-id": flags["auth-request-id"] ?? `${bundleId}-auth`
  });
  const send = await postBridgeRequest("send", {
    ...flags,
    "request-id": sendRequestId,
    "bundle-root": bundle.bundle_root,
    "target-device-id": requireFlag(flags, "target-device-id")
  });
  const events = await postBridgeRequest("events", {
    ...flags,
    "request-id": flags["events-request-id"] ?? `${bundleId}-send-events`,
    "action-request-id": sendRequestId
  });
  const results = await postBridgeRequest("results", {
    ...flags,
    "request-id": flags["results-request-id"] ?? `${bundleId}-send-results`,
    "action-request-id": sendRequestId
  });
  return {
    schema: "nekobuddy.workspace.adapter.send_result.v1",
    status: "send_requested",
    bundle,
    request_ids: {
      auth: auth.request.payload.request_id,
      send: sendRequestId,
      events: events.request.payload.request_id,
      results: results.request.payload.request_id
    },
    authorization: reconcileAuthorization(auth.request, auth.response),
    send_action: reconcileActionObservation(sendRequestId, send.response, events.response, results.response),
    authorization_response: auth.response,
    send_response: send.response,
    events_response: events.response,
    results_response: results.response
  };
}

async function receiveWorkspace(flags) {
  const stagedBundleId = requireFlag(flags, "staged-bundle-id");
  const bundleRoot = requireFlag(flags, "bundle-root");
  const targetRoot = requireFlag(flags, "target-root");
  const conflict = conflictStrategy(flags["conflict-strategy"] ?? "reject");
  const importRequestId = flags["import-request-id"] ?? `${stagedBundleId}-import`;
  const detail = await postBridgeRequest("detail", {
    ...flags,
    "request-id": flags["detail-request-id"] ?? `${stagedBundleId}-detail`,
    "staged-bundle-id": stagedBundleId
  });
  const dryRun = dryRunWorkspaceImport({
    ...flags,
    "bundle-root": bundleRoot,
    "target-root": targetRoot,
    "conflict-strategy": conflict
  });
  if (!["would_import", "would_skip"].includes(dryRun.status)) {
    return {
      schema: "nekobuddy.workspace.adapter.receive_result.v1",
      status: "dry_run_blocked",
      request_ids: {
        detail: detail.request.payload.request_id,
        import: null,
        results: null
      },
      detail_response: detail.response,
      dry_run: dryRun,
      import_response: null,
      results_response: null,
      adapter_import: null,
      import_action: null
    };
  }
  const bridgeImport = await postBridgeRequest("import", {
    ...flags,
    "request-id": importRequestId,
    "staged-bundle-id": stagedBundleId,
    "conflict-strategy": conflict
  });
  const results = await postBridgeRequest("results", {
    ...flags,
    "request-id": flags["results-request-id"] ?? `${stagedBundleId}-import-results`,
    "action-request-id": importRequestId
  });
  const importAction = reconcileActionObservation(importRequestId, bridgeImport.response, null, results.response);
  if (!actionSucceeded(importAction)) {
    return {
      schema: "nekobuddy.workspace.adapter.receive_result.v1",
      status: "bridge_import_pending_or_failed",
      request_ids: {
        detail: detail.request.payload.request_id,
        import: importRequestId,
        results: results.request.payload.request_id
      },
      detail_response: detail.response,
      dry_run: dryRun,
      import_response: bridgeImport.response,
      results_response: results.response,
      adapter_import: null,
      import_action: importAction
    };
  }
  const imported = confirmWorkspaceImport({
    ...flags,
    "bundle-root": bundleRoot,
    "target-root": targetRoot,
    "conflict-strategy": conflict
  });
  return {
    schema: "nekobuddy.workspace.adapter.receive_result.v1",
    status: "import_requested",
    request_ids: {
      detail: detail.request.payload.request_id,
      import: importRequestId,
      results: results.request.payload.request_id
    },
    detail_response: detail.response,
    dry_run: dryRun,
    import_response: bridgeImport.response,
    results_response: results.response,
    adapter_import: imported,
    import_action: importAction
  };
}

async function rollbackWorkspace(flags) {
  const bundleId = requireFlag(flags, "bundle-id");
  const rollbackRequestId = flags["rollback-request-id"] ?? `${bundleId}-rollback`;
  const bridgeRollback = await postBridgeRequest("rollback", {
    ...flags,
    "request-id": rollbackRequestId,
    "bundle-id": bundleId
  });
  const results = await postBridgeRequest("results", {
    ...flags,
    "request-id": flags["results-request-id"] ?? `${bundleId}-rollback-results`,
    "action-request-id": rollbackRequestId
  });
  const rollbackAction = reconcileActionObservation(rollbackRequestId, bridgeRollback.response, null, results.response);
  const adapterRollback = flags.receipt && actionSucceeded(rollbackAction)
    ? rollbackWorkspaceImport({ receipt: flags.receipt })
    : null;
  return {
    schema: "nekobuddy.workspace.adapter.rollback_result.v1",
    status: "rollback_requested",
    request_ids: {
      rollback: rollbackRequestId,
      results: results.request.payload.request_id
    },
    rollback_response: bridgeRollback.response,
    results_response: results.response,
    adapter_rollback: adapterRollback,
    rollback_action: rollbackAction
  };
}

function actionSucceeded(action) {
  return action?.latest_lifecycle_status === "succeeded" || action?.latest_status === "completed";
}

function reconcileAuthorization(request, response) {
  const requestedScopes = request?.payload?.requested_scopes ?? [];
  const responseScopes = Array.isArray(response?.authorization_scopes)
    ? response.authorization_scopes
    : Array.isArray(response?.granted_scopes)
      ? response.granted_scopes
      : [];
  return {
    request_id: request?.payload?.request_id ?? null,
    requested_scopes: requestedScopes,
    response_scopes: responseScopes,
    missing_response_scopes: requestedScopes.filter((scope) => !responseScopes.includes(scope)),
    requires_user_confirmation: Boolean(response?.requires_user_confirmation),
    security_state: response?.security_state ?? null,
    status: response?.status ?? null
  };
}

function reconcileActionObservation(actionRequestId, mutationResponse, eventsResponse, resultsResponse) {
  const mutationResults = actionResultsForRequest(mutationResponse, actionRequestId);
  const actionEvents = actionEventsForRequest(eventsResponse, actionRequestId);
  const exactResults = actionResultsForRequest(resultsResponse, actionRequestId);
  const latest = exactResults.at(-1) ?? actionEvents.at(-1) ?? mutationResults.at(-1) ?? null;
  return {
    action_request_id: actionRequestId,
    mutation_status: mutationResponse?.status ?? null,
    event_count: actionEvents.length,
    result_count: exactResults.length,
    matched_result_count: mutationResults.length + actionEvents.length + exactResults.length,
    latest_lifecycle_status: latest?.lifecycle_status ?? latest?.status ?? null,
    latest_status: latest?.status ?? null,
    final: ["succeeded", "failed", "conflict", "cancelled"].includes(
      latest?.lifecycle_status ?? latest?.status ?? ""
    ),
    latest_result: latest
  };
}

function actionResultsForRequest(response, actionRequestId) {
  if (!Array.isArray(response?.action_results)) return [];
  return response.action_results.filter((result) => result.request_id === actionRequestId);
}

function actionEventsForRequest(response, actionRequestId) {
  if (!Array.isArray(response?.events)) return [];
  return response.events
    .filter((event) => event.kind === "action.updated" && event.payload)
    .map((event) => event.payload)
    .filter((payload) => payload.request_id === actionRequestId);
}

function discoverBridgeEndpoint(flags) {
  if (flags["bridge-url"]) {
    return endpointFromUrl(flags["bridge-url"], "bridge-url");
  }
  if (flags.port) {
    return endpointFromParts({
      host: flags.host ?? DEFAULT_BRIDGE_HOST,
      port: Number(flags.port),
      requestPath: flags["request-path"] ?? DEFAULT_BRIDGE_REQUEST_PATH,
      source: "flags"
    });
  }
  const statusFile = flags["status-file"] ??
    process.env.NEKODROP_LOCAL_BRIDGE_STATUS_FILE ??
    defaultBridgeStatusFile();
  if (statusFile && existsSync(statusFile)) {
    return endpointFromRuntimeStatus(readJson(statusFile), statusFile);
  }
  const configFile = flags["bridge-config"] ?? process.env.NEKODROP_LOCAL_BRIDGE_CONFIG_FILE;
  if (configFile && existsSync(configFile)) {
    return endpointFromRuntimeStatus(readJson(configFile), configFile);
  }
  if (process.env.NEKODROP_LOCAL_BRIDGE_PORT) {
    return endpointFromParts({
      host: process.env.NEKODROP_LOCAL_BRIDGE_HOST ?? DEFAULT_BRIDGE_HOST,
      port: Number(process.env.NEKODROP_LOCAL_BRIDGE_PORT),
      requestPath: process.env.NEKODROP_LOCAL_BRIDGE_REQUEST_PATH ?? DEFAULT_BRIDGE_REQUEST_PATH,
      source: "env"
    });
  }
  return endpointFromParts({
    host: DEFAULT_BRIDGE_HOST,
    port: DEFAULT_BRIDGE_PORT,
    requestPath: DEFAULT_BRIDGE_REQUEST_PATH,
    source: "default"
  });
}

function endpointFromRuntimeStatus(value, source) {
  const runtime = value.local_bridge_runtime ?? value.local_bridge ?? value;
  if (runtime.active === false) {
    throw new Error(`local bridge runtime is not active in ${source}`);
  }
  if (runtime.url) {
    return endpointFromUrl(runtime.url, source);
  }
  return endpointFromParts({
    host: runtime.bind_host ?? runtime.host ?? DEFAULT_BRIDGE_HOST,
    port: Number(runtime.port),
    requestPath: runtime.request_path ?? runtime.path ?? DEFAULT_BRIDGE_REQUEST_PATH,
    source
  });
}

function endpointFromUrl(rawUrl, source) {
  const parsed = new URL(rawUrl);
  return endpointFromParts({
    host: parsed.hostname,
    port: Number(parsed.port || DEFAULT_BRIDGE_PORT),
    requestPath: parsed.pathname || DEFAULT_BRIDGE_REQUEST_PATH,
    source,
    protocol: parsed.protocol
  });
}

function endpointFromParts({ host, port, requestPath, source, protocol = "http:" }) {
  if (host !== DEFAULT_BRIDGE_HOST && host !== "localhost") {
    throw new Error(`local bridge host must be loopback, got ${host}`);
  }
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    throw new Error(`invalid local bridge port: ${port}`);
  }
  if (!requestPath || !requestPath.startsWith("/")) {
    throw new Error(`invalid local bridge request path: ${requestPath}`);
  }
  if (protocol !== "http:") {
    throw new Error(`local bridge only supports http, got ${protocol}`);
  }
  return {
    url: `http://${host}:${port}${requestPath}`,
    bind_host: host,
    port,
    request_path: requestPath,
    source
  };
}

function defaultBridgeStatusFile() {
  const home = homedir();
  if (!home) return null;
  if (process.platform === "darwin") {
    return join(home, "Library", "Application Support", "NekoDrop", "local_bridge_runtime_status.json");
  }
  if (process.platform === "win32") {
    const appData = process.env.APPDATA;
    return appData ? join(appData, "NekoDrop", "local_bridge_runtime_status.json") : null;
  }
  const configHome = process.env.XDG_CONFIG_HOME ?? join(home, ".config");
  return join(configHome, "NekoDrop", "local_bridge_runtime_status.json");
}

async function postJson(endpoint, requestBody) {
  const url = new URL(endpoint.url);
  const body = JSON.stringify(requestBody);
  return new Promise((resolvePromise, rejectPromise) => {
    const req = httpRequest(
      {
        method: "POST",
        hostname: url.hostname,
        port: url.port,
        path: `${url.pathname}${url.search}`,
        headers: {
          "content-type": "application/json",
          "content-length": Buffer.byteLength(body)
        },
        timeout: 10_000
      },
      (res) => {
        let data = "";
        res.setEncoding("utf8");
        res.on("data", (chunk) => {
          data += chunk;
        });
        res.on("end", () => {
          let parsed = null;
          if (data.trim()) {
            try {
              parsed = JSON.parse(data);
            } catch (error) {
              rejectPromise(new Error(`local bridge returned non-JSON response: ${error.message}`));
              return;
            }
          }
          if (res.statusCode && res.statusCode >= 400) {
            rejectPromise(new Error(`local bridge HTTP ${res.statusCode}: ${data}`));
            return;
          }
          resolvePromise(parsed);
        });
      }
    );
    req.on("timeout", () => {
      req.destroy(new Error("local bridge request timed out"));
    });
    req.on("error", rejectPromise);
    req.write(body);
    req.end();
  });
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

function workspaceSchemaDescriptor(flags) {
  const version = Number(flags["workspace-schema-version"] ?? WORKSPACE_SCHEMA_VERSION);
  if (!SUPPORTED_WORKSPACE_SCHEMA_VERSIONS.includes(version)) {
    throw new Error(`unsupported workspace schema version for export: ${version}`);
  }
  return {
    schema_id: WORKSPACE_SCHEMA_ID,
    current_version: version,
    supported_versions: SUPPORTED_WORKSPACE_SCHEMA_VERSIONS,
    migration_policy: migrationPolicy(flags["migration-policy"] ?? "manual_only")
  };
}

function workspaceMigrationPlan(manifest, flags) {
  const sourceVersion = workspaceSchemaVersionFromManifest(manifest);
  const targetVersion = Number(flags["target-schema-version"] ?? WORKSPACE_SCHEMA_VERSION);
  const policy = migrationPolicy(flags["migration-policy"] ?? manifest.application_schema?.migration_policy ?? "manual_only");
  const base = {
    schema_id: manifest.application_schema?.schema_id ?? manifest.compatibility?.workspace_schema_id ?? WORKSPACE_SCHEMA_ID,
    source_version: sourceVersion,
    target_version: targetVersion,
    supported_versions: SUPPORTED_WORKSPACE_SCHEMA_VERSIONS,
    migration_policy: policy,
    migration_required: sourceVersion !== targetVersion
  };
  if (!SUPPORTED_WORKSPACE_SCHEMA_VERSIONS.includes(sourceVersion)) {
    return {
      ...base,
      status: "unsupported_source_version",
      can_import: false,
      blocking_reason: "unsupported_workspace_schema_version"
    };
  }
  if (!SUPPORTED_WORKSPACE_SCHEMA_VERSIONS.includes(targetVersion)) {
    return {
      ...base,
      status: "unsupported_target_version",
      can_import: false,
      blocking_reason: "unsupported_target_workspace_schema_version"
    };
  }
  if (sourceVersion === targetVersion) {
    return {
      ...base,
      status: "not_required",
      can_import: true,
      blocking_reason: null
    };
  }
  if (policy === "manual_only") {
    return {
      ...base,
      status: "manual_migration_required",
      can_import: false,
      blocking_reason: "workspace_schema_migration_required"
    };
  }
  return {
    ...base,
    status: "adapter_migration_unavailable",
    can_import: false,
    blocking_reason: "workspace_schema_migration_unavailable"
  };
}

function workspaceSchemaVersionFromManifest(manifest) {
  const rawVersion = manifest.application_schema?.version ??
    manifest.compatibility?.workspace_schema_version ??
    manifest.workspace_schema_version ??
    WORKSPACE_SCHEMA_VERSION;
  const version = Number(rawVersion);
  if (!Number.isInteger(version) || version <= 0) {
    throw new Error(`invalid workspace schema version: ${rawVersion}`);
  }
  return version;
}

function validateImportableWorkspaceBundle(manifest, checksums, permissions) {
  if (manifest.schema !== BUNDLE_SCHEMA) {
    throw new Error(`unsupported bundle schema: ${manifest.schema}`);
  }
  if (manifest.bundle_type !== BUNDLE_TYPE) {
    throw new Error(`bundle type mismatch: expected workspace, got ${manifest.bundle_type}`);
  }
  const schemaId = manifest.application_schema?.schema_id ?? manifest.compatibility?.workspace_schema_id ?? WORKSPACE_SCHEMA_ID;
  if (schemaId !== WORKSPACE_SCHEMA_ID) {
    throw new Error(`workspace schema id mismatch: expected ${WORKSPACE_SCHEMA_ID}, got ${schemaId}`);
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
  const receiptVersion = Number(receipt.receipt_version ?? RECEIPT_VERSION);
  if (receiptVersion !== RECEIPT_VERSION) {
    throw new Error(`unsupported adapter import receipt version: ${receipt.receipt_version}`);
  }
  assertSafeBundleId(receipt.bundle_id);
  if (receipt.bundle_type !== BUNDLE_TYPE) {
    throw new Error(`receipt bundle_type must be ${BUNDLE_TYPE}`);
  }
  if ((receipt.workspace_schema_id ?? WORKSPACE_SCHEMA_ID) !== WORKSPACE_SCHEMA_ID) {
    throw new Error(`receipt workspace_schema_id must be ${WORKSPACE_SCHEMA_ID}`);
  }
  const schemaVersion = Number(receipt.workspace_schema_version ?? WORKSPACE_SCHEMA_VERSION);
  if (!SUPPORTED_WORKSPACE_SCHEMA_VERSIONS.includes(schemaVersion)) {
    throw new Error(`unsupported receipt workspace schema version: ${schemaVersion}`);
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

function validateImportTransaction(transaction) {
  if (transaction.schema !== IMPORT_TRANSACTION_SCHEMA) {
    throw new Error(`unsupported import transaction schema: ${transaction.schema}`);
  }
  if (Number(transaction.transaction_version ?? IMPORT_TRANSACTION_VERSION) !== IMPORT_TRANSACTION_VERSION) {
    throw new Error(`unsupported import transaction version: ${transaction.transaction_version}`);
  }
  assertSafeBundleId(transaction.bundle_id);
  if (transaction.bundle_type !== BUNDLE_TYPE) {
    throw new Error(`transaction bundle_type must be ${BUNDLE_TYPE}`);
  }
  if (!IMPORT_TRANSACTION_STATES.includes(transaction.state)) {
    throw new Error(`unsupported import transaction state: ${transaction.state}`);
  }
  if (typeof transaction.target_path !== "string" || transaction.target_path.trim() === "") {
    throw new Error("transaction target_path is required");
  }
  if (typeof transaction.temp_path !== "string" || transaction.temp_path.trim() === "") {
    throw new Error("transaction temp_path is required");
  }
  if (!Array.isArray(transaction.files) || !Array.isArray(transaction.committed_manifest_paths)) {
    throw new Error("transaction committed_manifest_paths is required");
  }
  for (const file of transaction.files) {
    assertSafeBundlePath(file.manifest_path);
    if (!Number.isFinite(file.size) || !/^[a-f0-9]{64}$/.test(file.sha256)) {
      throw new Error(`invalid transaction file entry: ${file.manifest_path}`);
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

function workspaceImportTransactionRoot(targetRoot) {
  return join(targetRoot, "workspaces", ".nekobuddy-workspace-import-transactions");
}

function workspaceImportTempRoot(targetRoot) {
  return join(targetRoot, "workspaces", ".nekobuddy-workspace-import-tmp");
}

function newTransactionId(bundleId) {
  assertSafeBundleId(bundleId);
  return `${bundleId}-${Date.now()}-${process.pid}`;
}

function updateImportTransaction(transaction, patch) {
  Object.assign(transaction, patch);
  writeJson(transaction.transaction_path, transaction);
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

function migrationPolicy(value) {
  if (MIGRATION_POLICIES.has(value)) return value;
  throw new Error("--migration-policy must be manual_only or adapter_managed");
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
  nekobuddy-workspace-adapter recover-import --transaction <path>
  nekobuddy-workspace-adapter contract
  nekobuddy-workspace-adapter request <auth|send|events|results|detail|import|rollback>
  nekobuddy-workspace-adapter discover-bridge [--status-file <path>|--bridge-config <path>|--port <port>]
  nekobuddy-workspace-adapter post <auth|send|events|results|detail|import|rollback> [--bridge-url <url>|--port <port>]
  nekobuddy-workspace-adapter send-workspace --source <dir> --output <dir> --bundle-id <id> --name <name> --target-device-id <id>
  nekobuddy-workspace-adapter receive-workspace --staged-bundle-id <id> --bundle-root <dir> --target-root <dir>
  nekobuddy-workspace-adapter rollback-workspace --bundle-id <id> [--receipt <path>]
  nekobuddy-workspace-adapter workflow --source <dir> --output <dir> --bundle-id <id> --name <name> --target-device-id <id>
`);
}
