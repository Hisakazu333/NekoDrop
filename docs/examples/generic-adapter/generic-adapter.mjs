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
import { basename, join, relative, sep } from "node:path";

const BUNDLE_SCHEMA = "nekolink.bundle.v1";
const CHECKSUM_ALGORITHM = "sha256";
const CLIENT = {
  client_id: "generic.adapter.sample",
  display_name: "Generic Adapter Sample",
  app_kind: "generic"
};

const TYPE_CONFIG = {
  skill: { scope: "skill.install", target: "adapter.skill" },
  session: { scope: "session.import", target: "adapter.session" },
  workspace: { scope: "workspace.import", target: "adapter.workspace" },
  agent_profile: { scope: "agent_profile.import", target: "adapter.agent_profile" },
  config_snapshot: { scope: "config.import", target: "adapter.config" }
};

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});

async function main() {
  const [command, ...args] = process.argv.slice(2);
  if (command === "export") {
    printJson(exportBundle(parseFlags(args)));
    return;
  }
  if (command === "request") {
    const [kind, ...rest] = args;
    printJson(buildRequest(kind, parseFlags(rest)));
    return;
  }
  if (command === "workflow") {
    printJson(buildWorkflow(parseFlags(args)));
    return;
  }
  if (command === "post") {
    const [kind, ...rest] = args;
    await postRequest(kind, parseFlags(rest));
    return;
  }
  if (command === "cursor") {
    printJson(nextCursorFromResponse(parseFlags(args)));
    return;
  }
  usage();
  process.exit(command ? 1 : 0);
}

