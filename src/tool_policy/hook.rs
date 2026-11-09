use super::{Policy, Scan, launch::Snapshot};
use crate::{
    Result,
    config::AgentKind,
    feedback::{self, model},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

#[derive(Debug, Serialize)]
pub struct Decision {
    pub decision: &'static str,
    pub reason: Option<String>,
    pub unresolved: bool,
}
pub fn cli(args: &[String]) -> Result<()> {
    let [mode, path] = args else {
        return Err("usage: crewmoor tool-policy hook|bridge SNAPSHOT".into());
    };
    if !matches!(mode.as_str(), "hook" | "bridge") {
        return Err("choose tool-policy hook or bridge".into());
    }
    let decision = evaluate(Path::new(path), mode == "bridge");
    let output = if mode == "bridge" {
        serde_json::to_value(decision)?
    } else {
        native_output(&decision)
    };
    println!("{output}");
    Ok(())
}
fn evaluate(path: &Path, bridge: bool) -> Decision {
    let mut role = "unknown".to_string();
    let result = load_and_check(path, bridge, &mut role);
    match result {
        Ok((policy, scan)) => match scan.entry {
            Some(entry) => deny(&role, &entry, &policy.feedback),
            None => Decision {
                decision: "no_match",
                reason: None,
                unresolved: scan.unresolved,
            },
        },
        Err(error) => deny(&role, "checker failure", &error.to_string()),
    }
}
fn load_and_check(path: &Path, bridge: bool, role: &mut String) -> Result<(Policy, Scan)> {
    let snapshot: Snapshot = serde_json::from_slice(&fs::read(path)?)?;
    *role = snapshot.context.registration.role.clone();
    snapshot.policy.validate()?;
    let input = read_input()?;
    identity(&snapshot, &input, bridge)?;
    let (command, cwd) = command(&snapshot, &input, bridge)?;
    let scan = match command {
        Some(command) => bounded(snapshot.policy.clone(), cwd, command)?,
        None => Scan {
            entry: None,
            unresolved: true,
        },
    };
    Ok((snapshot.policy, scan))
}
fn read_input() -> Result<Value> {
    let mut input = String::new();
    std::io::stdin()
        .take(1_048_577)
        .read_to_string(&mut input)?;
    super::bounded_source(&input)?;
    Ok(serde_json::from_str(&input)?)
}
fn identity(snapshot: &Snapshot, input: &Value, bridge: bool) -> Result<()> {
    role_identity(snapshot)?;
    session_identity(snapshot, input, bridge)
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
fn session_identity(snapshot: &Snapshot, input: &Value, bridge: bool) -> Result<()> {
    let expected = &snapshot.context;
    let session = feedback::registered_session(&snapshot.registry, expected)?
        .ok_or("native SessionStart/session association missing")?;
    let field = if bridge { "sessionID" } else { "session_id" };
    if input[field].as_str() != Some(&session) {
        return Err("native tool session mismatch".into());
    }
    if bridge && input["instance"].as_str() != Some(expected.instance.as_str()) {
        return Err("bridge instance mismatch".into());
    }
    if bridge != (expected.registration.agent == AgentKind::OpenCode) {
        return Err("tool hook agent kind mismatch".into());
    }
    Ok(())
}
fn command(snapshot: &Snapshot, input: &Value, bridge: bool) -> Result<(Option<String>, PathBuf)> {
    let Some(args) = tool_arguments(input, bridge)? else {
        return Ok((None, snapshot.context.registration.cwd.clone()));
    };
    let command = args["command"]
        .as_str()
        .ok_or("native shell command missing")?
        .to_owned();
    let cwd = working_directory(snapshot, input, bridge)?;
    Ok((Some(command), cwd))
}
fn tool_arguments(input: &Value, bridge: bool) -> Result<Option<&Value>> {
    if !bridge && input["hook_event_name"] != "PreToolUse" {
        return Err("expected native PreToolUse input".into());
    }
    let field = if bridge { "tool" } else { "tool_name" };
    let tool = input[field].as_str().ok_or("native tool name missing")?;
    let expected = if bridge { "shell" } else { "Bash" };
    if tool != expected {
        return Ok(None);
    }
    Ok(Some(if bridge {
        &input["input"]
    } else {
        &input["tool_input"]
    }))
}
fn working_directory(snapshot: &Snapshot, input: &Value, bridge: bool) -> Result<PathBuf> {
    let cwd = if bridge {
        match input["input"].get("workdir") {
            Some(value) => PathBuf::from(
                value
                    .as_str()
                    .ok_or("native shell workdir must be a string")?,
            ),
            None => snapshot.context.registration.cwd.clone(),
        }
    } else {
        PathBuf::from(input["cwd"].as_str().ok_or("native tool cwd missing")?)
    };
    let cwd = snapshot.context.registration.cwd.join(cwd);
    crate::config::absolute(&cwd)?;
    Ok(cwd)
}
fn bounded(policy: Policy, cwd: PathBuf, command: String) -> Result<Scan> {
    let budget = Duration::from_millis(policy.timeout_ms);
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result = super::scan(&policy, &cwd, &command).map_err(|error| error.to_string());
        let _ = sender.send(result);
    });
    receiver
        .recv_timeout(budget)
        .map_err(|_| "role checker timed out or failed")?
        .map_err(Into::into)
}
fn deny(role: &str, entry: &str, detail: &str) -> Decision {
    Decision {
        decision: "deny",
        unresolved: false,
        reason: Some(format!(
            "STOP: 本次命令被执行前角色检查拒绝，未执行。\nINTENT: 全量测试/覆盖率或检查失败。\nReason: {detail}\nDetail: 当前角色 {role}；命中入口 {entry}。\nState: denied before execution。\nNext: 全量检查交提交角色；禁止换脚本或语言绕过。仅运行必要且符合明确配置的定向测试。"
        )),
    }
}
fn native_output(decision: &Decision) -> Value {
    if decision.decision == "deny" {
        json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":decision.reason}})
    } else {
        // No permission grant and no extra warning/context for ordinary no-match calls.
        json!({})
    }
}
