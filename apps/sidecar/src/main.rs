use std::collections::BTreeMap;
use std::env;
use std::net::TcpListener;
use std::path::PathBuf;

use nekobuddy_resource_adapters::{
    build_app_manifest as build_resource_app_manifest,
    build_bridge_request as build_resource_bridge_request,
    build_descriptor as build_resource_descriptor,
    build_transaction_contract as build_resource_transaction_contract, confirm_resource_import,
    dry_run_resource_import, export_resource_bundle, recover_resource_import,
    rollback_resource_import, ExportResourceBundleRequest, ImportResourceRequest,
    NekobuddyResourceKind, ResourceBridgeRequestOptions,
};
use nekobuddy_workspace_adapter::{
    build_app_manifest, build_bridge_request, build_descriptor, build_transaction_contract,
    confirm_workspace_import, dry_run_workspace_import, export_workspace_bundle,
    recover_workspace_import, rollback_workspace_import, workspace_app_export_send,
    workspace_app_import_confirm, workspace_app_import_review, workspace_app_rollback_latest,
    BridgeRequestOptions, ExportWorkspaceBundleRequest, ImportWorkspaceRequest,
    WorkspaceAppExportSendRequest, WorkspaceAppImportConfirmRequest,
    WorkspaceAppImportReviewRequest, WorkspaceAppRollbackRequest, WorkspaceConflictStrategy,
    WorkspaceMigrationPolicy,
};
use nekodrop_network::Endpoint;
use nekodrop_service::{
    accept_transfer, connection_code_for_endpoint, create_transfer_plan,
    endpoint_from_connection_code, send_paths,
};
use nekolink_adapter_contract::{
    AdapterConflictStrategy as ResourceConflictStrategy,
    AdapterMigrationPolicy as ResourceMigrationPolicy,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let Some(command) = args.first().map(String::as_str) else {
        print_usage();
        return Err("missing command".into());
    };

    match command {
        "plan" => run_plan(&args[1..]),
        "receive" => run_receive(&args[1..]),
        "send" => run_send(&args[1..]),
        "workspace-adapter" => run_workspace_adapter(&args[1..]),
        "session-adapter" => run_resource_adapter(NekobuddyResourceKind::Session, &args[1..]),
        "skill-adapter" => run_resource_adapter(NekobuddyResourceKind::Skill, &args[1..]),
        "agent-profile-adapter" => {
            run_resource_adapter(NekobuddyResourceKind::AgentProfile, &args[1..])
        }
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        _ => {
            print_usage();
            Err(format!("unknown command: {command}"))
        }
    }
}

fn run_plan(args: &[String]) -> Result<(), String> {
    let paths = args.iter().map(PathBuf::from).collect::<Vec<_>>();
    let plan = create_transfer_plan(&paths).map_err(|error| error.to_string())?;
    println!(
        "root={} files={} bytes={}",
        plan.manifest.root_name,
        plan.file_count(),
        plan.total_bytes()
    );
    for file in plan.files {
        println!(
            "file path={} size={} sha256={} source={}",
            file.manifest_path,
            file.size,
            file.sha256,
            file.source_path.display()
        );
    }
    Ok(())
}

fn run_receive(args: &[String]) -> Result<(), String> {
    if args.len() != 2 {
        print_usage();
        return Err("receive requires <bind-host:port> <receive-dir>".into());
    }

    let listener = TcpListener::bind(&args[0])
        .map_err(|error| format!("failed to bind {}: {error}", args[0]))?;
    let receive_dir = PathBuf::from(&args[1]);
    let local_addr = listener
        .local_addr()
        .map_err(|error| format!("failed to read listener address: {error}"))?;
    let connection_code = connection_code_for_endpoint(
        Endpoint::tcp(local_addr.ip().to_string(), local_addr.port()),
        None,
    )
    .map_err(|error| error.to_string())?;
    println!(
        "listening={} receive_dir={}",
        local_addr,
        receive_dir.display()
    );
    println!("code={connection_code}");

    let report = accept_transfer(&listener, &receive_dir).map_err(|error| error.to_string())?;
    println!("received files={}", report.files.len());
    for file in report.files {
        println!(
            "received path={} bytes={} sha256={} verified={}",
            file.path.display(),
            file.bytes_written,
            file.sha256,
            file.verified
        );
    }

    Ok(())
}

