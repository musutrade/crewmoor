use super::{INSTANCE_ENV, event, model};
use crate::{Result, config::AgentKind};
use std::{fs, io::Read, path::Path, time::Duration};

pub fn cli(args: &[String]) -> Result<()> {
    match args.split_first() {
        Some((cmd, rest)) if cmd == "hooks" => hooks(rest),
        _ => records(args),
    }
}
fn records(args: &[String]) -> Result<()> {
    let (command, rest) = args.split_first().ok_or("missing feedback command")?;
    match command.as_str() {
        "register" => register(rest),
        "hook" => receive_command(rest),
        "show" => show(rest),
        "report" => report(rest),
        "report-to" => report_to(rest),
        _ => Err("unknown feedback command".into()),
    }
}
fn report_to(args: &[String]) -> Result<()> {
    let [binding, session, source, kind, summary] = args else {
        return Err("usage: feedback report-to BINDING SESSION agent|skill TYPE SUMMARY".into());
    };
    let binding = serde_json::from_slice(&fs::read(binding)?)?;
    print(&super::report_to(
        &binding,
        session,
        source,
        kind,
        summary,
        &crate::message::Native,
    )?)
}
fn register(args: &[String]) -> Result<()> {
    match args {
        [registry, config] => {
            let registration = serde_json::from_slice(&fs::read(config)?)?;
            print(&model::register(Path::new(registry), registration)?)
        }
        _ => Err("usage: feedback register REGISTRY CONFIG".into()),
    }
}
fn receive_command(args: &[String]) -> Result<()> {
    let [agent, registry] = args else {
        return Err("usage: feedback hook AGENT REGISTRY".into());
    };
    hook(agent, Path::new(registry));
    Ok(())
}
fn show(args: &[String]) -> Result<()> {
    let [registry, instance] = args else {
        return Err("usage: feedback show REGISTRY INSTANCE".into());
    };
    print(&super::show(Path::new(registry), instance)?)
}
fn report(args: &[String]) -> Result<()> {
    match args {
        [registry, instance, session, source, kind, summary] => event::report(
            Path::new(registry),
            instance,
            session,
            source,
            kind,
            summary,
        ),
        _ => {
            Err("usage: feedback report REGISTRY INSTANCE SESSION agent|skill TYPE SUMMARY".into())
        }
    }
}
fn hooks(args: &[String]) -> Result<()> {
    let (command, rest) = args.split_first().ok_or("missing hooks command")?;
    match command.as_str() {
        "plan" => plan(rest),
        "apply" => apply(rest, false),
        "rollback" => apply(rest, true),
        _ => Err("unknown user hooks command".into()),
    }
}
fn plan(args: &[String]) -> Result<()> {
    match args {
        [agent, action, registry, binary, directory] => {
            let agent: AgentKind = serde_json::from_value(agent.as_str().into())?;
            print(&super::install::preview(
                agent,
                action,
                Path::new(registry),
                Path::new(binary),
                Path::new(directory),
            )?)
        }
        _ => Err(
            "usage: feedback hooks plan AGENT install|uninstall REGISTRY BINARY NEW_PLAN_DIR"
                .into(),
        ),
    }
}
fn apply(args: &[String], rollback: bool) -> Result<()> {
    let [directory] = args else {
        return Err("supply one user hooks plan directory".into());
    };
    print(&super::install::apply(Path::new(directory), rollback)?)
}
fn print(value: &impl serde::Serialize) -> Result<()> {
    let encoded = serde_json::to_string_pretty(value)?;
    println!("{encoded}");
    Ok(())
}
fn hook(agent: &str, registry: &Path) {
    // No CrewMoor marker: do not read stdin, look up sessions, or collect anything.
    if let Some(instance) = std::env::var_os(INSTANCE_ENV)
        && !receive(agent, registry, &instance.to_string_lossy())
    {
        eprintln!("CrewMoor feedback not recorded; task is not replayed");
    }
    // No decision, additional context, retry or task-completion signal to the Agent.
    println!("{{}}");
}
/// Forwarding must finish inside the Agents' 2 s Hook timeout, leaving room for startup.
const FORWARD_BUDGET: Duration = Duration::from_millis(1500);
fn receive(agent: &str, registry: &Path, instance: &str) -> bool {
    let transport = crate::message::Bounded::new(FORWARD_BUDGET);
    // An unknown or mismatched context never reads stdin.
    if super::associated(agent, registry, instance).is_err() {
        return false;
    }
    let mut bytes = Vec::new();
    let read = std::io::stdin().take(1_048_577).read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > 1_048_576 {
        return false;
    }
    super::process_hook_event(agent, registry, instance, &bytes, &transport)
}
