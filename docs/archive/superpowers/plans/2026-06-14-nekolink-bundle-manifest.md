# NekoLink Bundle Manifest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add bundle manifest, checksum index, permissions, and validation types to `nekolink-protocol`.

**Architecture:** This change only touches `crates/nekolink-protocol/src/lib.rs`. Bundle manifest validation stays in the protocol crate because it defines wire-compatible JSON shape and protocol-level safety rules. Storage detection, staging, UI preview, local bridge, and transport changes are out of scope.

**Tech Stack:** Rust, serde, existing `ProtocolError` / `ErrorCode`, `cargo test -p nekolink-protocol`.

---

### Task 1: Add failing bundle manifest acceptance tests

**Files:**
- Modify: `crates/nekolink-protocol/src/lib.rs`

- [ ] **Step 0: Add checksum map import**

Extend the existing `std` import so `BundleChecksums` can model the documented `checksums.json.files` object:

```rust
use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};
```

- [ ] **Step 1: Add a test for a valid skill bundle**

Add this test inside the existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn validates_skill_bundle_manifest() {
        let manifest = valid_bundle_manifest();
        let checksums = valid_bundle_checksums();
        let permissions = valid_bundle_permissions(false);

        manifest.validate().unwrap();
        checksums.validate_against(&manifest).unwrap();
        assert!(permissions.can_import().unwrap());
    }
```

- [ ] **Step 2: Add test helpers**

Add these helpers inside the same test module:

```rust
    fn valid_bundle_manifest() -> BundleManifest {
        BundleManifest {
            schema: BUNDLE_SCHEMA_V1.to_string(),
            bundle_id: "bundle_1234567890".to_string(),
            bundle_type: BundleType::Skill,
            display_name: "voice_transcribe".to_string(),
            source_app: "OpenNeko".to_string(),
            created_at: "2026-06-14T10:30:00Z".to_string(),
            sender: BundleSender {
                device_id: "neko-device-1234567890".to_string(),
                device_name: "MacBook".to_string(),
                fingerprint: "sha256:0123456789abcdef".to_string(),
            },
            compatibility: BundleCompatibility {
                min_nekolink_version: PROTOCOL_VERSION,
                required_capabilities: vec![Capability::BundleTransfer],
            },
            summary: BundleSummary {
                file_count: 2,
                total_bytes: 4096,
            },
            files: vec![
                BundleFile {
                    path: "files/manifest.json".to_string(),
                    size: 1024,
                    sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        .to_string(),
                    role: "manifest".to_string(),
                },
                BundleFile {
                    path: "files/content.bin".to_string(),
                    size: 3072,
                    sha256: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                        .to_string(),
                    role: "payload".to_string(),
                },
            ],
        }
    }

    fn valid_bundle_checksums() -> BundleChecksums {
        let mut files = BTreeMap::new();
        files.insert(
            "files/manifest.json".to_string(),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        );
        files.insert(
            "files/content.bin".to_string(),
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_string(),
        );
        BundleChecksums {
            algorithm: BUNDLE_CHECKSUM_SHA256.to_string(),
            files,
        }
    }

    fn valid_bundle_permissions(contains_secrets: bool) -> BundlePermissions {
        BundlePermissions {
            requested_scopes: vec![
                BundlePermissionScope::SkillInstall,
                BundlePermissionScope::WorkspaceImport,
            ],
            writes: vec![BundleWritePermission {
                target: "openneko.skills".to_string(),
                mode: BundleWriteMode::CreateOnly,
            }],
            secrets: BundleSecretsPolicy {
                contains_secrets,
                redacted_fields: Vec::new(),
            },
        }
    }
```

- [ ] **Step 3: Run the test to verify it fails**

Run:

```bash
cargo test -p nekolink-protocol validates_skill_bundle_manifest
```

Expected: FAIL because `BundleManifest` and related types are not defined.

### Task 2: Implement bundle types and valid-case validation

**Files:**
- Modify: `crates/nekolink-protocol/src/lib.rs`

- [ ] **Step 1: Add bundle constants**

Add near existing protocol constants:

```rust
pub const BUNDLE_SCHEMA_V1: &str = "nekolink.bundle.v1";
pub const BUNDLE_CHECKSUM_SHA256: &str = "sha256";
```

- [ ] **Step 2: Add bundle capability**

Add to `Capability`:

```rust
    #[serde(rename = "bundle_transfer")]
    BundleTransfer,
```

Add to `Capability::as_str`:

```rust
            Self::BundleTransfer => "bundle_transfer",
