//! Protocol-only acknowledgement for adapter tests, never an actual checker.
pub fn dispatch(args: &[String]) -> Option<crewmoor::Result<()>> {
    match args.get(1).map(String::as_str) {
        Some("validate") => Some(Ok(())),
        Some("session-bind") => {
            println!("{{\"revision\":1}}");
            Some(Ok(()))
        }
        Some("compat-hook") => Some(compat(false)),
        Some("compat-bridge") => Some(compat(true)),
        _ => None,
    }
}

fn compat(bridge: bool) -> crewmoor::Result<()> {
    use std::io::Read;
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    if value["policy"].get("checker").is_some()
        || !value["policy"]["base_dir"]
            .as_str()
            .is_some_and(|path| std::path::Path::new(path).is_absolute())
    {
        return Err("legacy adapter did not translate rules".into());
    }
    if bridge {
        println!("{{\"decision\":\"no_match\",\"reason\":null,\"unresolved\":false}}");
    } else {
        println!("{{}}");
    }
    Ok(())
}
