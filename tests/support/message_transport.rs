// Synthetic Herdr CLI boundary: fields follow the installed 0.9.5 schema.
use crewmoor::Result;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

pub fn dispatch(args: &[String]) -> Option<Result<()>> {
    if args.get(1).map(String::as_str) != Some("agent") {
        return None;
    }
    Some(call(args))
}
fn call(args: &[String]) -> Result<()> {
    let socket = PathBuf::from(std::env::var_os("HERDR_SOCKET_PATH").ok_or("missing endpoint")?);
    let state: Value = serde_json::from_slice(&fs::read(socket.with_extension("json"))?)?;
    let mode = state["mode"].as_str().unwrap_or("");
    if mode == "native-error" {
        eprintln!("{{\"error\":{{\"code\":\"agent_blocked\",\"message\":\"blocked\"}}}}");
        return Err("synthetic rejection".into());
    }
    if mode == "bad-json" {
        println!("invalid");
        return Ok(());
    }
    respond(args, &socket, &state)
}
fn respond(args: &[String], socket: &std::path::Path, state: &Value) -> Result<()> {
    let handle = args.get(3).ok_or("missing target")?;
    let mut agent = state[handle].clone();
    let kind = match args.get(2).map(String::as_str) {
        Some("get") => "agent_info",
        Some("prompt") => {
            record_prompt(args, socket)?;
            if state["prompt_failure"] == true {
                return Err("synthetic outcome unknown".into());
            }
            if let Some(session) = state.get("prompt_session") {
                agent["agent_session"]["value"] = session.clone();
            }
            "agent_prompted"
        }
        _ => return Err("unsupported fixture command".into()),
    };
    println!(
        "{}",
        json!({"id":"fixture","result":{"type":kind,"agent":agent}})
    );
    Ok(())
}
fn record_prompt(args: &[String], socket: &std::path::Path) -> Result<()> {
    let names = [
        "HERDR_PANE_ID",
        "CODEX_SESSION_ID",
        "HC_SOCKET",
        "CREWMOOR_FEEDBACK_INSTANCE",
    ];
    let inherited: Vec<_> = names
        .iter()
        .filter(|name| std::env::var_os(name).is_some())
        .collect();
    fs::write(
        socket.with_extension("submitted"),
        serde_json::to_vec(&json!({"args":args,"inherited":inherited}))?,
    )?;
    Ok(())
}
