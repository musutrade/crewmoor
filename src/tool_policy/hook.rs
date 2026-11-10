use super::launch::Snapshot;
use crate::{
    Result,
    config::AgentKind,
    feedback::{self, model},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Seek, Write},
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};
pub fn cli(args: &[String]) -> Result<()> {
    if args == ["verify"] {
        return verify();
    }
    let [mode, path] = args else {
        return Err("usage: crewmoor tool-policy verify; legacy hook|bridge SNAPSHOT".into());
    };
    if !matches!(mode.as_str(), "hook" | "bridge") {
        return Err("unknown tool-policy mode".into());
    }
    let result = legacy(Path::new(path), mode == "bridge");
    let output = match result {
        Ok(output) => output,
        Err(error) => denial(mode == "bridge", &error.to_string()),
    };
    println!("{output}");
    Ok(())
}
fn input() -> Result<Value> {
    let mut text = String::new();
    std::io::stdin().take(1_048_577).read_to_string(&mut text)?;
    if text.len() > 1_048_576 {
        return Err("tool input exceeds 1 MiB".into());
    }
    Ok(serde_json::from_str(&text)?)
}
fn verify() -> Result<()> {
    let path =
        std::env::var_os("TOOL_RULE_HOOK_CONTEXT").ok_or("trusted role context path missing")?;
    let snapshot: Snapshot = serde_json::from_slice(&fs::read(path)?)?;
    let value = input()?;
    let bridge = value["bridge"].as_bool().ok_or("verifier mode missing")?;
    role_identity(&snapshot)?;
    session_identity(&snapshot, &value["input"], bridge, false)?;
    println!(
        "{}",
        json!({"context":format!("role={} instance={}",snapshot.context.registration.role,snapshot.context.instance)})
    );
    Ok(())
}
fn legacy(path: &Path, bridge: bool) -> Result<Value> {
    let mut value: Value = serde_json::from_slice(&fs::read(path)?)?;
    let snapshot: Snapshot = serde_json::from_value(value.clone())?;
    let input = input()?;
    identity(&snapshot, &input, bridge)?;
    let checker = legacy_checker()?;
    let policy = value["policy"]
        .as_object_mut()
        .ok_or("legacy policy missing")?;
    policy.remove("checker");
    policy.insert("base_dir".into(), json!(snapshot.context.registration.cwd));
    let mut command = Command::new(checker);
    command.arg(if bridge {
        "compat-bridge"
    } else {
        "compat-hook"
    });
    forward(
        command,
        &json!({"policy":policy,"input":input,"context":format!("role={} instance={}",snapshot.context.registration.role,snapshot.context.instance)}),
    )
}
fn legacy_checker() -> Result<std::ffi::OsString> {
    let checker = std::env::var_os("TOOL_RULE_HOOK_EXECUTABLE").ok_or("legacy tool-policy requires explicit TOOL_RULE_HOOK_EXECUTABLE absolute path; install the independent package and migrate new role configuration")?;
    crate::config::absolute(Path::new(&checker))?;
    Ok(checker)
}
pub(super) fn forward(command: Command, value: &Value) -> Result<Value> {
    let (mut child, mut stdout) = legacy_io(command, value)?;
    wait(&mut child)?;
    stdout.rewind()?;
    let mut output = String::new();
    stdout.take(1_048_577).read_to_string(&mut output)?;
    if output.len() > 1_048_576 {
        return Err("legacy checker output too large".into());
    }
    Ok(serde_json::from_str(&output)?)
}
fn legacy_io(mut command: Command, value: &Value) -> Result<(std::process::Child, std::fs::File)> {
    let stdout = tempfile::tempfile()?;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(stdout.try_clone()?)
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or("checker stdin missing")?;
    let bytes = value.to_string();
    std::thread::spawn(move || {
        let _ = stdin.write_all(bytes.as_bytes());
    });
    Ok((child, stdout))
}
fn wait(child: &mut std::process::Child) -> Result<()> {
    let start = std::time::Instant::now();
    while child.try_wait()?.is_none() {
        if start.elapsed() >= Duration::from_secs(6) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("legacy independent checker timed out".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    if !child.wait()?.success() {
        return Err("legacy independent checker failed".into());
    }
    Ok(())
}
fn denial(bridge: bool, reason: &str) -> Value {
    let reason = format!("Tool rule hook adapter: denied; {reason}");
    if bridge {
        json!({"decision":"deny","reason":reason,"unresolved":true})
    } else {
        json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}})
    }
}
fn identity(snapshot: &Snapshot, input: &Value, bridge: bool) -> Result<()> {
    role_identity(snapshot)?;
    session_identity(snapshot, input, bridge, true)
}
fn role_identity(snapshot: &Snapshot) -> Result<()> {
    let expected = &snapshot.context;
    let marker = std::env::var(feedback::INSTANCE_ENV)?;
    if marker != expected.instance {
        return Err("role instance mismatch".into());
    }
    let actual = model::load_for_agent(&snapshot.registry, &marker, &expected.registration.agent)?;
    if actual != *expected {
        return Err("registered role identity changed".into());
    }
    feedback::require_unchanged(&snapshot.registry, expected)?;
    Ok(())
}
fn session_identity(
    snapshot: &Snapshot,
    input: &Value,
    bridge: bool,
    require_event_instance: bool,
) -> Result<()> {
    let expected = &snapshot.context;
    let session = feedback::registered_session(&snapshot.registry, expected)?
        .ok_or("native SessionStart/session association missing")?;
    let field = if bridge { "sessionID" } else { "session_id" };
    if input[field].as_str() != Some(&session) {
        return Err("native tool session mismatch".into());
    }
    event_identity(expected, input, bridge, require_event_instance)
}
fn event_identity(
    expected: &feedback::Context,
    input: &Value,
    bridge: bool,
    require_event_instance: bool,
) -> Result<()> {
    if expected.registration.agent == AgentKind::Codex
        && input.get("agent_id").is_some_and(|id| !id.is_null())
    {
        return Err("native Codex child caller is not this registered root role".into());
    }
    if bridge
        && require_event_instance
        && input["instance"].as_str() != Some(expected.instance.as_str())
    {
        return Err("bridge instance mismatch".into());
    }
    if bridge != (expected.registration.agent == AgentKind::OpenCode) {
        return Err("tool hook agent kind mismatch".into());
    }
    Ok(())
}
