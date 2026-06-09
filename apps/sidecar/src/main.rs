use std::env;
use std::net::TcpListener;
use std::path::PathBuf;

use nekodrop_network::Endpoint;
use nekodrop_service::{
    accept_transfer, connection_code_for_endpoint, create_transfer_plan,
    endpoint_from_connection_code, send_paths,
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
         nekodrop-sidecar send <host:port|connection-code> <path> [path...]"
    );
}
