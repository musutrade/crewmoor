use crate::Result;
use toml::Value;
type Declaration = Option<(usize, Vec<Value>)>;

pub fn validate(args: &[String]) -> Result<()> {
    inject(args, "'tool-rule-hook' hook").map(|_| ())
}
pub fn inject(args: &[String], command: &str) -> Result<Vec<String>> {
    let mut result = args.to_vec();
    let (declaration, enabled) = existing(args)?;
    let quoted = serde_json::to_string(command)?;
    let group: Value = toml::from_str(&format!(
        "matcher='^Bash$'\nhooks=[{{type='command',command={},timeout=10}}]",
        quoted
    ))?;
    if let Some((index, mut groups)) = declaration {
        groups.push(group);
        result[index] = format!("hooks.PreToolUse={}", Value::Array(groups));
    } else {
        result.extend([
            "-c".into(),
            format!("hooks.PreToolUse={}", Value::Array(vec![group])),
        ]);
    }
    if !enabled {
        result.extend(["-c".into(), "features.hooks=true".into()]);
    }
    Ok(result)
}
fn existing(args: &[String]) -> Result<(Declaration, bool)> {
    let mut declaration = None;
    let mut enabled = false;
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if matches!(arg.as_str(), "-c" | "--config") {
            let value = args
                .get(index + 1)
                .ok_or("Codex config override missing value")?;
            inspect(value, index + 1, &mut declaration, &mut enabled)?;
            index += 2;
        } else {
            index += 1;
        }
    }
    Ok((declaration, enabled))
}
fn inspect(
    input: &str,
    index: usize,
    declaration: &mut Declaration,
    enabled: &mut bool,
) -> Result<()> {
    let Some((key, document)) = relevant_override(input)? else {
        return Ok(());
    };
    features(key, &document, enabled)?;
    let Some(hooks) = document.get("hooks") else {
        return Ok(());
    };
    if key == "hooks" {
        return Err("role hook cannot merge hooks parent table overrides".into());
    }
    let Some(groups) = hooks.get("PreToolUse") else {
        return Ok(());
    };
    if key != "hooks.PreToolUse" || declaration.is_some() {
        return Err("role hook needs at most one exact hooks.PreToolUse array override".into());
    }
    let groups = groups
        .as_array()
        .ok_or("hooks.PreToolUse override must be an array")?;
    validate_groups(groups)?;
    *declaration = Some((index, groups.clone()));
    Ok(())
}
fn relevant_override(input: &str) -> Result<Option<(&str, Value)>> {
    let (key, _) = input
        .split_once('=')
        .ok_or("Codex config override must be key=value")?;
    let key = key.trim();
    if !hook_related(key) {
        return Ok(None);
    }
    let document: Value = toml::from_str(input)?;
    Ok(Some((key, document)))
}
fn hook_related(key: &str) -> bool {
    let root = key
        .split('.')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(['\'', '"']);
    ["hooks", "features"].contains(&root)
}
fn features(key: &str, document: &Value, enabled: &mut bool) -> Result<()> {
    if document.get("features").is_some() {
        if document["features"].get("hooks").is_some() && key != "features.hooks" {
            return Err("ambiguous features.hooks override".into());
        }
        if key == "features" {
            return Err("role hook cannot merge a features parent table override".into());
        }
        if key == "features.hooks" {
            if *enabled || document["features"]["hooks"].as_bool() != Some(true) {
                return Err("role hook needs one unambiguous features.hooks=true override".into());
            }
            *enabled = true;
        }
    }
    Ok(())
}
fn validate_groups(groups: &[Value]) -> Result<()> {
    for group in groups {
        if !group.is_table() || !group.get("hooks").is_some_and(Value::is_array) {
            return Err("existing PreToolUse group must contain a hooks array".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_one_existing_cli_group_and_unrelated_overrides() {
        let args = [
            "-c",
            "model='example'",
            "--config",
            "hooks.PreToolUse=[{matcher='Read',hooks=[{type='command',command='existing'}]}]",
            "-c",
            "hooks.Stop=[]",
        ]
        .map(String::from);
        let raw = vec!["-c".into(), "model=gpt-6.1-sol".into()];
        let merged = inject(&raw, "fixed hook").unwrap();
        assert_eq!(&merged[..2], &raw);
        let result = inject(&args, "'/fixed/tool-rule-hook' hook").unwrap();
        assert_eq!(&result[..3], &args[..3]);
        assert_eq!(&result[4..6], &args[4..6]);
        let document: Value = toml::from_str(&result[3]).unwrap();
        let groups = document["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0]["hooks"][0]["command"].as_str(), Some("existing"));
        assert_eq!(
            groups[1]["hooks"][0]["command"].as_str(),
            Some("'/fixed/tool-rule-hook' hook")
        );
        assert_eq!(
            result
                .iter()
                .filter(|arg| arg.starts_with("hooks.PreToolUse="))
                .count(),
            1
        );
    }
    #[test]
    fn rejects_conflicting_or_unparseable_cli_declarations_before_start() {
        for input in [
            "hooks={PreToolUse=[]}",
            "hooks.PreToolUse={}",
            "hooks.PreToolUse.invalid=[]",
            "hooks.'PreToolUse'=[]",
            "features.hooks=false",
            "features.\"hooks\"=false",
            "features={hooks=true}",
            "hooks.PreToolUse=[{}]",
            "hooks.PreToolUse=[",
        ] {
            assert!(
                inject(&["-c".into(), input.into()], "fixed hook").is_err(),
                "{input}"
            );
        }
        assert!(
            inject(
                &[
                    "-c".into(),
                    "hooks.PreToolUse=[]".into(),
                    "-c".into(),
                    "hooks.PreToolUse=[]".into()
                ],
                "fixed hook"
            )
            .is_err()
        );
        assert!(inject(&["-c".into()], "fixed hook").is_err());
        let ordinary = inject(&[], "fixed hook").unwrap();
        assert!(!ordinary.contains(&"--no-daemon".into()));
        let explicit = inject(&["--no-daemon".into()], "fixed hook").unwrap();
        assert_eq!(
            explicit.iter().filter(|arg| *arg == "--no-daemon").count(),
            1
        );
    }
}
