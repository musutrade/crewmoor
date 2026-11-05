//! A foreground TUI parent holds the native stdio lease; never a service manager.
use super::{api::Api, endpoint::Endpoint, prepare};
use crate::{
    Result,
    config::Config,
    feedback::{self, model},
    process,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader},
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Stdio},
    sync::mpsc,
    time::Duration,
};

pub fn run(args: &[String]) -> Result<()> {
    let launch = prepare::read()?;
    let config = &launch.config;
    if args != prepare::arguments(config) && args != prepare::tui_arguments(config) {
        return Err("OpenCode launch arguments differ from owned config".into());
    }
    let context = launch_context(config)?;
    let password = uuid::Uuid::new_v4().simple().to_string();
    let mut server = start_server(&launch, &password)?;
    let result = interact(&launch, &context, &password, &mut server);
    finish(&launch.root, result, server)
}
fn finish(root: &Path, result: Result<()>, mut server: Child) -> Result<()> {
    drop(server.stdin.take());
    let ended = server.wait();
    save_exit(root, &result, &ended)?;
    result?;
    if !ended?.success() {
        return Err("private OpenCode service exited unsuccessfully".into());
    }
    Ok(())
}
fn launch_context(config: &Config) -> Result<feedback::Context> {
    let registry = config
        .feedback_registry
        .as_ref()
        .ok_or("missing feedback_registry")?;
    let instance = config
        .feedback_instance
        .as_deref()
        .ok_or("missing feedback instance")?;
    model::load_for_agent(registry, instance, &crate::config::AgentKind::OpenCode)
}

fn start_server(launch: &prepare::Launch, password: &str) -> Result<Child> {
    let mut command = launch.command()?;
    command
        .args(["serve", "--stdio", "--port", "0", "--hostname", "127.0.0.1"])
        .current_dir(&launch.config.cwd)
        .env("OPENCODE_PASSWORD", password)
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(fs::File::create(
            launch.root.join("opencode-server.stderr"),
        )?);
    for key in crate::app::CHILD_MARKERS {
        command.env_remove(key);
    }
    if let Some(instance) = &launch.config.feedback_instance {
        command.env(crate::feedback::INSTANCE_ENV, instance);
    }
    for key in [
        "HC_SOCKET",
        "HC_SESSION",
        "HC_TERMINAL_ID",
        "HC_TERMINAL_SOCKET",
        "HC_SCOPE",
        "HC_RUNTIME",
    ] {
        command.env_remove(key);
    }
    command.env_remove(prepare::LAUNCH_ENV);
    Ok(command.spawn()?)
}

fn ready(server: &mut Child) -> Result<String> {
    let stdout = server
        .stdout
        .take()
        .ok_or("missing native stdio readiness")?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = tx.send(reader.read_line(&mut line).map(|_| line));
        let _ = std::io::copy(&mut reader, &mut std::io::sink());
    });
    let value: Value = serde_json::from_str(&rx.recv_timeout(Duration::from_secs(20))??)?;
    crate::native::text(&value, "/url")
}

struct Connected {
    url: String,
    password: String,
    info: Value,
    session: String,
}

fn connect(
    config: &Config,
    context: &feedback::Context,
    password: &str,
    server: &mut Child,
) -> Result<Connected> {
    let url = ready(server)?;
    let api = Api::new(&url, password, Duration::from_secs(10))?;
    let info = api.get("/api/info")?;
    check_server(config, server, &info)?;
    let session = create_session(config, context, &api)?;
    super::events::start(config, context, &url, password)?;
    Ok(Connected {
        url,
        password: password.into(),
        info,
        session,
    })
}
fn create_session(config: &Config, context: &feedback::Context, api: &Api) -> Result<String> {
    let options = config.opencode.clone().unwrap_or_default();
    let made = api.post(
        "/api/session",
        &options.session_payload(&config.cwd, &config.role),
    )?;
    let session = crate::native::text(&made, "/data/id")?;
    let fact = api.session(&session)?;
    options.check_session(&fact)?;
    super::events::enroll(config, context, &made["data"])?;
    Ok(session)
}
fn interact(
    launch: &prepare::Launch,
    context: &feedback::Context,
    password: &str,
    server: &mut Child,
) -> Result<()> {
    let config = &launch.config;
    let connected = connect(config, context, password, server)?;
    let mut tui = start_tui(launch, &connected)?;
    let recorded = publish(config, context, &connected, server, &tui);
    if recorded.is_err() {
        let _ = tui.kill();
    }
    let ended = tui.wait();
    recorded?;
    if !ended?.success() {
        return Err("OpenCode TUI exited unsuccessfully".into());
    }
    Ok(())
}
fn publish(
    config: &Config,
    context: &feedback::Context,
    connected: &Connected,
    server: &Child,
    tui: &Child,
) -> Result<()> {
    let endpoint = endpoint(context, connected, server, tui)?;
    endpoint.save(
        config
            .feedback_registry
            .as_ref()
            .ok_or("missing registry")?,
    )
}

fn check_server(config: &Config, server: &Child, info: &Value) -> Result<()> {
    let expected = config
        .agent_version
        .trim_start_matches("opencode ")
        .trim_start_matches('v');
    if info["version"] != expected || info["pid"].as_u64() != Some(u64::from(server.id())) {
        return Err("private native server returned a different version/PID".into());
    }
    process::agent_identity(server.id(), &config.agent_executable, &config.cwd)?;
    Ok(())
}

fn start_tui(launch: &prepare::Launch, connected: &Connected) -> Result<Child> {
    let config = &launch.config;
    let mut command = launch.command()?;
    command
        .args(["--server", &connected.url, "--session", &connected.session])
        .args(prepare::arguments(config))
        .env("OPENCODE_PASSWORD", &connected.password)
        .env_remove(prepare::LAUNCH_ENV)
        .current_dir(&config.cwd);
    Ok(command.spawn()?)
}

fn endpoint(
    context: &feedback::Context,
    connected: &Connected,
    server: &Child,
    tui: &Child,
) -> Result<Endpoint> {
    Ok(Endpoint {
        instance: context.instance.clone(),
        session_id: connected.session.clone(),
        url: connected.url.clone(),
        password: connected.password.clone(),
        version: crate::native::text(&connected.info, "/version")?,
        launcher: process::identity(std::process::id())?,
        server: process::identity(server.id())?,
        tui: process::identity(tui.id())?,
    })
}

fn save_exit(
    root: &Path,
    result: &Result<()>,
    ended: &std::io::Result<std::process::ExitStatus>,
) -> Result<()> {
    let value = json!({"tui_result":result.as_ref().map_err(|e|e.to_string()),"native_lease_closed":true,"server_exit":ended.as_ref().map(|s|s.code()).map_err(|e|e.to_string())});
    model::write_new(
        &root.join("opencode-exit.json"),
        &serde_json::to_vec(&value)?,
    )
}