function exportBundle(flags) {
  const source = requireFlag(flags, "source");
  const output = requireFlag(flags, "output");
  const bundleId = requireFlag(flags, "bundle-id");
  const bundleType = requireKnownType(requireFlag(flags, "type"));
  const displayName = requireFlag(flags, "name");
  const sourceApp = flags["source-app"] ?? "Generic Adapter";
  const containsSecrets = flags["contains-secrets"] === "true";
  const stripFields = toArray(flags["strip-field"]);

  if (!existsSync(source) || !statSync(source).isDirectory()) {
    throw new Error(`--source must be a directory: ${source}`);
  }
  assertSafeBundleId(bundleId);

  const bundleRoot = join(output, bundleId);
  rmSync(bundleRoot, { recursive: true, force: true });
  mkdirSync(join(bundleRoot, "files"), { recursive: true });

  const payloadFiles = [];
  for (const sourcePath of listFiles(source)) {
    const sourceRelative = normalizePath(relative(source, sourcePath));
    const destinationRelative = `files/${sourceRelative}`;
    const destination = join(bundleRoot, destinationRelative);
    mkdirSync(destination.slice(0, destination.lastIndexOf(sep)), { recursive: true });
    copySanitizedFile(sourcePath, destination, stripFields);
    const bytes = readFileSync(destination);
    payloadFiles.push({
      path: destinationRelative,
      size: bytes.byteLength,
      sha256: sha256(bytes),
      role: roleForFile(sourceRelative)
    });
  }

  if (payloadFiles.length === 0) {
    throw new Error("--source must contain at least one file");
  }

  payloadFiles.sort((left, right) => left.path.localeCompare(right.path));
  const manifest = {
    schema: BUNDLE_SCHEMA,
    bundle_id: bundleId,
    bundle_type: bundleType,
    display_name: displayName,
    source_app: sourceApp,
    created_at: new Date().toISOString(),
    sender: {
      device_id: "generic-adapter-sample",
      device_name: "Generic Adapter Host",
      fingerprint: "sha256:sample"
    },
    compatibility: {
      min_nekolink_version: 1,
      required_capabilities: ["bundle_transfer"]
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
  const config = TYPE_CONFIG[bundleType];
  const permissions = {
    requested_scopes: [config.scope],
    writes: [
      {
        target: config.target,
        mode: "manual_import"
      }
    ],
    secrets: {
      contains_secrets: containsSecrets,
      redacted_fields: stripFields
    }
  };

  writeJson(join(bundleRoot, "bundle.json"), manifest);
  writeJson(join(bundleRoot, "checksums.json"), checksums);
  writeJson(join(bundleRoot, "permissions.json"), permissions);

  return {
    bundle_root: bundleRoot,
    bundle_id: bundleId,
    bundle_type: bundleType,
    file_count: payloadFiles.length,
    total_bytes: manifest.summary.total_bytes
  };
}

function buildRequest(kind, flags) {
  if (kind === "auth") {
    return {
      kind: "authorization.request",
      payload: {
        request_id: flags["request-id"] ?? `adapter-auth-${Date.now()}`,
        client: CLIENT,
        requested_scopes: toArray(flags.scope).length > 0
          ? toArray(flags.scope)
          : ["device.read", "bundle.send", "bundle.import.request", "transfer.status.read"],
        reason: flags.reason ?? "Send and import user-selected bundles",
        ttl_seconds: Number(flags["ttl-seconds"] ?? 3600)
      }
    };
  }
  if (kind === "send") {
    return {
      kind: "bundle.send",
      payload: {
        request_id: flags["request-id"] ?? `adapter-send-${Date.now()}`,
        client: CLIENT,
        target_device_id: requireFlag(flags, "target-device-id"),
        bundle_root: requireFlag(flags, "bundle-root"),
        bundle_type: requireKnownType(requireFlag(flags, "type")),
        require_trusted_device: flags["require-trusted-device"] !== "false"
      }
    };
  }
  if (kind === "import") {
    return {
      kind: "bundle.import",
      payload: {
        request_id: flags["request-id"] ?? `adapter-import-${Date.now()}`,
        client: CLIENT,
        staged_bundle_id: requireFlag(flags, "staged-bundle-id"),
        expected_bundle_type: requireKnownType(requireFlag(flags, "type")),
        conflict_strategy: requireConflictStrategy(flags["conflict-strategy"] ?? "reject")
      }
    };
  }
  if (kind === "detail") {
    return {
      kind: "bundle.detail",
      payload: {
        request_id: flags["request-id"] ?? `adapter-detail-${Date.now()}`,
        client: CLIENT,
        staged_bundle_id: requireFlag(flags, "staged-bundle-id")
      }
    };
  }
  if (kind === "rollback") {
    return {
      kind: "bundle.rollback",
      payload: {
        request_id: flags["request-id"] ?? `adapter-rollback-${Date.now()}`,
        client: CLIENT,
        bundle_id: requireFlag(flags, "bundle-id")
      }
    };
  }
  if (kind === "events") {
    return {
      kind: "events.poll",
      payload: {
        request_id: flags["request-id"] ?? `adapter-events-${Date.now()}`,
        client: CLIENT,
        after_event_id: flags["after-event-id"] ?? null,
        limit: Number(flags.limit ?? 20),
        timeout_ms: flags["timeout-ms"] === undefined ? undefined : Number(flags["timeout-ms"])
      }
    };
  }
  if (kind === "results") {
    return {
      kind: "actions.results",
      payload: {
        request_id: flags["request-id"] ?? `adapter-results-${Date.now()}`,
        client: CLIENT,
        action_request_id: flags["action-request-id"] ?? null,
        after_claimed_at_ms: flags["after-claimed-at-ms"] === undefined || flags["after-claimed-at-ms"] === null
          ? null
          : Number(flags["after-claimed-at-ms"]),
        limit: Number(flags.limit ?? 20)
      }
    };
  }
  throw new Error("request kind must be auth, send, detail, import, rollback, events, or results");
}

function buildWorkflow(flags) {
  const mode = flags.mode ?? "send";
  const steps = [];
  const sendRequestId = flags["send-request-id"] ?? "adapter-send-001";
  const importRequestId = flags["import-request-id"] ?? "adapter-import-001";
  const rollbackRequestId = flags["rollback-request-id"] ?? "adapter-rollback-001";
  if (mode === "full-loop") {
    const bundleRoot = flags["bundle-root"] ?? join(requireFlag(flags, "output"), requireFlag(flags, "bundle-id"));
    return {
      client: CLIENT,
      mode,
      steps: [
        {
          step: "export",
          command: buildExportCommand(flags),
          produces: { bundle_root: bundleRoot }
        },
        {
          step: "authorize",
          request: buildRequest("auth", {
            scope: ["device.read", "bundle.read", "bundle.send", "bundle.import.request", "transfer.status.read"],
            reason: flags.reason ?? "Send and import user-selected bundles"
          })
        },
        {
          step: "send",
          request: buildRequest("send", {
            "request-id": sendRequestId,
            "bundle-root": bundleRoot,
            "target-device-id": requireFlag(flags, "target-device-id"),
            type: requireKnownType(requireFlag(flags, "type")),
            "require-trusted-device": "true"
          })
        },
        {
          step: "observe_send",
          request: buildRequest("events", {
            "after-event-id": flags["after-event-id"] ?? null,
            limit: flags.limit ?? 20,
            "timeout-ms": flags["timeout-ms"] ?? 15000
          })
        },
        {
          step: "send_results",
          request: buildRequest("results", {
            "action-request-id": sendRequestId,
            "after-claimed-at-ms": flags["after-claimed-at-ms"] ?? null,
            limit: flags.limit ?? 20
          })
        },
        {
          step: "inspect_received_bundle",
          request: buildRequest("detail", {
            "staged-bundle-id": requireFlag(flags, "staged-bundle-id")
          })
        },
        {
          step: "import",
          request: buildRequest("import", {
            "request-id": importRequestId,
            "staged-bundle-id": requireFlag(flags, "staged-bundle-id"),
            type: requireKnownType(requireFlag(flags, "type")),
            "conflict-strategy": flags["conflict-strategy"] ?? "reject"
          })
        },
        {
          step: "observe_import",
          request: buildRequest("events", {
            "after-event-id": flags["after-event-id"] ?? null,
            limit: flags.limit ?? 20,
            "timeout-ms": flags["timeout-ms"] ?? 15000
          })
        },
        {
          step: "inspect_after_import",
          request: buildRequest("detail", {
            "staged-bundle-id": requireFlag(flags, "staged-bundle-id")
          })
        },
        {
          step: "import_results",
          request: buildRequest("results", {
            "action-request-id": importRequestId,
            "after-claimed-at-ms": flags["after-claimed-at-ms"] ?? null,
            limit: flags.limit ?? 20
          })
        },
        {
          step: "rollback",
          request: buildRequest("rollback", {
            "request-id": rollbackRequestId,
            "bundle-id": requireFlag(flags, "staged-bundle-id")
          })
        },
        {
          step: "observe_rollback",
          request: buildRequest("events", {
            "after-event-id": flags["after-event-id"] ?? null,
            limit: flags.limit ?? 20,
            "timeout-ms": flags["timeout-ms"] ?? 15000
          })
        },
        {
          step: "rollback_results",
          request: buildRequest("results", {
            "action-request-id": rollbackRequestId,
            "after-claimed-at-ms": flags["after-claimed-at-ms"] ?? null,
            limit: flags.limit ?? 20
          })
        }
      ],
      notes: [
        "Run export on the sending device.",
        "POST bridge requests on the device that owns that phase.",
        "Keep events_next_after_id between observe calls; reset to null when events_cursor_state is missing.",
        "Rollback only removes files imported into NekoDrop's local import area."
      ]
    };
  }
  steps.push({
    step: "authorize",
    request: buildRequest("auth", {
      scope: ["device.read", "bundle.read", "bundle.send", "bundle.import.request", "transfer.status.read"],
      reason: flags.reason ?? "Send and import user-selected bundles"
    })
  });
  if (mode === "send" || mode === "roundtrip") {
    steps.push({
      step: "send",
      request: buildRequest("send", {
        "request-id": sendRequestId,
        "bundle-root": requireFlag(flags, "bundle-root"),
        "target-device-id": requireFlag(flags, "target-device-id"),
        type: requireKnownType(requireFlag(flags, "type")),
        "require-trusted-device": flags["require-trusted-device"]
      })
    });
  }
  steps.push({
    step: "observe",
    request: buildRequest("events", {
      "after-event-id": flags["after-event-id"] ?? null,
      limit: flags.limit ?? 20,
      "timeout-ms": flags["timeout-ms"] ?? 15000
    })
  });
  if (mode === "import" || mode === "roundtrip") {
    steps.push({
      step: "inspect",
      request: buildRequest("detail", {
        "staged-bundle-id": requireFlag(flags, "staged-bundle-id")
      })
    });
    steps.push({
      step: "import",
      request: buildRequest("import", {
        "request-id": importRequestId,
        "staged-bundle-id": requireFlag(flags, "staged-bundle-id"),
        type: requireKnownType(requireFlag(flags, "type")),
        "conflict-strategy": flags["conflict-strategy"] ?? "reject"
      })
    });
    steps.push({
      step: "inspect_after_import",
      request: buildRequest("detail", {
        "staged-bundle-id": requireFlag(flags, "staged-bundle-id")
      })
    });
  }
  if (mode === "rollback") {
    steps.push({
      step: "inspect_before_rollback",
      request: buildRequest("detail", {
        "staged-bundle-id": requireFlag(flags, "bundle-id")
      })
    });
    steps.push({
      step: "rollback",
      request: buildRequest("rollback", {
        "request-id": rollbackRequestId,
        "bundle-id": requireFlag(flags, "bundle-id")
      })
    });
  }
  const resultActionRequestId =
    mode === "rollback"
      ? rollbackRequestId
      : mode === "import" || mode === "roundtrip"
        ? importRequestId
        : mode === "send"
          ? sendRequestId
          : null;
  steps.push({
    step: "results",
    request: buildRequest("results", {
      "action-request-id": resultActionRequestId,
      "after-claimed-at-ms": flags["after-claimed-at-ms"] ?? null,
      limit: flags.limit ?? 20
    })
  });

  return {
    client: CLIENT,
    mode,
    steps
  };
}

function buildExportCommand(flags) {
  const command = [
    "node",
    "docs/examples/generic-adapter/generic-adapter.mjs",
    "export",
    "--source",
    requireFlag(flags, "source"),
    "--output",
    requireFlag(flags, "output"),
    "--bundle-id",
    requireFlag(flags, "bundle-id"),
    "--type",
    requireKnownType(requireFlag(flags, "type")),
    "--name",
    requireFlag(flags, "name")
  ];
  if (flags["source-app"]) {
    command.push("--source-app", flags["source-app"]);
  }
  for (const field of toArray(flags["strip-field"])) {
    command.push("--strip-field", field);
  }
  if (flags["contains-secrets"] !== undefined) {
    command.push("--contains-secrets", flags["contains-secrets"]);
  }
  return command;
}

async function postRequest(kind, flags) {
  const port = requireFlag(flags, "port");
  const url = `http://127.0.0.1:${port}/nekolink/local-bridge/v1`;
  const request = buildRequest(kind, flags);
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request)
  });
  const text = await response.text();
  if (!response.ok) {
    throw new Error(`local bridge returned HTTP ${response.status}: ${text}`);
  }
  console.log(text);
}

