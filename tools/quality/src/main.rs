fn main() -> std::process::ExitCode {
    match crewmoor_quality::driver::cli(std::env::args().skip(1).collect()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("QUALITY GATE FAILED: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
