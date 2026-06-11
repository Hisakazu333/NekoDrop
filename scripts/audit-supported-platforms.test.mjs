import assert from "node:assert/strict";
import test from "node:test";

import {
  buildOsvQueryBatch,
  collectResolvedCrates,
  groupFindingsByIdentity,
  parseAuditTargets,
  selectBlockingFindings,
} from "./audit-supported-platforms.mjs";

test("collectResolvedCrates only keeps crates present in the resolved target graph", () => {
  const metadata = {
    packages: [
      {
        id: "registry+https://github.com/rust-lang/crates.io-index#tauri@2.11.2",
        name: "tauri",
        version: "2.11.2",
        source: "registry+https://github.com/rust-lang/crates.io-index",
      },
      {
        id: "registry+https://github.com/rust-lang/crates.io-index#glib@0.18.5",
        name: "glib",
        version: "0.18.5",
        source: "registry+https://github.com/rust-lang/crates.io-index",
      },
      {
        id: "path+file:///repo/crates/nekodrop-core#0.1.0",
        name: "nekodrop-core",
        version: "0.1.0",
        source: null,
      },
    ],
    resolve: {
      nodes: [
        {
          id: "registry+https://github.com/rust-lang/crates.io-index#tauri@2.11.2",
        },
        {
          id: "path+file:///repo/crates/nekodrop-core#0.1.0",
        },
      ],
    },
  };

  assert.deepEqual(collectResolvedCrates(metadata), [
    {
      name: "tauri",
      version: "2.11.2",
      purl: "pkg:cargo/tauri@2.11.2",
    },
  ]);
});

test("parseAuditTargets accepts comma-separated target triples", () => {
  assert.deepEqual(parseAuditTargets("aarch64-apple-darwin, x86_64-pc-windows-msvc"), [
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
  ]);
});

test("selectBlockingFindings blocks moderate or worse vulnerabilities", () => {
  const findings = [
    {
      packageName: "glib",
      packageVersion: "0.18.5",
      id: "GHSA-wrw7-89jp-8q8g",
      severity: "MODERATE",
      summary: "Unsound iterator",
      target: "x86_64-unknown-linux-gnu",
    },
    {
      packageName: "legacy",
      packageVersion: "1.0.0",
      id: "RUSTSEC-0000-0000",
      severity: "UNKNOWN",
      summary: "Unmaintained",
      target: "x86_64-pc-windows-msvc",
    },
  ];

  assert.deepEqual(selectBlockingFindings(findings).map((finding) => finding.id), [
    "GHSA-wrw7-89jp-8q8g",
  ]);
});

test("buildOsvQueryBatch uses ecosystem and name without purl", () => {
  assert.deepEqual(
    buildOsvQueryBatch([
      {
        name: "tauri",
        version: "2.11.2",
        purl: "pkg:cargo/tauri@2.11.2",
      },
    ]),
    {
      queries: [
        {
          package: {
            ecosystem: "crates.io",
            name: "tauri",
          },
          version: "2.11.2",
        },
      ],
    },
  );
});

test("groupFindingsByIdentity folds the same advisory across targets", () => {
  assert.deepEqual(
    groupFindingsByIdentity([
      {
        ecosystem: "crates.io",
        packageName: "unic-common",
        packageVersion: "0.9.0",
        id: "RUSTSEC-2025-0080",
        severity: "UNKNOWN",
        summary: "unmaintained",
        target: "x86_64-apple-darwin",
      },
      {
        ecosystem: "crates.io",
        packageName: "unic-common",
        packageVersion: "0.9.0",
        id: "RUSTSEC-2025-0080",
        severity: "UNKNOWN",
        summary: "unmaintained",
        target: "aarch64-apple-darwin",
      },
    ]),
    [
      {
        ecosystem: "crates.io",
        packageName: "unic-common",
        packageVersion: "0.9.0",
        id: "RUSTSEC-2025-0080",
        severity: "UNKNOWN",
        summary: "unmaintained",
        targets: ["aarch64-apple-darwin", "x86_64-apple-darwin"],
      },
    ],
  );
});