function nextCursorFromResponse(flags) {
  const responsePath = requireFlag(flags, "response");
  const response = JSON.parse(readFileSync(responsePath, "utf8"));
  const cursorState = response.events_cursor_state ?? "ok";
  if (cursorState === "missing") {
    return {
      after_event_id: null,
      cursor_state: cursorState,
      reset_required: true
    };
  }
  return {
    after_event_id: response.events_next_after_id ?? null,
    cursor_state: cursorState,
    reset_required: false
  };
}

function listFiles(root) {
  const files = [];
  for (const name of readdirSync(root).sort()) {
    if (name.startsWith(".")) continue;
    const path = join(root, name);
    const metadata = statSync(path);
    if (metadata.isDirectory()) {
      files.push(...listFiles(path));
    } else if (metadata.isFile()) {
      files.push(path);
    }
  }
  return files;
}

function copySanitizedFile(source, destination, stripFields) {
  if (source.endsWith(".json") && stripFields.length > 0) {
    const value = JSON.parse(readFileSync(source, "utf8"));
    for (const field of stripFields) {
      deletePath(value, field);
    }
    writeJson(destination, value);
    return;
  }
  copyFileSync(source, destination);
}

function deletePath(value, path) {
  const parts = path.split(".").filter(Boolean);
  let cursor = value;
  for (const part of parts.slice(0, -1)) {
    if (!cursor || typeof cursor !== "object") return;
    cursor = cursor[part];
  }
  if (cursor && typeof cursor === "object") {
    delete cursor[parts.at(-1)];
  }
}

