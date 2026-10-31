use crate::{
    Result,
    config::Runtime,
    process::{self, Identity},
    record::Record,
};
use serde_json::Value;
use std::{
    fs,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

pub trait Runner {
    fn call(&self, record: &Record, args: &[String]) -> Result<Value>;
    fn boot(&self, record: &Record) -> Result<Identity>;
    fn absent(&self, record: &Record, args: &[String]) -> Result<()> {
        crate::failure::require_absent(self.call(record, args))
    }
}
pub struct Native;
pub fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).into()).collect()
}
pub fn command(record: &Record) -> Command {
    let mut cmd = Command::new(&record.config.runtime_binary);
    sanitize(&mut cmd);
    cmd.current_dir(&record.config.cwd);
    cmd.env("XDG_CONFIG_HOME", record.root.join("config"));
    cmd.env("XDG_STATE_HOME", record.root.join("state"));
    match record.config.runtime {
        Runtime::Tui => {
            cmd.args(["--session", &record.session]);
        }
        Runtime::Core => {
            cmd.env("HC_SESSION", &record.session)
                .env("HC_SOCKET", record.socket());
            cmd.arg("--json");
        }
    }
    cmd
}
fn sanitize(cmd: &mut Command) {
    for name in [
        "HERDR_SOCKET_PATH",
        "HERDR_SESSION",
        "HERDR_CONFIG_PATH",
        "HERDR_PANE_ID",
        "HERDR_TAB_ID",
        "HERDR_WORKSPACE_ID",
        "HC_SOCKET",
        "HC_SESSION",
        "HC_TERMINAL_ID",
        "HC_TERMINAL_SOCKET",
        "HC_SCOPE",
        "HC_RUNTIME",
        "HERDR_ENV",
        "CODEX_THREAD_ID",
        "CODEX_SESSION_ID",
        "CODEX_TOOL_CALL_ID",
        "CODEX_CI",
        "CODEX_VERSION",
        "CODEX_INTERNAL_ORIGINATOR_OVERRIDE",
        "CODEX_APPLY_PATCH_PRESERVE_LINE_ENDINGS",
        "CODEX_SANDBOX",
        "CODEX_SANDBOX_NETWORK_DISABLED",
        "CI",
        "CONTINUOUS_INTEGRATION",
        "NO_COLOR",
        "CLICOLOR",
        "CLICOLOR_FORCE",
        "FORCE_COLOR",
        "TERM",
        "TERM_PROGRAM",
        "TERM_PROGRAM_VERSION",
        "COLORTERM",
    ] {
        cmd.env_remove(name);
    }
}
pub fn version(binary: &std::path::Path) -> Result<String> {
    let mut cmd = Command::new(binary);
    sanitize(&mut cmd);
    let output = crate::transport::output(cmd.arg("--version"), Duration::from_secs(10))?;
    if !output.status.success() {
        return Err("version query failed".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
pub fn versions_config(config: &crate::config::Config) -> Result<()> {
    let actual = version(&config.runtime_binary)?;
    check_runtime_version(config, &actual)?;
    agent_version(config)
}
pub fn check_runtime_version(config: &crate::config::Config, actual: &str) -> Result<()> {
    if actual != config.runtime_version {
        return Err(format!("runtime version mismatch: {actual}").into());
    }
    Ok(())
}
fn agent_version(config: &crate::config::Config) -> Result<()> {
    if version(&config.agent_executable)? != config.agent_version {
        return Err("Agent version mismatch".into());
    }
    Ok(())
}
impl Runner for Native {
    fn call(&self, record: &Record, args: &[String]) -> Result<Value> {
        if record.config.app.is_some() {
            return crate::app::call(record, args);
        }
        let output = crate::transport::output(command(record).args(args), Duration::from_secs(10))?;
        if !output.status.success() {
            let value = serde_json::from_slice::<Value>(&output.stderr).unwrap_or(Value::Null);
            return Err(crate::failure::NativeFailure::decode(&value).into());
        }
        let value: Value = serde_json::from_slice(&output.stdout)?;
        payload(value, &record.config.runtime)
    }
    fn boot(&self, record: &Record) -> Result<Identity> {
        if record.config.app.is_some() {
            return crate::app::connect(record);
        }
        setup(record)?;
        boot_native(record)
    }
}
fn boot_native(record: &Record) -> Result<Identity> {
    match record.config.runtime {
        Runtime::Tui => tui_boot(record),
        Runtime::Core => core_boot(record),
    }
}
fn core_boot(record: &Record) -> Result<Identity> {
    Native.call(record, &strings(&["server", "start"]))?;
    core_identity(record)
}
fn core_identity(record: &Record) -> Result<Identity> {
    identity_from(&Native.call(record, &strings(&["server", "status"]))?)
}
pub fn identity_from(value: &Value) -> Result<Identity> {
    process::identity(pid(value, "/pid")?)
}
pub fn payload(value: Value, runtime: &Runtime) -> Result<Value> {
    if value.get("error").is_some() {
        return Err(crate::failure::NativeFailure::decode(&value).into());
    }
    match runtime {
        Runtime::Core => Ok(value),
        Runtime::Tui => Ok(value.get("result").cloned().unwrap_or(value)),
    }
}
fn setup(record: &Record) -> Result<()> {
    if record.socket().exists() {
        return Err("socket already exists; refusing instance reuse".into());
    }
    fs::create_dir_all(record.root.join("config/herdr-core"))?;
    fs::write(
        record.root.join("config/herdr-core/config.toml"),
        "notes = false\n",
    )?;
    fs::create_dir_all(record.root.join("config/herdr"))?;
    fs::write(
        record.root.join("config/herdr/config.toml"),
        "onboarding = false\n[update]\nversion_check = false\nmanifest_check = false\n",
    )?;
    Ok(())
}
fn tui_boot(record: &Record) -> Result<Identity> {
    let child = spawn_server(record)?;
    wait_socket(&record.socket())?;
    process::identity(child.id())
}
fn spawn_server(record: &Record) -> Result<std::process::Child> {
    Ok(server_command(record)?.spawn()?)
}
pub fn server_command(record: &Record) -> Result<Command> {
    use std::os::unix::process::CommandExt;
    let log = fs::File::create(record.root.join("server.log"))?;
    let mut cmd = command(record);
    cmd.arg("server")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .process_group(0);
    Ok(cmd)
}
pub fn wait_socket(path: &std::path::Path) -> Result<()> {
    for _ in 0..100 {
        if path.exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err("private server startup not confirmed; do not retry launch".into())
}
pub fn text(value: &Value, pointer: &str) -> Result<String> {
    Ok(value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing identity field {pointer}"))?
        .into())
}
pub fn pid(value: &Value, pointer: &str) -> Result<u32> {
    Ok(u32::try_from(
        value
            .pointer(pointer)
            .and_then(Value::as_u64)
            .ok_or("missing PID")?,
    )?)
}
pub fn optional_text<'a>(value: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => Ok(Some(value.as_str().ok_or("invalid optional text")?)),
    }
}
