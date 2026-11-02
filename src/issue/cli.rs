use super::{Assignment, DEFAULT_LABEL, Gh, gh};
use crate::Result;
use serde_json::Value;
use std::path::Path;

const USAGE: &str = "usage: crewmoor issue assign REGISTRY REPO NUMBER INSTANCE \
    | next REPO [--label LABEL] | current REGISTRY INSTANCE | done REGISTRY INSTANCE";

pub fn cli(args: &[String]) -> Result<()> {
    let encoded = serde_json::to_string_pretty(&run(args, &gh::Cli::default())?)?;
    println!("{encoded}");
    Ok(())
}
/// Parse and perform one issue operation, answering the JSON that `cli` prints.
pub fn run(args: &[String], gh: &impl Gh) -> Result<Value> {
    let (command, rest) = args.split_first().ok_or(USAGE)?;
    match command.as_str() {
        "assign" => assign(rest),
        "next" => next(rest, gh),
        "current" => current(rest),
        "done" => done(rest, gh),
        _ => Err(USAGE.into()),
    }
}
fn assign(args: &[String]) -> Result<Value> {
    let [registry, repo, number, instance] = args else {
        return Err(USAGE.into());
    };
    let assignment = Assignment {
        repo: repo.clone(),
        number: number.parse().map_err(|_| "invalid issue number")?,
    };
    super::assign(Path::new(registry), instance, &assignment)?;
    Ok(serde_json::to_value(assignment)?)
}
fn next(args: &[String], gh: &impl Gh) -> Result<Value> {
    let (repo, label) = match args {
        [repo] => (repo, DEFAULT_LABEL),
        [repo, flag, label] if flag == "--label" => (repo, label.as_str()),
        _ => return Err(USAGE.into()),
    };
    Ok(serde_json::to_value(super::next(repo, label, gh)?)?)
}
fn current(args: &[String]) -> Result<Value> {
    let [registry, instance] = args else {
        return Err(USAGE.into());
    };
    Ok(serde_json::to_value(super::current(
        Path::new(registry),
        instance,
    )?)?)
}
fn done(args: &[String], gh: &impl Gh) -> Result<Value> {
    let [registry, instance] = args else {
        return Err(USAGE.into());
    };
    Ok(serde_json::to_value(super::done(
        Path::new(registry),
        instance,
        gh,
    )?)?)
}
