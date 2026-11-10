//! Standalone static command rules; host/session selection is explicit.
mod ansi;
mod assets;
mod codex;
mod hook;
mod normalize;
mod python;
mod session;
mod shell;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub use hook::{Decision, cli};

use serde::{Deserialize, Serialize};

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub base_dir: PathBuf,
    #[serde(default)]
    pub skip_source: Vec<Vec<String>>,
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub allow: Vec<Vec<String>>,
    pub feedback: Feedback,
    #[serde(default = "python_calls")]
    pub python_calls: Vec<String>,
    #[serde(default = "timeout")]
    pub timeout_ms: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub argv: Vec<String>,
    pub label: String,
}
fn python_calls() -> Vec<String> {
    [
        "subprocess.run",
        "subprocess.call",
        "subprocess.check_call",
        "subprocess.check_output",
        "subprocess.Popen",
        "os.system",
        "os.popen",
    ]
    .map(String::from)
    .into()
}
fn timeout() -> u64 {
    3_000
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        absolute(&self.base_dir)?;
        self.feedback.validate()?;
        if self.entries.is_empty() || !(1..=5_000).contains(&self.timeout_ms) {
            return Err("tool policy needs entries and timeout_ms in 1..5000".into());
        }
        for entry in &self.entries {
            validate_argv(&entry.argv)?;
            normalize::argv(&entry.argv, Some(&self.base_dir))?;
            nonblank(&entry.label)?;
        }
        self.validate_allow()
    }
    fn validate_allow(&self) -> Result<()> {
        self.validate_exception_entries()?;
        for argv in &self.allow {
            cargo_target(&normalize::argv(argv, Some(&self.base_dir))?)?;
        }
        self.validate_source_exceptions()?;
        for call in &self.python_calls {
            nonblank(call)?;
        }
        Ok(())
    }
    fn validate_exception_entries(&self) -> Result<()> {
        for argv in &self.allow {
            validate_argv(argv)?;
            let canonical = normalize::argv(argv, Some(&self.base_dir))?;
            if !self.entries.iter().any(|entry| {
                normalize::argv(&entry.argv, Some(&self.base_dir)).is_ok_and(|prefix| {
                    canonical.starts_with(&prefix) && canonical.len() > prefix.len()
                })
            }) {
                return Err(
                    "a targeted exception must extend a configured entry with exact arguments"
                        .into(),
                );
            }
        }
        Ok(())
    }
    fn validate_source_exceptions(&self) -> Result<()> {
        for argv in &self.skip_source {
            if !self.allow.contains(argv)
                || !normalize::python_script(&normalize::argv(argv, Some(&self.base_dir))?)
            {
                return Err(
                    "skip_source must name an exact allow entry for a Python script".into(),
                );
            }
        }
        Ok(())
    }
}
fn cargo_target(argv: &[String]) -> Result<()> {
    if !argv.starts_with(&["cargo".into(), "test".into()]) {
        return Ok(());
    }
    let arguments = &argv[2..];
    let end = arguments
        .iter()
        .position(|arg| arg == "--")
        .unwrap_or(arguments.len());
    if arguments[..end].iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--all" | "--workspace" | "--all-targets" | "--tests" | "--bins" | "--benches"
        )
    }) {
        return Err("cargo test exception cannot select a full test sweep".into());
    }
    let target = cargo_test_targets(arguments)?;
    let filter = arguments.first().is_some_and(|arg| concrete(arg));
    if !target && !filter {
        return Err(
            "cargo test exception needs a concrete --test target or a leading test filter".into(),
        );
    }
    Ok(())
}
fn cargo_test_targets(arguments: &[String]) -> Result<bool> {
    let mut args = arguments.iter();
    let mut selected = false;
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        let target = if arg == "--test" {
            Some(args.next().ok_or("Cargo --test target missing")?.as_str())
        } else {
            arg.strip_prefix("--test=")
        };
        if let Some(target) = target {
            if !concrete(target) {
                return Err("every Cargo --test target must be concrete".into());
            }
            selected = true;
        }
    }
    Ok(selected)
}
fn concrete(value: &str) -> bool {
    !value.is_empty() && !value.starts_with('-') && !value.contains(['*', '?', '[', '\0'])
}
fn nonblank(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.contains('\0') || value.len() > 16_384 {
        return Err(
            "tool policy strings must have 1..16384 UTF-8 bytes, be nonblank and contain no NUL"
                .into(),
        );
    }
    Ok(())
}
fn validate_argv(argv: &[String]) -> Result<()> {
    if argv.is_empty() {
        return Err("tool policy argv must not be empty".into());
    }
    for arg in argv {
        nonblank(arg)?;
    }
    Ok(())
}

