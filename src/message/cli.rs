use super::{Binding, Message, Native};
use crate::{Result, feedback::model::write_new};
use std::{fs, path::Path};

pub fn cli(args: &[String]) -> Result<()> {
    match args {
        [cmd, config, binding] if cmd == "bind" => bind(config, binding),
        [cmd, binding, message] if cmd == "send" => send(binding, message),
        [cmd, binding, dir] if cmd == "source" => source(binding, dir),
        _ => Err("usage: crewmoor message bind CONFIG NEW_BINDING | send BINDING MESSAGE | source BINDING TEAM_DIR".into()),
    }
}
fn bind(config: &str, binding: &str) -> Result<()> {
    let binding = Path::new(binding);
    crate::config::absolute(binding)?;
    let config = serde_json::from_slice(&fs::read(config)?)?;
    let result = Binding::bind(config, &Native)?;
    write_new(binding, &serde_json::to_vec_pretty(&result)?)?;
    print(&result)
}
fn send(binding: &str, message: &str) -> Result<()> {
    let binding: Binding = serde_json::from_slice(&fs::read(binding)?)?;
    let message: Message = serde_json::from_slice(&fs::read(message)?)?;
    let receipt = super::send(&binding, &message, &Native)?;
    print(&receipt)
}
fn source(path: &str, dir: &str) -> Result<()> {
    let path = Path::new(path);
    crate::config::absolute(path)?;
    let mut document: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
    let mut binding: Binding = serde_json::from_value(document.clone())?;
    binding.attach_source(Path::new(dir).join("team.json"))?;
    document["config"]["team_config"] = serde_json::to_value(&binding.config.team_config)?;
    let parent = path.parent().ok_or("binding parent missing")?;
    let file = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("invalid binding filename")?;
    crate::issue::replace(parent, file, &serde_json::to_vec_pretty(&document)?)?;
    print(&binding)
}
fn print(value: &impl serde::Serialize) -> Result<()> {
    let encoded = serde_json::to_string_pretty(value)?;
    println!("{encoded}");
    Ok(())
}
