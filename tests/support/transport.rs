// Transport-level test double. Never used as a real container or Agent.
mod environment;
mod live_environment;
mod message_transport;
mod probe;
fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().collect();
    if let Some(value) = message_transport::dispatch(&args) {
        return result(value);
    }
    if let Some(value) = probe::dispatch(&args) {
        return result(value);
    }
    if args.iter().any(|arg| arg == "environment-evidence") {
        return result(live_environment::report(args.last()));
    }
    if let Some(value) = environment::dispatch(&args) {
        return result(value);
    }
    if args.iter().any(|a| a == "hold") {
        return hold();
    }
    if args.iter().any(|a| a == "--version") {
        println!("fixture 1");
        return std::process::ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "reply") {
        return reply(args.last());
    }
    std::process::ExitCode::FAILURE
}
fn result(value: crewmoor::Result<()>) -> std::process::ExitCode {
    match value {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => std::process::ExitCode::FAILURE,
    }
}
fn hold() -> std::process::ExitCode {
    use std::io::{Read, Write};
    if std::env::var_os("CREWMOOR_TRANSPORT_HANDSHAKE").is_some()
        && std::io::stdout().write_all(b"R\n").is_err()
    {
        return std::process::ExitCode::FAILURE;
    }
    let _ = std::io::stdin().read_to_end(&mut Vec::new());
    std::process::ExitCode::SUCCESS
}
fn reply(value: Option<&String>) -> std::process::ExitCode {
    match value {
        Some(value) => {
            println!("{value}");
            std::process::ExitCode::SUCCESS
        }
        None => std::process::ExitCode::FAILURE,
    }
}
