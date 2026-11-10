use crate::{Policy, Result, Scan};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
pub const RULES_ENV: &str = "TOOL_RULE_HOOK_RULES";
pub const VERIFY_ENV: &str = "TOOL_RULE_HOOK_VERIFY";
pub const CONTEXT_ENV: &str = "TOOL_RULE_HOOK_CONTEXT";
const OPENCODE: &[u8] = include_bytes!("../assets/opencode/index.ts");
#[derive(Debug, Serialize)]
pub struct Decision {
    pub decision: &'static str,
    pub reason: Option<String>,
    pub unresolved: bool,
}
pub fn cli(args: &[String]) -> Result<()> {
    match args {
        [mode,path]=>cli_file(mode,Path::new(path)),
        [mode,first,second]=>cli_three(mode,Path::new(first),second),
        [mode] if mode=="optional-hook"=>emit_optional(),
        [mode] if mode=="codex-hook"=>crate::codex::emit(),
        [mode] if ["hook","bridge","compat-hook","compat-bridge"].contains(&mode.as_str())=>emit(mode),
        _=>Err("usage: tool-rule-hook hook|optional-hook|codex-hook|bridge; validate RULES; export-plugin|export-marketplace|export-opencode NEW_DIRECTORY; [codex-]session-bind|session-update|session-end|session-show STORE < REQUEST; bridge-session STORE REALM; rules-update DEST SOURCE".into()),
    }
}
fn cli_file(mode: &str, path: &Path) -> Result<()> {
    if mode.starts_with("session-") || mode.starts_with("codex-session-") {
        let output = crate::session::control(mode, path, input()?)?;
        println!("{output}");
        return Ok(());
    }
    cli_path(mode, path)
}
fn cli_three(mode: &str, first: &Path, second: &str) -> Result<()> {
    match mode {
        "bridge-session" => emit_session(first, second),
        "rules-update" => update_rules(first, Path::new(second)),
        _ => Err("unknown tool-rule-hook command".into()),
    }
}
fn cli_path(mode: &str, path: &Path) -> Result<()> {
    match mode {
        "validate" => load(path)?.validate(),
        "export-opencode" => export(path),
        "export-plugin" => crate::assets::export(path),
        "export-marketplace" => crate::assets::marketplace(path),
        _ => Err("unknown tool-rule-hook command".into()),
    }
}
fn update_rules(destination: &Path, source: &Path) -> Result<()> {
    let policy = load(source)?;
    policy.validate()?;
    crate::session::atomic(destination, &serde_json::to_vec_pretty(&policy)?)
}
fn emit_session(root: &Path, scope: &str) -> Result<()> {
    let result = check_session(root, scope)
        .unwrap_or_else(|error| deny("checker failure", &error.to_string(), "", true));
    let output = serde_json::to_value(result)?;
    println!("{output}");
    Ok(())
}
fn check_session(root: &Path, scope: &str) -> Result<Decision> {
    let event = input()?;
    let binding = crate::session::resolve(root, scope, &event)?;
    check_binding(binding, event, true)
}
pub(crate) fn check_binding(
    binding: crate::session::Binding,
    event: Value,
    bridge: bool,
) -> Result<Decision> {
    let source = binding
        .rules
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "explicit unrestricted binding".into());
    let mut context = format!(
        "calling_session={} rules_source={} revision={} scope={}",
        binding.session, source, binding.revision, binding.scope
    );
    if let Some(verifier) = &binding.verifier {
        context.push_str(" verified_context=");
        context.push_str(&verifier.check_mode(&event, 1_000, bridge)?);
    }
    let Some(policy) = binding.policy else {
        return Ok(Decision {
            decision: "no_match",
            reason: None,
            unresolved: false,
        });
    };
    policy.validate()?;
    let event = if bridge {
        session_cwd(event, &binding.directory)?
    } else {
        event
    };
    let (command, cwd) = command(&event, bridge, &policy.base_dir)?;
    let scan = command
        .map(|command| bounded(policy.clone(), cwd, command))
        .transpose()?
        .unwrap_or(Scan {
            entry: None,
            unresolved: true,
        });
    Ok(decide(policy, scan, context))
}
fn session_cwd(mut event: Value, base: &Path) -> Result<Value> {
    if event["tool"] != "shell" {
        return Ok(event);
    }
    let directory = event["input"]
        .get("workdir")
        .map(|value| value.as_str().ok_or("native workdir must be text"))
        .transpose()?
        .unwrap_or(".");
    let directory = expand_home(directory)?;
    let directory = crate::normalize::path(Some(base), &directory)?;
    event["input"]["workdir"] = json!(directory);
    Ok(event)
}
fn expand_home(value: &str) -> Result<String> {
    if value == "~" || value.starts_with("~/") {
        let home = std::env::var("HOME").map_err(|_| "native home directory is unavailable")?;
        return Ok(format!("{home}{}", &value[1..]));
    }
    if value.contains('\\') {
        return Err("Windows workdir syntax is unsupported by this local Unix scanner".into());
    }
    Ok(value.into())
}

