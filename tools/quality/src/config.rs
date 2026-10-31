use crate::Result;
use std::{env, fs, path::Path};
use toml::Value;

pub fn environment(values: impl Iterator<Item = (String, String)>) -> Result<()> {
    for (key, value) in values {
        if !value.is_empty() && forbidden_environment(&key, &value) {
            return Err(format!("compiler environment override: {key}").into());
        }
    }
    Ok(())
}
fn forbidden_environment(key: &str, value: &str) -> bool {
    if key.ends_with("RUSTFLAGS") {
        return value.split([' ', '\u{1f}']).any(forbidden_flag);
    }
    override_key(key)
}
fn override_key(key: &str) -> bool {
    const KEYS: &[&str] = &[
        "RUSTFLAGS",
        "RUSTC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "CLIPPY_CONF_DIR",
        "CLIPPY_ARGS",
        "CARGO_BUILD_RUSTC",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
    ];
    KEYS.contains(&key) || key.ends_with("_RUSTFLAGS")
}
pub fn document(text: &str) -> Result<()> {
    walk(&toml::from_str::<Value>(text).map_err(|_| "invalid TOML configuration")?)
}
fn walk(value: &Value) -> Result<()> {
    match value {
        Value::Table(table) => {
            for (key, value) in table {
                entry(key, value)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                walk(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn entry(key: &str, value: &Value) -> Result<()> {
    match key {
        "rustflags" => flags(value)?,
        "rustc" | "rustc-wrapper" | "rustc-workspace-wrapper" => wrapper(key, value)?,
        "lints" => lint_values(value)?,
        _ => {}
    }
    walk(value)
}
fn wrapper(key: &str, value: &Value) -> Result<()> {
    if key == "rustc-wrapper" && value.as_str() == Some("sccache") {
        return Ok(());
    }
    Err("unknown compiler override".into())
}
fn lint_values(value: &Value) -> Result<()> {
    match value {
        Value::String(level) if matches!(level.as_str(), "allow" | "expect") => {
            return Err("Cargo lint suppression".into());
        }
        Value::Table(table) => {
            for value in table.values() {
                lint_values(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn flags(value: &Value) -> Result<()> {
    let array = value
        .as_array()
        .ok_or("rustflags must be an explicit array")?;
    for flag in array {
        let flag = flag.as_str().ok_or("invalid rustflag")?;
        if forbidden_flag(flag) {
            return Err("lint-altering rustflags".into());
        }
    }
    Ok(())
}
fn forbidden_flag(flag: &str) -> bool {
    [
        "-A",
        "-W",
        "--allow",
        "--warn",
        "--force-warn",
        "--cap-lints",
        "@",
    ]
    .iter()
    .any(|prefix| flag.starts_with(prefix))
}
pub fn workflow(text: &str) -> Result<()> {
    const BAD: &[&str] = &[
        "--allow",
        "--cap-lints",
        "--force-warn",
        "-A",
        "continue-on-error",
        "||",
        "set +e",
        "exit 0",
    ];
    if BAD.iter().any(|bad| text.contains(bad)) {
        return Err("CI suppression or failure swallowing".into());
    }
    Ok(())
}
pub fn check(root: &Path) -> Result<()> {
    external(root)?;
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(super::driver::included)
    {
        check_entry(&entry?.into_path())?;
    }
    Ok(())
}
fn external(root: &Path) -> Result<()> {
    environment(env::vars())?;
    for parent in root.ancestors() {
        cargo_config(parent)?;
    }
    if let Some(home) = env::var_os("HOME") {
        cargo_config(Path::new(&home))?;
    }
    if let Some(home) = env::var_os("CARGO_HOME") {
        custom_home(Path::new(&home))?;
    }
    Ok(())
}
pub fn custom_home(home: &Path) -> Result<()> {
    for name in ["config", "config.toml"] {
        let path = home.join(name);
        if path.is_file() {
            document(&fs::read_to_string(path)?)?;
        }
    }
    Ok(())
}
fn cargo_config(root: &Path) -> Result<()> {
    for name in [".cargo/config", ".cargo/config.toml"] {
        let path = root.join(name);
        if path.is_file() {
            document(&fs::read_to_string(path)?)?;
        }
    }
    Ok(())
}
fn check_entry(path: &Path) -> Result<()> {
    let name = path.file_name().and_then(|v| v.to_str());
    match name {
        Some("clippy.toml" | ".clippy.toml") => {
            return Err("Clippy configuration is forbidden".into());
        }
        Some("Cargo.toml") => document(&fs::read_to_string(path)?)?,
        Some(".cargo") => cargo_config(path.parent().ok_or("missing parent")?)?,
        _ => {}
    }
    check_workflow(path)
}
fn check_workflow(path: &Path) -> Result<()> {
    if path.is_file() && path.components().any(|c| c.as_os_str() == "workflows") {
        workflow(&fs::read_to_string(path)?)?;
    }
    Ok(())
}