#[derive(Debug, Default, Serialize)]
pub struct Scan {
    pub entry: Option<String>,
    pub unresolved: bool,
}
pub struct Scanner<'a> {
    policy: &'a Policy,
    cwd: PathBuf,
    scan: Scan,
    depth: usize,
    cwd_known: bool,
}
/// Synchronous analysis; the native CLI applies its finite timeout around the scan.
pub fn inspect(policy: &Policy, cwd: &Path, command: &str) -> Result<Scan> {
    policy.validate()?;
    scan(policy, cwd, command)
}
fn scan(policy: &Policy, cwd: &Path, command: &str) -> Result<Scan> {
    absolute(cwd)?;
    let mut scanner = Scanner {
        policy,
        cwd: cwd.into(),
        scan: Scan::default(),
        depth: 0,
        cwd_known: true,
    };
    scanner.shell(command)?;
    Ok(scanner.scan)
}
impl Scanner<'_> {
    fn argv(&mut self, argv: &[String], stdin: Option<&str>) -> Result<()> {
        if self.depth >= 16 {
            return Err("tool policy wrapper nesting exceeds 16".into());
        }
        self.depth += 1;
        let result = self.check_argv(argv, stdin);
        self.depth -= 1;
        result
    }
    fn check_argv(&mut self, argv: &[String], stdin: Option<&str>) -> Result<()> {
        let argv = normalize::argv(argv, self.cwd_known.then_some(self.cwd.as_path()))?;
        self.scan.unresolved |= argv.iter().any(|arg| arg.contains('\0'));
        if self.configured(&self.policy.allow, &argv)? {
            if self.configured(&self.policy.skip_source, &argv)? {
                return Ok(());
            }
            return self.wrapped(&argv, stdin);
        }
        for entry in &self.policy.entries {
            let prefix = normalize::argv(&entry.argv, Some(&self.policy.base_dir))?;
            if argv.starts_with(&prefix) {
                self.scan.entry.get_or_insert(entry.label.clone());
                return Ok(());
            }
        }
        self.wrapped(&argv, stdin)
    }
    fn configured(&self, entries: &[Vec<String>], argv: &[String]) -> Result<bool> {
        for entry in entries {
            if normalize::argv(entry, Some(&self.policy.base_dir))? == argv {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn wrapped(&mut self, argv: &[String], stdin: Option<&str>) -> Result<()> {
        let Some(program) = argv.first().map(String::as_str) else {
            return Ok(());
        };
        match program {
            "npm" => {
                self.scan.unresolved = true;
                Ok(())
            }
            "cd" => {
                self.cwd_known = false;
                self.scan.unresolved = true;
                Ok(())
            }
            "env" | "command" | "exec" | "nohup" => self.wrapper(program, &argv[1..], stdin),
            "sh" | "bash" | "dash" | "zsh" => self.shell_wrapper(&argv[1..], stdin),
            name if normalize::python(name) => self.python_entry(&argv[1..], stdin),
            _ => Ok(()),
        }
    }
    fn wrapper(&mut self, program: &str, args: &[String], stdin: Option<&str>) -> Result<()> {
        if program == "command"
            && args
                .first()
                .is_some_and(|arg| matches!(arg.as_str(), "-v" | "-V"))
        {
            return Ok(());
        }
        if let Some(args) = wrapper_args(program, args) {
            return self.argv(args, stdin);
        }
        self.scan.unresolved = true;
        Ok(())
    }
    fn shell_wrapper(&mut self, args: &[String], stdin: Option<&str>) -> Result<()> {
        match shell_source(args)? {
            ShellSource::Command(source) => self.shell(source),
            ShellSource::Stdin => self.shell(stdin.ok_or("shell stdin source unavailable")?),
            ShellSource::Unresolved => {
                self.scan.unresolved = true;
                Ok(())
            }
        }
    }
}
enum ShellSource<'a> {
    Command(&'a str),
    Stdin,
    Unresolved,
}
fn shell_source(args: &[String]) -> Result<ShellSource<'_>> {
    let Some((index, command, stdin)) = shell_options(args) else {
        return Ok(ShellSource::Unresolved);
    };
    if command {
        return Ok(ShellSource::Command(
            args.get(index).ok_or("shell -c source missing")?,
        ));
    }
    if stdin {
        return Ok(ShellSource::Stdin);
    }
    Ok(ShellSource::Unresolved)
}
fn shell_options(args: &[String]) -> Option<(usize, bool, bool)> {
    let (mut command, mut stdin) = (false, false);
    for (index, arg) in args.iter().enumerate() {
        match arg.as_str() {
            "--" | "-" => return Some((index + 1, command, stdin)),
            "-c" | "-lc" | "-ec" => command = true,
            "-s" => stdin = true,
            "-l" | "-e" => {}
            value if shell_script(value) => return Some((index, command, stdin)),
            _ => return None,
        }
    }
    Some((args.len(), command, stdin))
}
fn shell_script(arg: &str) -> bool {
    !arg.starts_with('-')
}
fn wrapper_args<'a>(program: &str, args: &'a [String]) -> Option<&'a [String]> {
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            return Some(&args[index + 1..]);
        }
        let width = wrapper_width(program, arg)?;
        if width == 0 {
            return Some(&args[index..]);
        }
        index += width;
        if index > args.len() {
            return None;
        }
    }
    Some(&args[index..])
}
fn wrapper_width(program: &str, arg: &str) -> Option<usize> {
    if program == "env" && arg.contains('=') && !arg.starts_with('-') {
        return Some(1);
    }
    if !arg.starts_with('-') {
        return Some(0);
    }
    match (program, arg) {
        ("env", "-i" | "--ignore-environment") | ("command", "-p") | ("exec", "-c" | "-l") => {
            Some(1)
        }
        ("env", "-u" | "--unset") | ("exec", "-a") => Some(2),
        _ => None,
    }
}
pub(crate) fn cargo_global(argv: &mut Vec<String>) {
    while let Some(arg) = argv.get(1) {
        let width = cargo_option(arg);
        if width == 0 || argv.len() <= width {
            break;
        }
        argv.drain(1..=width);
    }
}
fn cargo_option(arg: &str) -> usize {
    if arg.starts_with('+')
        || matches!(
            arg,
            "--locked" | "--offline" | "--frozen" | "--quiet" | "-q" | "--verbose" | "-v"
        )
    {
        return 1;
    }
    if matches!(
        arg,
        "--color" | "--config" | "--manifest-path" | "--target-dir"
    ) {
        return 2;
    }
    if ["--color=", "--config=", "--manifest-path=", "--target-dir="]
        .iter()
        .any(|prefix| arg.starts_with(prefix))
    {
        return 1;
    }
    0
}
pub(crate) fn bounded_source(source: &str) -> Result<()> {
    if source.len() > 1_048_576 {
        return Err("tool policy source exceeds 1 MiB".into());
    }
    Ok(())
}

pub fn absolute(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err("path must be absolute".into());
    }
    Ok(())
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Feedback {
    Text(String),
    Detail(FeedbackDetail),
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackDetail {
    pub reason: String,
    #[serde(default)]
    pub responsibility: Option<String>,
    #[serde(default)]
    pub next: Option<String>,
}
impl Feedback {
    fn validate(&self) -> Result<()> {
        match self {
            Self::Text(text) => nonblank(text),
            Self::Detail(detail) => {
                nonblank(&detail.reason)?;
                for value in [&detail.responsibility, &detail.next].into_iter().flatten() {
                    nonblank(value)?;
                }
                Ok(())
            }
        }
    }
    fn text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Detail(detail) => [
                &Some(detail.reason.clone()),
                &detail.responsibility,
                &detail.next,
            ]
            .into_iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n"),
        }
    }
}
