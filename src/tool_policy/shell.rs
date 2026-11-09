use super::{Scanner, bounded_source};
use crate::Result;
use std::{fs::File, io::Read, path::Path};
use tree_sitter::{Node, Parser};

impl Scanner<'_> {
    pub(super) fn shell(&mut self, source: &str) -> Result<()> {
        bounded_source(source)?;
        if self.depth >= 16 {
            return Err("tool policy wrapper nesting exceeds 16".into());
        }
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_bash::LANGUAGE.into())?;
        let tree = parser
            .parse(source, None)
            .ok_or("shell parsing did not complete")?;
        if tree.root_node().has_error() {
            return Err("unsupported or malformed shell syntax".into());
        }
        self.depth += 1;
        let result = self.walk(tree.root_node(), source);
        self.depth -= 1;
        result
    }
    fn walk(&mut self, root: Node<'_>, source: &str) -> Result<()> {
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            if node.kind() == "command" {
                self.command(node, source)?;
            }
            let mut cursor = node.walk();
            let children: Vec<_> = node.named_children(&mut cursor).collect();
            pending.extend(children.into_iter().rev());
        }
        Ok(())
    }
    fn command(&mut self, node: Node<'_>, source: &str) -> Result<()> {
        let mut argv = Vec::new();
        let name = node
            .child_by_field_name("name")
            .ok_or("shell command has no name")?;
        argv.push(self.word(name, source));
        let mut cursor = node.walk();
        for argument in node.children_by_field_name("argument", &mut cursor) {
            argv.push(self.word(argument, source));
        }
        let stdin = if needs_stdin(&argv) {
            stdin_source(node, source, &self.cwd)?
        } else {
            None
        };
        self.argv(&argv, stdin.as_deref())
    }
    fn word(&mut self, node: Node<'_>, source: &str) -> String {
        if let Some(word) = literal(node, source) {
            return word;
        }
        self.scan.unresolved = true;
        "\0dynamic".into()
    }
}
fn needs_stdin(mut argv: &[String]) -> bool {
    for _ in 0..16 {
        let Some(program) = argv.first() else {
            return false;
        };
        let name = std::path::Path::new(program)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if matches!(name, "env" | "command" | "exec" | "nohup") {
            let Some(next) = super::wrapper_args(name, &argv[1..]) else {
                return false;
            };
            argv = next;
        } else {
            return interpreter_stdin(name, &argv[1..]);
        }
    }
    false
}
fn interpreter_stdin(program: &str, args: &[String]) -> bool {
    match program {
        "sh" | "bash" | "dash" | "zsh" => args.first().is_some_and(|arg| arg == "-s"),
        name if name == "python" || name.starts_with("python3") => python_stdin(args),
        _ => false,
    }
}
fn python_stdin(args: &[String]) -> bool {
    let first = args
        .iter()
        .find(|arg| !matches!(arg.as_str(), "-u" | "-B" | "-I" | "-E" | "-s" | "-S"));
    first.is_none_or(|arg| arg == "-")
}
fn literal(node: Node<'_>, source: &str) -> Option<String> {
    let mut nodes = vec![node];
    while let Some(part) = nodes.pop() {
        if matches!(
            part.kind(),
            "simple_expansion"
                | "expansion"
                | "command_substitution"
                | "process_substitution"
                | "arithmetic_expansion"
        ) {
            return None;
        }
        let mut cursor = part.walk();
        nodes.extend(part.named_children(&mut cursor));
    }
    let words = shlex::split(node.utf8_text(source.as_bytes()).ok()?)?;
    if words.len() == 1 {
        words.into_iter().next()
    } else {
        None
    }
}
fn stdin_source(node: Node<'_>, source: &str, cwd: &Path) -> Result<Option<String>> {
    // Redirects live either on the command or its enclosing redirected_statement.
    let scope = node
        .parent()
        .filter(|p| p.kind() == "redirected_statement")
        .unwrap_or(node);
    let mut cursor = scope.walk();
    let mut content = None;
    for redirect in scope.children_by_field_name("redirect", &mut cursor) {
        if let Some(value) = redirect_source(redirect, source, cwd)? {
            if content.is_some() {
                return Err("multiple stdin redirects are unsupported".into());
            }
            content = Some(value);
        }
    }
    Ok(content)
}
fn redirect_source(node: Node<'_>, source: &str, cwd: &Path) -> Result<Option<String>> {
    let mut cursor = node.walk();
    match node.kind() {
        "heredoc_redirect" => {
            let body = node
                .named_children(&mut cursor)
                .find(|part| part.kind() == "heredoc_body")
                .ok_or("heredoc source missing")?;
            Ok(Some(body.utf8_text(source.as_bytes())?.into()))
        }
        "herestring_redirect" => {
            let value = node
                .named_children(&mut cursor)
                .last()
                .and_then(|part| literal(part, source))
                .ok_or("dynamic here-string source unavailable")?;
            Ok(Some(value))
        }
        "file_redirect" => file_stdin(node, source, cwd),
        _ => Ok(None),
    }
}
fn file_stdin(node: Node<'_>, source: &str, cwd: &Path) -> Result<Option<String>> {
    let text = node.utf8_text(source.as_bytes())?.trim_start();
    if !text.starts_with('<') {
        return Ok(None);
    }
    let destination = node
        .child_by_field_name("destination")
        .ok_or("stdin file missing")?;
    let path = literal(destination, source).ok_or("dynamic stdin file unavailable")?;
    Ok(Some(read_source(&cwd.join(path))?))
}
pub(super) fn read_source(path: &Path) -> Result<String> {
    let mut source = String::new();
    File::open(path)
        .map_err(|_| "script/stdin source is unreadable")?
        .take(1_048_577)
        .read_to_string(&mut source)
        .map_err(|_| "script source is not readable UTF-8")?;
    bounded_source(&source)?;
    Ok(source)
}
