// Transport-level test double. Never used as a real container or Agent.
mod environment;
mod live_environment;
fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|arg| arg == "environment-evidence") {
        return result(live_environment::report(args.last()));
    }
    if let Some(value) = environment::dispatch(&args) {
        return result(value);
    }
    if args.iter().any(|a| a == "detach-child") {
        return result(detach_child(args.last()));
    }
    if args.iter().any(|a| a == "socket-server") {
        return result(socket_server(args.last()));
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
    use std::io::Read;
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

fn detach_child(root: Option<&String>) -> crewmoor::Result<()> {
    let root = root.ok_or("missing state")?;
    let record = crewmoor::record::Record::load(std::path::Path::new(root))?;
    let socket = record.root.join("transport.sock");
    let child = crewmoor::native::server_command(&record)?
        .arg("socket-server")
        .arg(&socket)
        .spawn()?;
    crewmoor::native::wait_socket(&socket)?;
    println!("{}", child.id());
    Ok(())
}
fn socket_server(path: Option<&String>) -> crewmoor::Result<()> {
    let path = path.ok_or("missing socket")?;
    let listener = std::os::unix::net::UnixListener::bind(path)?;
    let (stream, _) = listener.accept()?;
    drop(stream);
    Ok(())
}