```

- [ ] **Step 3: Add bundle data types**

Add after `ProtocolError` and before `TransferOfferFile`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleType {
    Skill,
    Session,
    Workspace,
    AgentProfile,
    ConfigSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleSender {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleCompatibility {
    pub min_nekolink_version: u16,
    pub required_capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleSummary {
    pub file_count: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleManifest {
    pub schema: String,
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub display_name: String,
    pub source_app: String,
    pub created_at: String,
    pub sender: BundleSender,
    pub compatibility: BundleCompatibility,
    pub summary: BundleSummary,
    pub files: Vec<BundleFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleChecksums {
    pub algorithm: String,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BundlePermissionScope {
    #[serde(rename = "skill.install")]
    SkillInstall,
    #[serde(rename = "session.import")]
    SessionImport,
    #[serde(rename = "workspace.import")]
    WorkspaceImport,
    #[serde(rename = "agent_profile.import")]
    AgentProfileImport,
    #[serde(rename = "config.import")]
    ConfigImport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleWriteMode {
    CreateOnly,
    ManualImport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleWritePermission {
    pub target: String,
    pub mode: BundleWriteMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleSecretsPolicy {
    pub contains_secrets: bool,
    pub redacted_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundlePermissions {
    pub requested_scopes: Vec<BundlePermissionScope>,
    pub writes: Vec<BundleWritePermission>,
    pub secrets: BundleSecretsPolicy,
}
```

- [ ] **Step 4: Add minimal validation methods**

Add implementations:

```rust
impl BundleManifest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema != BUNDLE_SCHEMA_V1 {
            return Err(ProtocolError::new(
                ErrorCode::UnsupportedVersion,
                format!("unsupported bundle schema: {}", self.schema),
            ));
        }
        validate_non_empty("bundle_id", &self.bundle_id)?;
        validate_non_empty("display_name", &self.display_name)?;
        validate_non_empty("source_app", &self.source_app)?;
        validate_non_empty("created_at", &self.created_at)?;
        self.sender.validate()?;
        self.compatibility.validate()?;
        if self.summary.file_count != self.files.len() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                format!(
                    "bundle file count mismatch: {} != {}",
                    self.summary.file_count,
                    self.files.len()
                ),
            ));
        }
        let total_bytes = self.files.iter().map(|file| file.size).sum::<u64>();
        if self.summary.total_bytes != total_bytes {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                format!(
                    "bundle total bytes mismatch: {} != {}",
                    self.summary.total_bytes, total_bytes
                ),
            ));
        }
        for file in &self.files {
            file.validate()?;
        }
        Ok(())
    }
}

impl BundleSender {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("sender device_id", &self.device_id)?;
        validate_non_empty("sender device_name", &self.device_name)?;
        validate_non_empty("sender fingerprint", &self.fingerprint)
    }
}

impl BundleCompatibility {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.min_nekolink_version == 0 {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "min_nekolink_version cannot be 0",
            ));
        }
        if self.required_capabilities.is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "required_capabilities cannot be empty",
            ));
        }
        Ok(())
    }
}

impl BundleFile {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_bundle_path(&self.path)?;
        validate_sha256_hex("bundle file sha256", &self.sha256)?;
        validate_non_empty("bundle file role", &self.role)
    }
}

impl BundleChecksums {
    pub fn validate_against(&self, manifest: &BundleManifest) -> Result<(), ProtocolError> {
        if self.algorithm != BUNDLE_CHECKSUM_SHA256 {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                format!("unsupported bundle checksum algorithm: {}", self.algorithm),
            ));
        }
        if self.files.len() != manifest.files.len() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "bundle checksum file count mismatch",
            ));
        }
        for (path, sha256) in &self.files {
            validate_bundle_path(path)?;
            validate_sha256_hex("bundle checksum sha256", sha256)?;
        }
        for manifest_file in &manifest.files {
            let checksum = self.files.get(&manifest_file.path).ok_or_else(|| {
                ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    format!("missing checksum for {}", manifest_file.path),
                )
            })?;
            if checksum != &manifest_file.sha256 {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    format!("checksum mismatch for {}", manifest_file.path),
                ));
            }
        }
        Ok(())
    }
}

impl BundlePermissions {
    pub fn can_import(&self) -> Result<bool, ProtocolError> {
        self.validate()?;
        Ok(!self.secrets.contains_secrets)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.requested_scopes.is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "requested_scopes cannot be empty",
            ));
        }
        for write in &self.writes {
            write.validate()?;
        }
        for field in &self.secrets.redacted_fields {
            validate_non_empty("redacted field", field)?;
        }
        Ok(())
    }
}

impl BundleWritePermission {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_logical_target(&self.target)
    }
}
```

