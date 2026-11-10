//! Native plugin data selects exact Codex callers, independently of client env.
use crate::{
    Result,
    hook::{self, Decision},
    session::CodexBindings,
};
use std::path::PathBuf;

pub(crate) fn emit() -> Result<()> {
    let decision =
        check().unwrap_or_else(|error| hook::deny("checker failure", &error.to_string(), "", true));
    println!("{}", hook::native_output(&decision));
    Ok(())
}
fn check() -> Result<Decision> {
    let data = std::env::var_os("PLUGIN_DATA").ok_or(
        "native Codex PLUGIN_DATA is missing; this entry requires the installed Codex plugin",
    )?;
    let root = PathBuf::from(data);
    crate::absolute(&root)?;
    let bindings = CodexBindings::load(&root.join("tool-rule-hook"))?;
    if !bindings.is_empty() {
        let event = hook::input()?;
        if let Some(binding) = bindings.resolve(&event)? {
            return hook::check_binding(binding, event, false);
        }
    }
    Ok(Decision {
        decision: "no_match",
        reason: None,
        unresolved: false,
    })
}
