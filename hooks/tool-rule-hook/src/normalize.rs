use crate::{Result, absolute};
use std::path::{Component, Path, PathBuf};

pub(crate) fn path(cwd: Option<&Path>, value: &str) -> Result<String> {
    let input = Path::new(value);
    let full = if input.is_absolute() {
        input.to_owned()
    } else {
        cwd.ok_or("relative entry requires a known working directory")?
            .join(input)
    };
    absolute(&full)?;
    let mut result = PathBuf::new();
    for component in full.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            component => result.push(component.as_os_str()),
        }
    }
    Ok(result.to_str().ok_or("entry path is not UTF-8")?.into())
}
pub(crate) fn argv(input: &[String], cwd: Option<&Path>) -> Result<Vec<String>> {
    let mut argv = input.to_vec();
    let Some(program) = argv.first_mut() else {
        return Ok(argv);
    };
    *program = Path::new(program)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.clone());
    match argv[0].as_str() {
        "cargo" => crate::cargo_global(&mut argv),
        "npm" => return npm(&argv, cwd),
        program if python(program) => return python_argv(&argv, cwd),
        _ => {}
    }
    Ok(argv)
}
pub(crate) fn python(program: &str) -> bool {
    program == "python"
        || program == "python3"
        || program
            .strip_prefix("python3.")
            .is_some_and(|minor| !minor.is_empty() && minor.bytes().all(|c| c.is_ascii_digit()))
}
/// Index of the source selector; unknown flags remain explicit unknowns.
pub(crate) fn python_index(args: &[String]) -> Result<Option<usize>> {
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            return Ok(Some(index + 1));
        }
        let Some(width) = python_option_width(arg) else {
            return Ok(None);
        };
        if width == 0 {
            return Ok(Some(index));
        }
        if width == 2 {
            args.get(index + 1)
                .ok_or("Python interpreter option operand missing")?;
        }
        index += width;
    }
    Ok(Some(index))
}
fn python_option_width(arg: &str) -> Option<usize> {
    if [
        "-u", "-B", "-I", "-E", "-s", "-S", "-O", "-OO", "-q", "-b", "-bb", "-v", "-d",
    ]
    .contains(&arg)
    {
        return Some(1);
    }
    if ["-W", "-X"].contains(&arg) {
        return Some(2);
    }
    if arg.starts_with("-W") || arg.starts_with("-X") {
        return Some(1);
    }
    if ["-", "-m"].contains(&arg) || arg.starts_with("-c") || !arg.starts_with('-') {
        return Some(0);
    }
    None
}
fn python_argv(argv: &[String], cwd: Option<&Path>) -> Result<Vec<String>> {
    let Some(index) = python_index(&argv[1..])? else {
        return Ok(argv.to_vec());
    };
    let args = &argv[index + 1..];
    let mut result = vec![argv[0].clone()];
    result.extend_from_slice(args);
    let after_separator = index > 0 && argv[index] == "--";
    if let Some(script) = result
        .get_mut(1)
        .filter(|script| script.as_str() != "-" && (after_separator || !script.starts_with('-')))
    {
        *script = path(cwd, script)?;
    }
    Ok(result)
}
pub(crate) fn python_script(argv: &[String]) -> bool {
    argv.first().is_some_and(|program| python(program))
        && argv.get(1).is_some_and(|script| !script.starts_with('-'))
}
fn npm_prefix(argv: &[String]) -> Result<(Vec<String>, Option<&str>)> {
    let mut args = Vec::new();
    let mut project = None;
    let mut index = 1;
    while let Some(arg) = argv.get(index) {
        if arg == "--" {
            args.extend_from_slice(&argv[index..]);
            break;
        }
        if arg == "--prefix" {
            project = Some(
                argv.get(index + 1)
                    .ok_or("npm --prefix directory missing")?
                    .as_str(),
            );
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--prefix=") {
            if value.is_empty() {
                return Err("npm --prefix directory missing".into());
            }
            project = Some(value);
            index += 1;
        } else {
            args.push(arg.clone());
            index += 1;
        }
    }
    if project == Some("") {
        return Err("npm --prefix directory missing".into());
    }
    Ok((args, project))
}
fn npm_task(args: &[String]) -> Option<(&str, &[String])> {
    let (task, remaining) = match args.first().map(String::as_str) {
        Some("test") => ("test", &args[1..]),
        Some("run") if args.get(1).is_some_and(|task| !task.starts_with('-')) => {
            (args[1].as_str(), &args[2..])
        }
        _ => return None,
    };
    if !remaining.is_empty() && remaining[0] != "--" {
        return None;
    }
    Some((task, remaining))
}
fn npm(argv: &[String], cwd: Option<&Path>) -> Result<Vec<String>> {
    let (args, project) = npm_prefix(argv)?;
    let Some((task, remaining)) = npm_task(&args) else {
        return Ok(argv.to_vec());
    };
    let project_path = path(cwd, project.unwrap_or("."))?;
    let mut result = vec![
        "npm".into(),
        "--prefix".into(),
        project_path,
        "run".into(),
        task.into(),
    ];
    if remaining.len() > 1 {
        result.extend_from_slice(remaining);
    }
    Ok(result)
}
