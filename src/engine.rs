use crate::{
    Result,
    config::{Config, Runtime},
    core,
    native::{Native, Runner},
    process,
    record::Record,
    tui,
};
use std::{fs, path::Path};

pub fn launch(config: Config, root: &Path, runner: &impl Runner) -> Result<Record> {
    let mut record = Record::prepare(config, root)?;
    let _lock = record.lock()?;
    let result = start(&mut record, runner);
    preserve(&mut record, result)?;
    Ok(record)
}
fn start(record: &mut Record, runner: &impl Runner) -> Result<()> {
    record.phase = "server-request-intent".into();
    record.save()?;
    record.server = Some(runner.boot(record)?);
    record.save()?;
    crate::native::wait_socket(&record.socket())?;
    record.capture_socket()?;
    record.phase = "server-started".into();
    record.save()?;
    create(record, runner)?;
    inspect(record, runner)
}
fn create(r: &mut Record, runner: &impl Runner) -> Result<()> {
    match r.config.runtime {
        Runtime::Core => core::create(r, runner),
        Runtime::Tui => tui::create(r, runner),
    }
}
pub fn inspect(record: &mut Record, runner: &impl Runner) -> Result<()> {
    record.guard()?;
    match record.config.runtime {
        Runtime::Core => {
            let value = core::inspect_target(record, runner)?;
            core::observe(record, value)?;
        }
        Runtime::Tui => {
            let value = tui::target(record, runner)?;
            tui::observe(record, value, runner)?;
        }
    }
    record.phase = "observed".into();
    record.save()
}
pub fn stop(record: &mut Record, runner: &impl Runner) -> Result<()> {
    record.guard()?;
    record.phase = "stop-intent".into();
    record.save()?;
    match record.config.runtime {
        Runtime::Core => core::stop(record, runner)?,
        Runtime::Tui => tui::stop(record, runner)?,
    }
    confirm_exit(record)?;
    record.agent_process = None;
    record.ready = "not-running".into();
    record.phase = "stopped".into();
    record.save()?;
    record.release()
}
fn confirm_exit(record: &Record) -> Result<()> {
    let expected = record.server.as_ref().ok_or("missing server identity")?;
    for identity in [
        Some(expected),
        record.ending_process.as_ref(),
        record.agent_process.as_ref(),
        record.original_shell.as_ref(),
    ]
    .into_iter()
    .flatten()
    .chain(
        record
            .core_holders
            .iter()
            .flatten()
            .map(|holder| &holder.process),
    ) {
        process::wait_exited(identity)?;
    }
    socket_absent(record)
}
fn socket_absent(record: &Record) -> Result<()> {
    match fs::symlink_metadata(record.socket()) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => Err("native socket remains; binding retained".into()),
    }
}
pub fn preserve(record: &mut Record, result: Result<()>) -> Result<()> {
    if let Err(error) = result {
        record.phase = "unknown-outcome".into();
        record.ready = "unknown".into();
        record.save()?;
        return Err(error);
    }
    Ok(())
}
pub fn read_config(path: &Path) -> Result<Config> {
    Ok(toml::from_str(&fs::read_to_string(path)?)?)
}
pub fn launch_native(config: Config, root: &Path) -> Result<Record> {
    config.validate()?;
    crate::native::versions_config(&config)?;
    launch(config, root, &Native)
}
pub fn operate(command: &str, root: &Path) -> Result<Record> {
    let mut record = Record::load(root)?;
    let _lock = record.lock()?;
    let result = operation(command, &mut record);
    preserve(&mut record, result)?;
    Ok(record)
}
fn operation(command: &str, record: &mut Record) -> Result<()> {
    match command {
        "inspect" => inspect(record, &Native),
        "stop" => stop(record, &Native),
        _ => Err("unknown command".into()),
    }
}
pub fn cli(args: &[String]) -> Result<()> {
    match args {
        [cmd] => single(cmd),
        [cmd, config, root] if cmd == "launch" => print(&launch_native(
            read_config(Path::new(config))?,
            Path::new(root),
        )?),
        [cmd, root] => print(&operate(cmd, Path::new(root))?),
        _ => Err("usage: crewmoor launch CONFIG STATE | inspect STATE | stop STATE".into()),
    }
}
fn single(command: &str) -> Result<()> {
    if command != "--version" {
        return Err("unknown command".into());
    }
    println!("crewmoor {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
fn print(record: &Record) -> Result<()> {
    let encoded = serde_json::to_string_pretty(record)?;
    println!("{encoded}");
    Ok(())
}
pub fn verified_process(record: &Record) -> Result<()> {
    process::same(
        record
            .agent_process
            .as_ref()
            .ok_or("agent process unknown")?,
    )
}