fn decide(policy: Policy, scan: Scan, context: String) -> Decision {
    match scan.entry {
        Some(entry) => deny(&entry, &policy.feedback.text(), &context, scan.unresolved),
        None => Decision {
            decision: "no_match",
            reason: None,
            unresolved: scan.unresolved,
        },
    }
}
fn emit_optional() -> Result<()> {
    if [RULES_ENV, CONTEXT_ENV, VERIFY_ENV]
        .into_iter()
        .all(|name| std::env::var_os(name).is_none())
    {
        println!("{{}}");
        return Ok(());
    }
    emit("hook")
}
fn emit(mode: &str) -> Result<()> {
    let bridge = mode.ends_with("bridge");
    let result = evaluate(bridge, mode.starts_with("compat-"));
    let output = if bridge {
        serde_json::to_value(result)?
    } else {
        native_output(&result)
    };
    println!("{output}");
    Ok(())
}
fn export(directory: &Path) -> Result<()> {
    fs::create_dir(directory)?;
    fs::write(directory.join("index.ts"), OPENCODE)?;
    println!("{}", directory.display());
    Ok(())
}
pub(crate) fn load(path: &Path) -> Result<Policy> {
    crate::absolute(path)?;
    let source = crate::shell::read_source(path)?;
    Ok(serde_json::from_str(&source)?)
}
pub(crate) fn input() -> Result<Value> {
    let mut text = String::new();
    std::io::stdin().take(1_048_577).read_to_string(&mut text)?;
    crate::bounded_source(&text)?;
    Ok(serde_json::from_str(&text)?)
}
fn evaluate(bridge: bool, compatibility: bool) -> Decision {
    match check(bridge, compatibility) {
        Ok((policy, scan, context)) => decide(policy, scan, context),
        Err(error) => deny("checker failure", &error.to_string(), "", true),
    }
}
fn check(bridge: bool, compatibility: bool) -> Result<(Policy, Scan, String)> {
    let value = input()?;
    let (policy, value, context) = setup(value, bridge, compatibility)?;
    policy.validate()?;
    let (command, cwd) = command(&value, bridge, &policy.base_dir)?;
    let scan = match command {
        Some(command) => bounded(policy.clone(), cwd, command)?,
        None => Scan {
            entry: None,
            unresolved: true,
        },
    };
    Ok((policy, scan, context))
}
fn setup(value: Value, bridge: bool, compatibility: bool) -> Result<(Policy, Value, String)> {
    let result = if compatibility {
        (
            serde_json::from_value(value["policy"].clone())?,
            value["input"].clone(),
            value["context"].as_str().unwrap_or("").to_owned(),
        )
    } else {
        let path = std::env::var_os(RULES_ENV)
            .ok_or("TOOL_RULE_HOOK_RULES is missing in hook execution process")?;
        let policy = load(Path::new(&path))?;
        policy.validate()?;
        let verified = verify(&value, bridge, policy.timeout_ms)?;
        let field = if bridge { "sessionID" } else { "session_id" };
        let context = format!(
            "calling_session={} rules_source={} {verified}",
            value[field].as_str().unwrap_or("unknown"),
            Path::new(&path).display()
        );
        (policy, value, context)
    };
    Ok(result)
}
fn verify(input: &Value, bridge: bool, timeout: u64) -> Result<String> {
    let verifier = std::env::var_os(VERIFY_ENV)
        .map(|value| value.into_string().map_err(|_| "non-UTF8 context verifier"))
        .transpose()?;
    let context = std::env::var_os(CONTEXT_ENV);
    match (verifier, context) {
        (None, None) => Ok(String::new()),
        (Some(verifier), Some(context)) => {
            crate::absolute(Path::new(&context))?;
            let command = verifier_command(&verifier)?;
            let response = execute(
                command,
                &json!({"bridge":bridge,"input":input}),
                timeout.min(1_000),
            )?;
            Ok(response["context"]
                .as_str()
                .ok_or("context verifier response missing")?
                .into())
        }
        _ => Err(
            "context verifier and context path must both be set; no fallback to standalone mode"
                .into(),
        ),
    }
}
fn verifier_command(value: &str) -> Result<Command> {
    let argv: Vec<String> = serde_json::from_str(value)?;
    let (program, args) = argv
        .split_first()
        .ok_or("context verifier command is empty")?;
    crate::absolute(Path::new(program))?;
    let mut command = Command::new(program);
    command.args(args);
    Ok(command)
}
pub fn execute(mut command: Command, input: &Value, timeout: u64) -> Result<Value> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or("checker stdin missing")?;
    let mut stdout = child.stdout.take().ok_or("checker stdout missing")?;
    let text = input.to_string();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = stdin.write_all(text.as_bytes());
    });
    std::thread::spawn(move || {
        let mut result = String::new();
        let read = stdout.by_ref().take(1_048_577).read_to_string(&mut result);
        let _ = sender.send(read.map(|_| result));
    });
    let budget = Duration::from_millis(timeout.clamp(1, 5_000));
    let start = Instant::now();
    wait(&mut child, start, budget)?;
    let output = receiver.recv_timeout(budget.saturating_sub(start.elapsed()))??;
    crate::bounded_source(&output)?;
    Ok(serde_json::from_str(&output)?)
}
fn wait(child: &mut std::process::Child, start: Instant, budget: Duration) -> Result<()> {
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                return Err("context/checker process failed".into());
            }
            break;
        }
        if start.elapsed() >= budget {
            let _ = child.kill();
            let _ = child.wait();
            return Err("context/checker process timed out".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}
fn command(input: &Value, bridge: bool, base: &Path) -> Result<(Option<String>, PathBuf)> {
    let Some(args) = tool_arguments(input, bridge)? else {
        return Ok((None, base.into()));
    };
    let command = args["command"]
        .as_str()
        .ok_or("native shell command missing")?
        .to_owned();
    let cwd = working_directory(input, args, bridge, base)?;
    let cwd = PathBuf::from(crate::normalize::path(Some(base), cwd)?);
    Ok((Some(command), cwd))
}
fn tool_arguments(input: &Value, bridge: bool) -> Result<Option<&Value>> {
    if !bridge && input["hook_event_name"] != "PreToolUse" {
        return Err("expected native PreToolUse input".into());
    }
    let field = if bridge { "tool" } else { "tool_name" };
    let tool = input[field].as_str().ok_or("native tool name missing")?;
    if tool != if bridge { "shell" } else { "Bash" } {
        return Ok(None);
    }
    let args = if bridge {
        &input["input"]
    } else {
        &input["tool_input"]
    };
    Ok(Some(args))
}
fn working_directory<'a>(
    input: &'a Value,
    args: &'a Value,
    bridge: bool,
    base: &'a Path,
) -> Result<&'a str> {
    let cwd = if bridge {
        args.get("workdir")
            .map(|value| value.as_str().ok_or("shell workdir must be text"))
            .transpose()?
            .unwrap_or(base.to_str().ok_or("non-UTF8 base_dir")?)
    } else {
        input["cwd"].as_str().ok_or("native cwd missing")?
    };
    Ok(cwd)
}
fn bounded(policy: Policy, cwd: PathBuf, command: String) -> Result<Scan> {
    let budget = Duration::from_millis(policy.timeout_ms);
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result = crate::scan(&policy, &cwd, &command).map_err(|error| error.to_string());
        let _ = sender.send(result);
    });
    receiver
        .recv_timeout(budget)
        .map_err(|_| "static checker timed out or failed")?
        .map_err(Into::into)
}
pub(crate) fn deny(entry: &str, detail: &str, context: &str, unresolved: bool) -> Decision {
    Decision {
        decision: "deny",
        unresolved,
        reason: Some(format!(
            "Tool rule hook: denied.\nMatched rule: {entry}\nContext: {context}\n{detail}"
        )),
    }
}
pub(crate) fn native_output(decision: &Decision) -> Value {
    if decision.decision == "deny" {
        json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":decision.reason}})
    } else {
        json!({})
    }
}
