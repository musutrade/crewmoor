use crate::{Result, record::Record};

// Bash ANSI-C quoting keeps all bytes, including line breaks and control bytes,
// out of the terminal input stream. NUL cannot be an exec argument.
pub fn word(value: &str) -> Result<String> {
    if value.contains('\0') {
        return Err("NUL in launch argument".into());
    }
    let encoded: String = value.bytes().map(|byte| format!("\\x{byte:02x}")).collect();
    Ok(format!("$'{encoded}'"))
}
pub fn command(argv: &[String]) -> Result<String> {
    if argv.is_empty() {
        return Err("empty launch argv".into());
    }
    if !std::path::Path::new(&argv[0]).is_absolute() {
        return Err("startup executable must be absolute".into());
    }
    let words: Vec<String> = argv
        .iter()
        .map(|value| word(value))
        .collect::<Result<_>>()?;
    Ok(words.join(" "))
}
pub fn args(r: &Record) -> Result<Vec<String>> {
    let pane = r.handle.clone().ok_or("missing pane")?;
    let command = command(&r.config.argv())?;
    Ok(vec!["pane".into(), "run".into(), pane, command])
}