function roleForFile(path) {
  const name = basename(path).toLowerCase();
  if (name.includes("manifest") || name.endsWith(".json")) return "manifest";
  return "payload";
}

function parseFlags(args) {
  const flags = {};
  for (let index = 0; index < args.length; index += 1) {
    const token = args[index];
    if (!token.startsWith("--")) {
      throw new Error(`unexpected argument: ${token}`);
    }
    const key = token.slice(2);
    const next = args[index + 1];
    const value = !next || next.startsWith("--") ? "true" : args[++index];
    if (flags[key] === undefined) {
      flags[key] = value;
    } else if (Array.isArray(flags[key])) {
      flags[key].push(value);
    } else {
      flags[key] = [flags[key], value];
    }
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

function requireKnownType(type) {
  if (!TYPE_CONFIG[type]) {
    throw new Error(`unsupported bundle type: ${type}`);
  }
  return type;
}

function requireConflictStrategy(strategy) {
  if (["reject", "rename", "skip_conflicts"].includes(strategy)) return strategy;
  throw new Error("--conflict-strategy must be reject, rename, or skip_conflicts");
}

function assertSafeBundleId(bundleId) {
  if (!/^[A-Za-z0-9._-]+$/.test(bundleId) || bundleId.includes("..")) {
    throw new Error(`unsafe bundle id: ${bundleId}`);
  }
}

function normalizePath(path) {
  return path.split(sep).join("/");
}

function toArray(value) {
  if (value === undefined) return [];
  return Array.isArray(value) ? value : [value];
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function printJson(value) {
  console.log(JSON.stringify(value, null, 2));
}

function usage() {
  console.log(`Usage:
  node generic-adapter.mjs export --source DIR --output DIR --bundle-id ID --type session --name NAME
  node generic-adapter.mjs request auth
  node generic-adapter.mjs request send --bundle-root DIR --target-device-id ID --type workspace
  node generic-adapter.mjs request detail --staged-bundle-id ID
  node generic-adapter.mjs request import --staged-bundle-id ID --type session --conflict-strategy reject
  node generic-adapter.mjs request rollback --bundle-id ID
  node generic-adapter.mjs request events --timeout-ms 15000
  node generic-adapter.mjs request results --action-request-id ACTION_REQUEST_ID
  node generic-adapter.mjs workflow --mode roundtrip --bundle-root DIR --target-device-id ID --staged-bundle-id ID --type workspace --conflict-strategy rename
  node generic-adapter.mjs workflow --mode full-loop --source DIR --output DIR --bundle-id ID --name NAME --target-device-id ID --staged-bundle-id ID --type workspace --conflict-strategy rename
  node generic-adapter.mjs workflow --mode rollback --bundle-id ID
  node generic-adapter.mjs cursor --response bridge-events-response.json
  node generic-adapter.mjs post send --port 47321 --bundle-root DIR --target-device-id ID --type workspace`);
}
