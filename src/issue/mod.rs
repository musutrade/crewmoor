//! Explicit issue assignment for a feedback instance, read from GitHub through `gh`.
//! The dispatcher writes assignments itself; nothing here assigns or delivers automatically.
mod cli;
pub mod gh;
pub use cli::{cli, run};
pub use gh::Gh;

use crate::{Result, feedback::model};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// Assignment record kept in the feedback instance directory.
pub const ASSIGNMENT_FILE: &str = "issue-assignment.json";
pub const DEFAULT_LABEL: &str = "ready";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub repo: String,
    pub number: u64,
}
impl Assignment {
    fn validate(&self) -> Result<()> {
        repo(&self.repo)?;
        if self.number == 0 {
            return Err("invalid issue number".into());
        }
        Ok(())
    }
}
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub url: String,
}
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Done {
    pub closed: bool,
}

/// `OWNER/NAME` made of GitHub name characters; never read as a `gh` option.
pub fn repo(value: &str) -> Result<()> {
    let parts: Vec<&str> = value.split('/').collect();
    let valid = |part: &&str| {
        !part.is_empty()
            && !part.starts_with('-')
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if parts.len() != 2 || !parts.iter().all(valid) {
        return Err("invalid repository; expected OWNER/NAME".into());
    }
    Ok(())
}
/// One label; a comma would make `gh` split it into several required labels.
pub fn label(value: &str) -> Result<()> {
    model::identifier(value)?;
    if value.contains(',') {
        return Err("invalid label".into());
    }
    Ok(())
}

/// Replace the instance's assignment; assigning again overwrites the previous record.
pub fn assign(registry: &Path, instance: &str, assignment: &Assignment) -> Result<()> {
    assignment.validate()?;
    model::load(registry, instance)?;
    replace(
        &model::instance_dir(registry, instance)?,
        &serde_json::to_vec(assignment)?,
    )
}
/// Stage a private file beside the record, then rename it over the record atomically.
fn replace(directory: &Path, bytes: &[u8]) -> Result<()> {
    let staged = directory.join(format!(
        ".{ASSIGNMENT_FILE}.{}",
        uuid::Uuid::new_v4().simple()
    ));
    model::write_new(&staged, bytes)?;
    let renamed = fs::rename(&staged, directory.join(ASSIGNMENT_FILE));
    if renamed.is_err() {
        let _ = fs::remove_file(&staged);
    }
    Ok(renamed?)
}
/// The instance's current assignment; no record is an error, never a guess.
pub fn current(registry: &Path, instance: &str) -> Result<Assignment> {
    let path = model::instance_dir(registry, instance)?.join(ASSIGNMENT_FILE);
    let bytes = match fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err("no issue assigned to this instance".into());
        }
        read => read?,
    };
    let assignment: Assignment = serde_json::from_slice(&bytes)?;
    assignment.validate()?;
    Ok(assignment)
}
/// Whether GitHub reports the assigned issue closed.
pub fn done(registry: &Path, instance: &str, gh: &impl Gh) -> Result<Done> {
    let assignment = current(registry, instance)?;
    let value = gh.call(&[
        "issue".into(),
        "view".into(),
        assignment.number.to_string(),
        format!("--repo={}", assignment.repo),
        "--json=state".into(),
    ])?;
    match value["state"].as_str() {
        Some("CLOSED") => Ok(Done { closed: true }),
        Some("OPEN") => Ok(Done { closed: false }),
        _ => Err("unexpected issue state from gh".into()),
    }
}
/// The oldest open issue carrying `label`.
pub fn next(repository: &str, wanted: &str, gh: &impl Gh) -> Result<Issue> {
    repo(repository)?;
    label(wanted)?;
    let value = gh.call(&[
        "issue".into(),
        "list".into(),
        format!("--repo={repository}"),
        "--state=open".into(),
        format!("--label={wanted}"),
        "--search=sort:created-asc".into(),
        "--limit=1".into(),
        "--json=number,title,body,url".into(),
    ])?;
    let issues: Vec<Issue> = serde_json::from_value(value)?;
    issues
        .into_iter()
        .next()
        .ok_or_else(|| format!("no open issue labelled {wanted}").into())
}
