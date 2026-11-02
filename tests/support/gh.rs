// `gh` test double for tests/issue.rs. Never calls GitHub.
// In $GH_FIXTURE_DIR: writes its arguments one per line to `gh-args`, prints `answer`,
// then exits with the code in `status`. Missing setup exits 2.
use std::{env, fs, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let Some(dir) = env::var_os("GH_FIXTURE_DIR").map(PathBuf::from) else {
        return ExitCode::from(2);
    };
    let args: Vec<String> = env::args().skip(1).collect();
    let logged = fs::write(dir.join("gh-args"), args.join("\n") + "\n").is_ok();
    print!(
        "{}",
        fs::read_to_string(dir.join("answer")).unwrap_or_default()
    );
    eprintln!("fake gh stderr");
    let status = fs::read_to_string(dir.join("status"))
        .ok()
        .and_then(|s| s.trim().parse().ok());
    ExitCode::from(status.filter(|_| logged).unwrap_or(2u8))
}
