use crate::{Result, source::Function};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
pub struct Report {
    #[serde(rename = "type")]
    kind: String,
    data: Vec<Data>,
}
#[derive(Deserialize)]
struct Data {
    files: Vec<File>,
}
#[derive(Deserialize)]
struct File {
    filename: PathBuf,
    segments: Vec<Segment>,
}
// LLVM segment: line, column, execution count, has_count, region_entry, gap.
#[derive(Deserialize)]
struct Segment(u64, u64, u64, bool, bool, bool);
pub type Lines = BTreeMap<usize, bool>;

pub fn score(cc: u64, coverage: f64) -> Result<f64> {
    if cc == 0 || !coverage.is_finite() || !(0.0..=1.0).contains(&coverage) {
        return Err("invalid CC or coverage".into());
    }
    let value = (cc as f64).powi(2) * (1.0 - coverage).powi(3) + cc as f64;
    Ok(value)
}
pub fn enforce(cc: u64, coverage: f64) -> Result<f64> {
    let value = score(cc, coverage)?;
    if value > 10.0 {
        return Err(format!("CRAP {value} > 10").into());
    }
    Ok(value)
}
pub fn parse(text: &str) -> Result<BTreeMap<PathBuf, Lines>> {
    parse_for(text, &[])
}
pub fn parse_for(text: &str, wanted: &[PathBuf]) -> Result<BTreeMap<PathBuf, Lines>> {
    let report: Report = serde_json::from_str(text)?;
    if report.kind != "llvm.coverage.json.export" || report.data.is_empty() {
        return Err("missing LLVM coverage data".into());
    }
    let mut files = BTreeMap::new();
    for data in report.data {
        for file in data
            .files
            .into_iter()
            .filter(|f| wanted.is_empty() || wanted.contains(&f.filename))
        {
            merge_file(&mut files, file)?;
        }
    }
    if files.is_empty() {
        return Err("empty coverage file inventory".into());
    }
    Ok(files)
}
fn merge_file(files: &mut BTreeMap<PathBuf, Lines>, file: File) -> Result<()> {
    let path = file.filename.canonicalize()?;
    let lines = files.entry(path).or_default();
    for segment in file.segments {
        merge_segment(lines, segment)?;
    }
    Ok(())
}
fn merge_segment(lines: &mut Lines, s: Segment) -> Result<()> {
    if s.0 == 0 || s.1 == 0 {
        return Err("invalid LLVM source coordinate".into());
    }
    // Only has_count and non-gap segments contribute executable lines.
    let _region_entry = s.4;
    if s.3 && !s.5 {
        let count = lines.entry(usize::try_from(s.0)?).or_default();
        *count |= s.2 > 0;
    }
    Ok(())
}
pub fn fraction(lines: &Lines, function: &Function) -> Result<f64> {
    let values: Vec<_> = lines
        .range(function.start..=function.end)
        .map(|(_, v)| *v)
        .collect();
    if values.is_empty() {
        return Err(format!("missing coverage for {}", function.name).into());
    }
    Ok(values.iter().filter(|v| **v).count() as f64 / values.len() as f64)
}
pub fn check_file(
    path: &Path,
    functions: &[Function],
    files: &BTreeMap<PathBuf, Lines>,
) -> Result<usize> {
    if functions.is_empty() {
        return Ok(0);
    }
    let lines = files.get(path).ok_or("source file has no coverage")?;
    for function in functions {
        check_function(path, function, lines)?;
    }
    Ok(functions.len())
}
fn check_function(path: &Path, f: &Function, lines: &Lines) -> Result<()> {
    if f.unsupported {
        return Err(format!("unsupported macro in {}:{}", path.display(), f.start).into());
    }
    let cov = fraction(lines, f)?;
    let value = score(f.cc, cov)?;
    println!(
        "{}:{} {} CC={} coverage={cov:.6} CRAP={value:.6}",
        path.display(),
        f.start,
        f.name,
        f.cc
    );
    enforce(f.cc, cov)?;
    Ok(())
}
