use crate::{Result, config, coverage, inventory::Resolver, source};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn included(entry: &walkdir::DirEntry) -> bool {
    !matches!(
        entry.file_name().to_str(),
        Some(".git" | "target" | "__pycache__")
    )
}
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .to_path_buf()
}
pub fn cli(args: Vec<String>) -> Result<()> {
    let (cmd, tail) = args.split_first().ok_or("missing command")?;
    dispatch(cmd, tail)
}
fn dispatch(cmd: &str, tail: &[String]) -> Result<()> {
    match cmd {
        "--help" => {
            arity(tail, 0)?;
            println!(
                "crewmoor-quality: verify | lint-policy MANIFEST | crap MANIFEST COVERAGE | check MANIFEST"
            );
            Ok(())
        }
        "verify" => {
            arity(tail, 0)?;
            verify()
        }
        _ => dispatch_project(cmd, tail),
    }
}
fn arity(args: &[String], count: usize) -> Result<()> {
    if args.len() != count {
        return Err("invalid argument count".into());
    }
    Ok(())
}
fn dispatch_project(cmd: &str, tail: &[String]) -> Result<()> {
    match (cmd, tail) {
        ("lint-policy", [manifest]) => policy(Path::new(manifest)).map(|_| ()),
        ("crap", [manifest, report]) => crap(Path::new(manifest), Path::new(report)),
        ("check", [manifest]) => check(Path::new(manifest)),
        _ => Err("invalid command; see --help".into()),
    }
}
#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
}
#[derive(Deserialize)]
struct Package {
    id: String,
    source: Option<String>,
    manifest_path: PathBuf,
    targets: Vec<Target>,
}
#[derive(Deserialize)]
struct Target {
    src_path: PathBuf,
    kind: Vec<String>,
}
pub type Inventory = BTreeMap<PathBuf, bool>;
pub fn command(args: &[String]) -> Result<String> {
    let output = Command::new("cargo")
        .args(args)
        .current_dir(root())
        .output()?;
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        eprint!("{}", String::from_utf8_lossy(&output.stdout));
        return Err(format!("cargo {args:?} failed: {}", output.status).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}
fn metadata(manifest: &Path) -> Result<Metadata> {
    let args = vec![
        "metadata".into(),
        "--locked".into(),
        "--format-version=1".into(),
        "--manifest-path".into(),
        manifest.display().to_string(),
    ];
    Ok(serde_json::from_str(&command(&args)?)?)
}
pub fn inventory(manifest: &Path) -> Result<Inventory> {
    let meta = metadata(manifest)?;
    let mut resolver = Resolver::default();
    for package in meta.packages {
        if meta.workspace_members.contains(&package.id) || package.source.is_none() {
            add_targets(&mut resolver, package.targets)?;
        }
    }
    let files = resolver.finish();
    if files.is_empty() {
        return Err("empty Rust target inventory".into());
    }
    Ok(files)
}
fn add_targets(resolver: &mut Resolver, targets: Vec<Target>) -> Result<()> {
    for target in targets {
        resolver.target(&target.src_path, target.kind == ["test"])?;
    }
    Ok(())
}
pub fn policy(manifest: &Path) -> Result<Inventory> {
    let manifest = manifest.canonicalize()?;
    config::check(&root())?;
    config::check(manifest.parent().ok_or("missing manifest directory")?)?;
    let files = inventory(&manifest)?;
    for (path, test) in &files {
        source::analyze(&fs::read_to_string(path)?, *test)?;
    }
    Ok(files)
}
pub fn crap(manifest: &Path, report: &Path) -> Result<()> {
    let files = policy(manifest)?;
    let coverage = report_for(manifest, report, &files)?;
    let mut count = 0;
    for (path, test) in files {
        let functions = source::analyze(&fs::read_to_string(&path)?, test)?;
        count += coverage::check_file(&path, &functions, &coverage)?;
    }
    if count == 0 {
        return Err("no functions measured".into());
    }
    Ok(())
}
fn report_for(
    manifest: &Path,
    report: &Path,
    files: &Inventory,
) -> Result<BTreeMap<PathBuf, coverage::Lines>> {
    coverage::parse_scoped(
        &fs::read_to_string(report)?,
        &files.keys().cloned().collect::<Vec<_>>(),
        &excluded_sources(manifest)?,
    )
}
pub fn plan(manifest: &Path, report: &Path) -> Vec<Vec<String>> {
    let manifest = manifest.display().to_string();
    let common = vec![
        "--locked".into(),
        "--manifest-path".into(),
        manifest.clone(),
        "--workspace".into(),
        "--all-targets".into(),
        "--all-features".into(),
    ];
    let mut lint = vec!["clippy".into()];
    lint.extend(common.clone());
    lint.extend(["--".into(), "-D".into(), "warnings".into()]);
    let mut cov = vec![
        "llvm-cov".into(),
        "--no-rustc-wrapper".into(),
        "--no-default-ignore-filename-regex".into(),
        "--include-build-script".into(),
    ];
    cov.extend(common);
    cov.extend([
        "--json".into(),
        "--output-path".into(),
        report.display().to_string(),
    ]);
    vec![
        vec![
            "fmt".into(),
            "--manifest-path".into(),
            manifest,
            "--all".into(),
            "--check".into(),
        ],
        lint,
        cov,
    ]
}
pub fn execute(plan: &[Vec<String>], runner: impl Fn(&[String]) -> Result<String>) -> Result<()> {
    for args in plan {
        runner(args)?;
    }
    Ok(())
}
pub fn check(manifest: &Path) -> Result<()> {
    policy(manifest)?;
    let temp = tempfile::tempdir()?;
    let report = temp.path().join("coverage.json");
    execute(&plan(manifest, &report), command)?;
    crap(manifest, &report)
}
pub fn business(root: &Path) -> Result<()> {
    let manifest = root.join("Cargo.toml");
    if manifest.is_file() {
        return check(&manifest);
    }
    println!(
        "Business acceptance: N/A — no business Cargo.toml; quality tool and fixtures checked separately."
    );
    Ok(())
}
pub fn verify() -> Result<()> {
    verify_with(check)
}
pub fn verify_with(run: impl Fn(&Path) -> Result<()>) -> Result<()> {
    for manifest in ["tools/quality/Cargo.toml", "fixtures/quality/Cargo.toml"] {
        run(&root().join(manifest))?;
    }
    business(&root())
}

fn excluded_sources(manifest: &Path) -> Result<Vec<PathBuf>> {
    let meta = metadata(manifest)?;
    let mut roots = compiler_sources()?;
    for package in meta.packages {
        if package.source.is_some() {
            roots.push(
                package
                    .manifest_path
                    .parent()
                    .ok_or("dependency has no directory")?
                    .canonicalize()?,
            );
        }
    }
    Ok(roots)
}
fn compiler_sources() -> Result<Vec<PathBuf>> {
    let sysroot = rustc_output("--print=sysroot")?;
    let version = rustc_output("-vV")?;
    let hash = version
        .lines()
        .find_map(|line| line.strip_prefix("commit-hash: "))
        .ok_or("missing compiler commit")?;
    Ok(vec![
        PathBuf::from(sysroot.trim()).join("lib/rustlib/src/rust"),
        PathBuf::from("/rustc").join(hash).join("library"),
    ])
}
fn rustc_output(arg: &str) -> Result<String> {
    let output = Command::new("rustc").arg(arg).output()?;
    if !output.status.success() {
        return Err("rustc identity query failed".into());
    }
    Ok(String::from_utf8(output.stdout)?)
}