- [ ] **Step 5: Add helper validators**

Add private helpers near `validate_transfer_manifest_path`:

```rust
fn validate_non_empty(field: &str, value: &str) -> Result<(), ProtocolError> {
    if value.trim().is_empty() {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("{field} cannot be empty"),
        ));
    }
    Ok(())
}

fn validate_sha256_hex(field: &str, value: &str) -> Result<(), ProtocolError> {
    if value.len() != 64 || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("{field} must be a 64-character hex SHA-256"),
        ));
    }
    Ok(())
}

fn validate_bundle_path(path: &str) -> Result<(), ProtocolError> {
    validate_transfer_manifest_path(path)?;
    if path == "bundle.json" || path == "checksums.json" || path == "permissions.json" {
        return Ok(());
    }
    if !path.starts_with("files/") {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "bundle payload path must be under files/",
        ));
    }
    Ok(())
}

fn validate_logical_target(target: &str) -> Result<(), ProtocolError> {
    validate_non_empty("write target", target)?;
    if target.starts_with('/')
        || target.starts_with('\\')
        || target.contains('\\')
        || target.contains("..")
        || target.contains(':')
    {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "write target must be a logical target, not a filesystem path",
        ));
    }
    Ok(())
}
```

- [ ] **Step 6: Run the valid-case test**

Run:

```bash
cargo test -p nekolink-protocol validates_skill_bundle_manifest
```

Expected: PASS.

### Task 3: Add failing invalid-case tests

**Files:**
- Modify: `crates/nekolink-protocol/src/lib.rs`

- [ ] **Step 1: Add tests for invalid manifest fields**

Add these tests:

```rust
    #[test]
    fn rejects_bundle_manifest_with_mismatched_summary() {
        let mut manifest = valid_bundle_manifest();
        manifest.summary.file_count = 99;

        let error = manifest.validate().unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("file count mismatch"));
    }

    #[test]
    fn rejects_bundle_manifest_with_unsafe_paths() {
        let mut manifest = valid_bundle_manifest();
        manifest.files[0].path = "../secret.txt".to_string();

        let error = manifest.validate().unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("unsafe path segment"));
    }

    #[test]
    fn rejects_bundle_manifest_with_bad_sha256() {
        let mut manifest = valid_bundle_manifest();
        manifest.files[0].sha256 = "not-a-hash".to_string();

        let error = manifest.validate().unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("64-character hex"));
    }
```

- [ ] **Step 2: Add tests for checksums and permissions**

Add:

```rust
    #[test]
    fn rejects_bundle_checksums_that_do_not_match_manifest() {
        let manifest = valid_bundle_manifest();
        let mut checksums = valid_bundle_checksums();
        *checksums.files.get_mut("files/manifest.json").unwrap() =
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_string();

        let error = checksums.validate_against(&manifest).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("checksum mismatch"));
    }

    #[test]
    fn marks_bundle_permissions_with_secrets_as_not_importable() {
        let permissions = valid_bundle_permissions(true);

        assert!(!permissions.can_import().unwrap());
    }

    #[test]
    fn rejects_bundle_write_permissions_with_filesystem_targets() {
        let mut permissions = valid_bundle_permissions(false);
        permissions.writes[0].target = "/Users/example/.ssh".to_string();

        let error = permissions.can_import().unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("logical target"));
    }
```

- [ ] **Step 3: Run invalid tests**

Run:

```bash
cargo test -p nekolink-protocol bundle_
```

Expected: PASS after Task 2 implementation. If any test passes before implementation, review it.

### Task 4: Verify full protocol crate and update docs status

**Files:**
- Modify: `docs/STATUS.md`

- [ ] **Step 1: Update current status**

Change the `NekoLink bundle spec` row to mention that protocol manifest types are now implemented, while receive detection/staging/bridge remain pending.

- [ ] **Step 2: Run focused crate tests**

Run:

```bash
cargo test -p nekolink-protocol
```

Expected: PASS.

- [ ] **Step 3: Run workspace check**

Run:

```bash
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 4: Run formatting and whitespace checks**

Run:

```bash
cargo fmt --all -- --check
git diff --check
```

Expected: both PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/nekolink-protocol/src/lib.rs docs/STATUS.md docs/superpowers/plans/2026-06-14-nekolink-bundle-manifest.md
git commit -m "feat: add nekolink bundle manifest model"
```

Expected: one commit on `protocol/nekolink-bundle-manifest`.
