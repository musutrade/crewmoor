// Transport-level test double. Never used as a real container or Agent.
fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().collect();
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
