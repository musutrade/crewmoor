fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = tool_rule_hook::cli(&args) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
