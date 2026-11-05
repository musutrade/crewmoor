fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
fn run() -> crewmoor::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let name = args
        .first()
        .map(std::path::Path::new)
        .and_then(std::path::Path::file_name);
    if name == Some(std::ffi::OsStr::new("opencode")) {
        return crewmoor::opencode::launch::run(&args[1..]);
    }
    crewmoor::engine::cli(&args[1..])
}
