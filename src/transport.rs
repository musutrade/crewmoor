use crate::{Result, failure::NativeFailure};
use std::{
    io::{Read, Seek},
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

pub fn output(cmd: &mut Command, limit: Duration) -> Result<Output> {
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?)
        .spawn()?;
    let status = wait(&mut child, limit)?;
    let out = snapshot(&mut stdout)?;
    let err = snapshot(&mut stderr)?;
    Ok(Output {
        status,
        stdout: out,
        stderr: err,
    })
}
fn snapshot(file: &mut std::fs::File) -> Result<Vec<u8>> {
    file.rewind()?;
    let length = file.metadata()?.len();
    let mut bytes = Vec::new();
    file.take(length).read_to_end(&mut bytes)?;
    Ok(bytes)
}
fn wait(child: &mut Child, limit: Duration) -> Result<std::process::ExitStatus> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if start.elapsed() >= limit {
            return timeout(child);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn timeout(child: &mut Child) -> Result<std::process::ExitStatus> {
    child.kill()?;
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if child.try_wait()?.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err(NativeFailure {
        code: "timeout".into(),
        message: format!(
            "owned CLI {} timed out; request outcome unknown; never resend",
            child.id()
        ),
    }
    .into())
}
