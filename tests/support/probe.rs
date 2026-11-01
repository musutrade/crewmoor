pub fn dispatch(args: &[String]) -> Option<crewmoor::Result<()>> {
    if std::env::var_os("CREWMOOR_ARGV_EVIDENCE").is_some() {
        return Some(argv(args));
    }
    if args.iter().any(|arg| arg == "blank-ack") {
        return Some(Ok(()));
    }
    if args.iter().any(|arg| arg == "stall") {
        stall();
    }
    None
}
fn argv(args: &[String]) -> crewmoor::Result<()> {
    let kernel = std::fs::read("/proc/self/cmdline")?;
    let kernel = kernel.strip_suffix(b"\0").ok_or("invalid kernel argv")?;
    let kernel: Vec<String> = kernel
        .split(|byte| *byte == 0)
        .map(|arg| String::from_utf8(arg.to_vec()))
        .collect::<std::result::Result<_, _>>()?;
    let path = std::env::var_os("CREWMOOR_ARGV_EVIDENCE").ok_or("missing probe destination")?;
    let value = serde_json::json!({"argv":args,"kernel_argv":kernel});
    std::fs::write(path, serde_json::to_vec(&value)?)?;
    Ok(())
}
fn stall() -> ! {
    loop {
        std::thread::park();
    }
}
