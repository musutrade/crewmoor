//! Runs the real `crewmoor feedback hook` callback with explicit stdin and a hard timeout,
//! so a callback that waits on input fails the test instead of hanging the suite.
//! `Some(bytes)` is written and then stdin is closed.
use std::{
    io::{Read, Write},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

pub fn run(
    agent: &str,
    registry: &std::path::Path,
    envs: &[(&str, &str)],
    input: Option<&[u8]>,
) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_crewmoor"));
    command
        .args(["feedback", "hook", agent, registry.to_str().unwrap()])
        .env_remove(crewmoor::feedback::INSTANCE_ENV)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command.spawn().unwrap();
    let mut writer = child.stdin.take();
    // `None` keeps the writer open for the whole run; the callback must not read it.
    if let Some(bytes) = input {
        writer.take().unwrap().write_all(bytes).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("hook callback did not exit within 10 s");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    drop(writer);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_end(&mut stderr)
        .unwrap();
    Output {
        status,
        stdout,
        stderr,
    }
}