fn run_send(args: &[String]) -> Result<(), String> {
    if args.len() < 2 {
        print_usage();
        return Err("send requires <host:port> <path> [path...]".into());
    }

    let endpoint = parse_endpoint_or_connection_code(&args[0])?;
    let paths = args[1..].iter().map(PathBuf::from).collect::<Vec<_>>();
    let report = send_paths(&endpoint, &paths).map_err(|error| error.to_string())?;
    println!(
        "sent root={} files={} bytes={}",
        report.plan.manifest.root_name,
        report.sent_files.len(),
        report.plan.total_bytes()
    );
    for file in report.sent_files {
        println!("sent path={} bytes={}", file.manifest_path, file.bytes_sent);
    }

    Ok(())
}

fn run_resource_adapter(kind: NekobuddyResourceKind, args: &[String]) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        print_usage();
        return Err("resource adapter requires a command".into());
    };
    let (positionals, flags) = parse_flags(&args[1..])?;
    match command {
        "descriptor" => print_json(&build_resource_descriptor(kind)),
        "app-manifest" => print_json(&build_resource_app_manifest(kind)),
        "contract" => print_json(&build_resource_transaction_contract(kind)),
        "export" => {
            let migration_policy = flag_optional(&flags, "migration-policy")
                .map(ResourceMigrationPolicy::parse)
                .transpose()?
                .unwrap_or(ResourceMigrationPolicy::ManualOnly);
            let result = export_resource_bundle(ExportResourceBundleRequest {
                kind,
                source_path: PathBuf::from(flag_required(&flags, "source")?),
                output_root: PathBuf::from(flag_required(&flags, "output")?),
                bundle_id: flag_required(&flags, "bundle-id")?.to_string(),
                display_name: flag_required(&flags, "name")?.to_string(),
                contains_secrets: flag_bool(&flags, "contains-secrets"),
                migration_policy,
            })
            .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "import-dry-run" => {
            let result = dry_run_resource_import(resource_import_request(kind, &flags)?)
                .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "import-confirm" => {
            let result = confirm_resource_import(resource_import_request(kind, &flags)?)
                .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "rollback" => {
            let result =
                rollback_resource_import(kind, &PathBuf::from(flag_required(&flags, "receipt")?))
                    .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "recover-import" => {
            let result = recover_resource_import(
                kind,
                &PathBuf::from(flag_required(&flags, "transaction")?),
            )
            .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "request" => {
            let Some(request_kind) = positionals.first() else {
                return Err("resource adapter request requires <auth|send|detail|import|rollback|events|results>".into());
            };
            let result = build_resource_bridge_request(
                kind,
                request_kind,
                resource_bridge_request_options(&flags)?,
            )
            .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        _ => Err(format!("unknown resource adapter command: {command}")),
    }
}

fn run_workspace_adapter(args: &[String]) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        print_usage();
        return Err("workspace-adapter requires a command".into());
    };
    let (positionals, flags) = parse_flags(&args[1..])?;
    match command {
        "descriptor" => print_json(&build_descriptor()),
        "app-manifest" => print_json(&build_app_manifest()),
        "contract" => print_json(&build_transaction_contract()),
        "app-export-send" => {
            let migration_policy = flag_optional(&flags, "migration-policy")
                .map(WorkspaceMigrationPolicy::parse)
                .transpose()
                .map_err(|error| error.to_string())?
                .unwrap_or(WorkspaceMigrationPolicy::ManualOnly);
            let result = workspace_app_export_send(WorkspaceAppExportSendRequest {
                source_path: PathBuf::from(flag_required(&flags, "source")?),
                output_root: PathBuf::from(flag_required(&flags, "output")?),
                bundle_id: flag_required(&flags, "bundle-id")?.to_string(),
                display_name: flag_required(&flags, "name")?.to_string(),
                contains_secrets: flag_bool(&flags, "contains-secrets"),
                migration_policy,
                target_device_id: flag_optional(&flags, "target-device-id").map(ToOwned::to_owned),
                request_id: flag_optional(&flags, "request-id").map(ToOwned::to_owned),
            })
            .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "app-import-review" => {
            let result = workspace_app_import_review(workspace_app_import_review_request(&flags)?)
                .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "app-import-confirm" => {
            let result =
                workspace_app_import_confirm(workspace_app_import_confirm_request(&flags)?)
                    .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "app-rollback" => {
            let result = workspace_app_rollback_latest(WorkspaceAppRollbackRequest {
                registry_path: PathBuf::from(flag_required(&flags, "registry")?),
                bundle_id: flag_required(&flags, "bundle-id")?.to_string(),
            })
            .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "export" => {
            let migration_policy = flag_optional(&flags, "migration-policy")
                .map(WorkspaceMigrationPolicy::parse)
                .transpose()
                .map_err(|error| error.to_string())?
                .unwrap_or(WorkspaceMigrationPolicy::ManualOnly);
            let result = export_workspace_bundle(ExportWorkspaceBundleRequest {
                source_path: PathBuf::from(flag_required(&flags, "source")?),
                output_root: PathBuf::from(flag_required(&flags, "output")?),
                bundle_id: flag_required(&flags, "bundle-id")?.to_string(),
                display_name: flag_required(&flags, "name")?.to_string(),
                contains_secrets: flag_bool(&flags, "contains-secrets"),
                migration_policy,
            })
            .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "import-dry-run" => {
            let result = dry_run_workspace_import(workspace_import_request(&flags)?)
                .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "import-confirm" => {
            let result = confirm_workspace_import(workspace_import_request(&flags)?)
                .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "rollback" => {
            let result =
                rollback_workspace_import(&PathBuf::from(flag_required(&flags, "receipt")?))
                    .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "recover-import" => {
            let result =
                recover_workspace_import(&PathBuf::from(flag_required(&flags, "transaction")?))
                    .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "request" => {
            let Some(kind) = positionals.first() else {
                return Err("workspace-adapter request requires <auth|send|detail|import|rollback|events|results>".into());
            };
            let result = build_bridge_request(kind, bridge_request_options(&flags)?)
                .map_err(|error| error.to_string())?;
            print_json(&result)
        }
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        _ => Err(format!("unknown workspace-adapter command: {command}")),
    }
}

fn resource_import_request(
    kind: NekobuddyResourceKind,
    flags: &BTreeMap<String, String>,
) -> Result<ImportResourceRequest, String> {
    let conflict_strategy = flag_optional(flags, "conflict-strategy")
        .map(ResourceConflictStrategy::parse)
        .transpose()?
        .unwrap_or(ResourceConflictStrategy::Reject);
    Ok(ImportResourceRequest {
        kind,
        bundle_root: PathBuf::from(flag_required(flags, "bundle-root")?),
        target_root: PathBuf::from(flag_required(flags, "target-root")?),
        conflict_strategy,
        simulate_fail_after_copy: flag_bool(flags, "simulate-fail-after-copy"),
    })
}

fn workspace_import_request(
    flags: &BTreeMap<String, String>,
) -> Result<ImportWorkspaceRequest, String> {
    let conflict_strategy = flag_optional(flags, "conflict-strategy")
        .map(WorkspaceConflictStrategy::parse)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or(WorkspaceConflictStrategy::Reject);
    Ok(ImportWorkspaceRequest {
        bundle_root: PathBuf::from(flag_required(flags, "bundle-root")?),
        target_root: PathBuf::from(flag_required(flags, "target-root")?),
        conflict_strategy,
        simulate_fail_after_copy: flag_bool(flags, "simulate-fail-after-copy"),
    })
}

fn workspace_app_import_review_request(
    flags: &BTreeMap<String, String>,
) -> Result<WorkspaceAppImportReviewRequest, String> {
    let conflict_strategy = flag_optional(flags, "conflict-strategy")
        .map(WorkspaceConflictStrategy::parse)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or(WorkspaceConflictStrategy::Reject);
    Ok(WorkspaceAppImportReviewRequest {
        bundle_root: PathBuf::from(flag_required(flags, "bundle-root")?),
        target_root: PathBuf::from(flag_required(flags, "target-root")?),
        conflict_strategy,
    })
}

fn workspace_app_import_confirm_request(
    flags: &BTreeMap<String, String>,
) -> Result<WorkspaceAppImportConfirmRequest, String> {
    let conflict_strategy = flag_optional(flags, "conflict-strategy")
        .map(WorkspaceConflictStrategy::parse)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or(WorkspaceConflictStrategy::Reject);
    Ok(WorkspaceAppImportConfirmRequest {
        bundle_root: PathBuf::from(flag_required(flags, "bundle-root")?),
        target_root: PathBuf::from(flag_required(flags, "target-root")?),
        conflict_strategy,
        registry_path: PathBuf::from(flag_required(flags, "registry")?),
    })
}

fn resource_bridge_request_options(
    flags: &BTreeMap<String, String>,
) -> Result<ResourceBridgeRequestOptions, String> {
    let conflict_strategy = flag_optional(flags, "conflict-strategy")
        .map(ResourceConflictStrategy::parse)
        .transpose()?;
    let ttl_seconds = flag_optional(flags, "ttl-seconds")
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|error| format!("invalid --ttl-seconds: {error}"))
        })
        .transpose()?;
    Ok(ResourceBridgeRequestOptions {
        request_id: flag_optional(flags, "request-id").map(ToOwned::to_owned),
        target_device_id: flag_optional(flags, "target-device-id").map(ToOwned::to_owned),
        bundle_root: flag_optional(flags, "bundle-root").map(PathBuf::from),
        staged_bundle_id: flag_optional(flags, "staged-bundle-id").map(ToOwned::to_owned),
        bundle_id: flag_optional(flags, "bundle-id").map(ToOwned::to_owned),
        action_request_id: flag_optional(flags, "action-request-id").map(ToOwned::to_owned),
        ttl_seconds: ttl_seconds.or(Some(3600)),
        conflict_strategy,
    })
}

fn bridge_request_options(
    flags: &BTreeMap<String, String>,
) -> Result<BridgeRequestOptions, String> {
    let conflict_strategy = flag_optional(flags, "conflict-strategy")
        .map(WorkspaceConflictStrategy::parse)
        .transpose()
        .map_err(|error| error.to_string())?;
    let ttl_seconds = flag_optional(flags, "ttl-seconds")
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|error| format!("invalid --ttl-seconds: {error}"))
        })
        .transpose()?;
    Ok(BridgeRequestOptions {
        request_id: flag_optional(flags, "request-id").map(ToOwned::to_owned),
        target_device_id: flag_optional(flags, "target-device-id").map(ToOwned::to_owned),
        bundle_root: flag_optional(flags, "bundle-root").map(PathBuf::from),
        staged_bundle_id: flag_optional(flags, "staged-bundle-id").map(ToOwned::to_owned),
        bundle_id: flag_optional(flags, "bundle-id").map(ToOwned::to_owned),
        action_request_id: flag_optional(flags, "action-request-id").map(ToOwned::to_owned),
        ttl_seconds: ttl_seconds.or(Some(3600)),
        conflict_strategy,
    })
}

fn parse_flags(args: &[String]) -> Result<(Vec<String>, BTreeMap<String, String>), String> {
    let mut positionals = Vec::new();
    let mut flags = BTreeMap::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if let Some(name) = arg.strip_prefix("--") {
            if name.is_empty() {
                return Err("empty flag name".into());
            }
            if index + 1 >= args.len() || args[index + 1].starts_with("--") {
                flags.insert(name.to_string(), "true".to_string());
                index += 1;
            } else {
                flags.insert(name.to_string(), args[index + 1].clone());
                index += 2;
            }
        } else {
            positionals.push(arg.clone());
            index += 1;
        }
    }
    Ok((positionals, flags))
}

fn flag_required<'a>(flags: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str, String> {
    flags
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing --{name}"))
}

fn flag_optional<'a>(flags: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    flags
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn flag_bool(flags: &BTreeMap<String, String>, name: &str) -> bool {
    matches!(
        flag_optional(flags, name),
        Some("true" | "1" | "yes" | "on")
    )
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed to serialize json: {error}"))?;
    println!("{json}");
    Ok(())
}

fn parse_endpoint_or_connection_code(value: &str) -> Result<Endpoint, String> {
    if value.starts_with("nekodrop-v1;") {
        return endpoint_from_connection_code(value).map_err(|error| error.to_string());
    }

    let (host, port) = value
        .rsplit_once(':')
        .ok_or_else(|| format!("endpoint must be <host:port>: {value}"))?;
    let port = port
        .parse::<u16>()
        .map_err(|error| format!("invalid endpoint port in {value}: {error}"))?;
    Ok(Endpoint::tcp(host, port))
}

fn print_usage() {
    eprintln!(
        "NekoDrop sidecar\n\
         \n\
         Commands:\n\
         nekodrop-sidecar plan <path> [path...]\n\
         nekodrop-sidecar receive <bind-host:port> <receive-dir>\n\
         nekodrop-sidecar send <host:port|connection-code> <path> [path...]\n\
         nekodrop-sidecar workspace-adapter descriptor\n\
         nekodrop-sidecar workspace-adapter app-manifest\n\
         nekodrop-sidecar workspace-adapter contract\n\
         nekodrop-sidecar workspace-adapter app-export-send --source <dir> --output <dir> --bundle-id <id> --name <name> [--target-device-id <id>]\n\
         nekodrop-sidecar workspace-adapter app-import-review --bundle-root <dir> --target-root <dir>\n\
         nekodrop-sidecar workspace-adapter app-import-confirm --bundle-root <dir> --target-root <dir> --registry <path>\n\
         nekodrop-sidecar workspace-adapter app-rollback --registry <path> --bundle-id <id>\n\
         nekodrop-sidecar workspace-adapter export --source <dir> --output <dir> --bundle-id <id> --name <name>\n\
         nekodrop-sidecar workspace-adapter import-dry-run --bundle-root <dir> --target-root <dir>\n\
         nekodrop-sidecar workspace-adapter import-confirm --bundle-root <dir> --target-root <dir>\n\
         nekodrop-sidecar workspace-adapter rollback --receipt <path>\n\
         nekodrop-sidecar workspace-adapter recover-import --transaction <path>\n\
         nekodrop-sidecar workspace-adapter request <auth|send|detail|import|rollback|events|results>\n\
         nekodrop-sidecar session-adapter <descriptor|app-manifest|contract|export|import-dry-run|import-confirm|rollback|recover-import|request>\n\
         nekodrop-sidecar skill-adapter <descriptor|app-manifest|contract|export|import-dry-run|import-confirm|rollback|recover-import|request>\n\
         nekodrop-sidecar agent-profile-adapter <descriptor|app-manifest|contract|export|import-dry-run|import-confirm|rollback|recover-import|request>"
    );
}
