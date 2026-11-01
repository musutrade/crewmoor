use super::{Binding, Message, Native};
use crate::{Result, feedback::model::write_new};
use std::{fs, path::Path};

pub fn cli(args: &[String]) -> Result<()> {
    match args {
        [cmd, config, binding] if cmd == "bind" => bind(config, binding),
        [cmd, binding, message] if cmd == "send" => send(binding, message),
        _ => Err("usage: crewmoor message bind CONFIG NEW_BINDING | send BINDING MESSAGE".into()),
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
    print(&super::send(&binding, &message, &Native)?)
}
fn print(value: &impl serde::Serialize) -> Result<()> {
    let encoded = serde_json::to_string_pretty(value)?;
    println!("{encoded}");
    Ok(())
}
