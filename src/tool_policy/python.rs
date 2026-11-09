use super::{Scanner, bounded_source, shell::read_source};
use crate::Result;
use rustpython_ast::{self as ast, Visitor};
use rustpython_parser::Parse;

enum Invocation {
    Shell(String),
    Argv(Vec<String>),
    Dynamic,
}
struct Calls<'a> {
    names: &'a [String],
    calls: Vec<Invocation>,
}
impl Visitor for Calls<'_> {
    fn visit_expr_call(&mut self, call: ast::ExprCall) {
        if dotted(&call.func).is_some_and(|name| self.names.contains(&name)) {
            let argument = call.args.first().or_else(|| {
                call.keywords
                    .iter()
                    .find(|kw| kw.arg.as_deref() == Some("args"))
                    .map(|kw| &kw.value)
            });
            self.calls.push(invocation(argument));
        }
        self.generic_visit_expr_call(call);
    }
}
fn dotted(expr: &ast::Expr) -> Option<String> {
    let mut parts = Vec::new();
    let mut current = expr;
    loop {
        match current {
            ast::Expr::Name(name) => {
                parts.push(name.id.to_string());
                break;
            }
            ast::Expr::Attribute(attr) => {
                parts.push(attr.attr.to_string());
                current = &attr.value;
            }
            _ => return None,
        }
    }
    parts.reverse();
    Some(parts.join("."))
}
fn string(expr: &ast::Expr) -> Option<String> {
    match expr {
        ast::Expr::Constant(constant) => match &constant.value {
            ast::Constant::Str(value) => Some(value.clone()),
            _ => None,
        },
        _ => None,
    }
}
fn invocation(expr: Option<&ast::Expr>) -> Invocation {
    let Some(expr) = expr else {
        return Invocation::Dynamic;
    };
    if let Some(value) = string(expr) {
        return Invocation::Shell(value);
    }
    let items = match expr {
        ast::Expr::List(list) => &list.elts,
        ast::Expr::Tuple(tuple) => &tuple.elts,
        _ => return Invocation::Dynamic,
    };
    Invocation::Argv(
        items
            .iter()
            .map(|expr| string(expr).unwrap_or_else(|| "\0dynamic".into()))
            .collect(),
    )
}
impl Scanner<'_> {
    pub(super) fn python_entry(&mut self, args: &[String], stdin: Option<&str>) -> Result<()> {
        // Only documented interpreter flags without source operands are skipped.
        let args = args.strip_prefix(&["--".into()]).unwrap_or(args);
        let index = args
            .iter()
            .position(|arg| !matches!(arg.as_str(), "-u" | "-B" | "-I" | "-E" | "-s" | "-S"))
            .unwrap_or(args.len());
        let args = &args[index..];
        if let Some(source) = args
            .first()
            .and_then(|arg| arg.strip_prefix("-c"))
            .filter(|source| !source.is_empty())
        {
            return self.python(source);
        }
        match args.first().map(String::as_str) {
            Some("-c") => self.python(args.get(1).ok_or("Python -c source missing")?),
            None | Some("-") => self.python(stdin.ok_or("Python stdin source unavailable")?),
            Some(path) if !path.starts_with('-') => self.python_file(path),
            _ => {
                self.scan.unresolved = true;
                Ok(())
            }
        }
    }
    fn python_file(&mut self, path: &str) -> Result<()> {
        if !self.cwd_known && std::path::Path::new(path).is_relative() {
            return Err(
                "relative Python file after shell cd is unresolved; use the tool working directory"
                    .into(),
            );
        }
        self.python(&read_source(&self.cwd.join(path))?)
    }
    fn python(&mut self, source: &str) -> Result<()> {
        bounded_source(source)?;
        if self.depth >= 16 {
            return Err("tool policy wrapper nesting exceeds 16".into());
        }
        let suite = ast::Suite::parse(source, "<tool-policy>")
            .map_err(|_| "Python source syntax is invalid or unsupported")?;
        let mut calls = Calls {
            names: &self.policy.python_calls,
            calls: Vec::new(),
        };
        for statement in suite {
            calls.visit_stmt(statement);
        }
        self.depth += 1;
        let result = self.python_calls(calls.calls);
        self.depth -= 1;
        result
    }
    fn python_calls(&mut self, calls: Vec<Invocation>) -> Result<()> {
        for call in calls {
            match call {
                Invocation::Shell(command) => self.shell(&command)?,
                Invocation::Argv(argv) => self.argv(&argv, None)?,
                Invocation::Dynamic => self.scan.unresolved = true,
            }
        }
        Ok(())
    }
}
