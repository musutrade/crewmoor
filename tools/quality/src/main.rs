use quote::ToTokens;
use std::{env, fs, process};
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
};

#[derive(Default)]
struct Complexity {
    decisions: u64,
    unsupported: bool,
}
impl<'ast> Visit<'ast> for Complexity {
    fn visit_expr(&mut self, node: &'ast syn::Expr) {
        self.decisions += match node {
            syn::Expr::If(_)
            | syn::Expr::While(_)
            | syn::Expr::ForLoop(_)
            | syn::Expr::Loop(_)
            | syn::Expr::Try(_) => 1,
            syn::Expr::Binary(b) if matches!(b.op, syn::BinOp::And(_) | syn::BinOp::Or(_)) => 1,
            syn::Expr::Match(m) => {
                m.arms.len().saturating_sub(1) as u64
                    + m.arms.iter().filter(|a| a.guard.is_some()).count() as u64
            }
            _ => 0,
        };
        if matches!(node, syn::Expr::Closure(_)) {
            self.unsupported = true;
        }
        visit::visit_expr(self, node);
    }
    fn visit_pat_or(&mut self, p: &'ast syn::PatOr) {
        self.decisions += p.cases.len().saturating_sub(1) as u64;
        visit::visit_pat_or(self, p);
    }
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        // Macro arguments are token trees, not visited Rust expressions.
        let tokens = m.tokens.to_string();
        let ids = identifiers(m.tokens.clone());
        if ids
            .iter()
            .any(|i| ["if", "match", "while", "for", "loop", "fn"].contains(&i.as_str()))
            || tokens.contains('?')
            || tokens.contains("&&")
            || tokens.contains("||")
        {
            self.unsupported = true;
        }
        // Standard assertion/formatting macros are treated as opaque calls.
        const CALLS: &[&str] = &[
            "assert",
            "assert_eq",
            "assert_ne",
            "debug_assert",
            "debug_assert_eq",
            "debug_assert_ne",
            "format",
            "format_args",
            "println",
            "eprintln",
            "vec",
            "panic",
            "unreachable",
            "todo",
        ];
        if !m
            .path
            .get_ident()
            .is_some_and(|i| CALLS.iter().any(|name| i == name))
        {
            self.unsupported = true;
        }
    }
    // Nested named functions have their own independent score.
    fn visit_item_fn(&mut self, _: &'ast syn::ItemFn) {}
}
#[derive(Default)]
struct Functions {
    rows: Vec<serde_json::Value>,
}
impl Functions {
    fn record(&mut self, name: &syn::Ident, block: &syn::Block, start: usize) {
        let mut cc = Complexity::default();
        cc.visit_block(block);
        self.rows.push(serde_json::json!({
            "name": name.to_string(), "start": start, "end": block.span().end().line,
            "cc": cc.decisions + 1, "unsupported": cc.unsupported
        }));
    }
}
impl<'ast> Visit<'ast> for Functions {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.record(&f.sig.ident, &f.block, f.sig.span().start().line);
        visit::visit_item_fn(self, f);
    }
    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.record(&f.sig.ident, &f.block, f.sig.span().start().line);
        visit::visit_impl_item_fn(self, f);
    }
    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        if let Some(block) = &f.default {
            self.record(&f.sig.ident, block, f.sig.span().start().line);
        }
        visit::visit_trait_item_fn(self, f);
    }
}
// Fail closed on suppression attributes, including nested cfg_attr payloads.
#[derive(Default)]
struct Policy {
    test: bool,
    errors: Vec<String>,
}
fn identifiers(tokens: proc_macro2::TokenStream) -> Vec<String> {
    tokens
        .into_iter()
        .flat_map(|t| match t {
            proc_macro2::TokenTree::Ident(i) => vec![i.to_string()],
            proc_macro2::TokenTree::Group(g) => identifiers(g.stream()),
            _ => Vec::new(),
        })
        .collect()
}
fn has_suppression(tokens: proc_macro2::TokenStream) -> bool {
    let mut attribute = false;
    for token in tokens {
        match token {
            proc_macro2::TokenTree::Punct(p) => attribute = p.as_char() == '#',
            proc_macro2::TokenTree::Group(g) => {
                if attribute
                    && identifiers(g.stream())
                        .iter()
                        .any(|i| i == "allow" || i == "expect")
                {
                    return true;
                }
                if has_suppression(g.stream()) {
                    return true;
                }
                attribute = false;
            }
            _ => attribute = false,
        }
    }
    false
}
fn is_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("test")
            || (a.path().is_ident("cfg")
                && a.parse_args::<syn::Path>()
                    .is_ok_and(|p| p.is_ident("test")))
    })
}
impl<'ast> Visit<'ast> for Policy {
    fn visit_attribute(&mut self, a: &'ast syn::Attribute) {
        if identifiers(a.to_token_stream())
            .iter()
            .any(|i| i == "allow" || i == "expect")
        {
            self.errors.push(format!(
                "lint suppression attribute at line {}",
                a.span().start().line
            ));
        }
        visit::visit_attribute(self, a);
    }
    fn visit_item(&mut self, item: &'ast syn::Item) {
        let attrs = match item {
            syn::Item::Fn(x) => &x.attrs,
            syn::Item::Mod(x) => &x.attrs,
            syn::Item::Impl(x) => &x.attrs,
            syn::Item::Const(x) => &x.attrs,
            syn::Item::Static(x) => &x.attrs,
            _ => &[][..],
        };
        let before = self.test;
        self.test |= is_test(attrs);
        visit::visit_item(self, item);
        self.test = before;
    }
    fn visit_expr_method_call(&mut self, e: &'ast syn::ExprMethodCall) {
        if !self.test && e.method == "unwrap" {
            self.errors
                .push(format!("unwrap at line {}", e.span().start().line));
        }
        visit::visit_expr_method_call(self, e);
    }
    fn visit_expr_path(&mut self, e: &'ast syn::ExprPath) {
        if !self.test && e.path.segments.last().is_some_and(|s| s.ident == "unwrap") {
            self.errors.push(format!(
                "unwrap reference at line {}",
                e.span().start().line
            ));
        }
        visit::visit_expr_path(self, e);
    }
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let ids = identifiers(m.tokens.clone());
        if has_suppression(m.tokens.clone()) {
            self.errors
                .push("suppression-like tokens in macro; expand or remove them".into());
        }
        if !self.test && ids.iter().any(|i| i == "unwrap") {
            self.errors.push("unwrap token in non-test macro".into());
        }
        visit::visit_macro(self, m);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).ok_or("expected Rust source path")?;
    let syntax = syn::parse_file(&fs::read_to_string(path)?)?;
    let mut policy = Policy {
        test: env::args().any(|a| a == "--test-context"),
        ..Policy::default()
    };
    policy.test |= is_test(&syntax.attrs);
    policy.visit_file(&syntax);
    if !policy.errors.is_empty() {
        return Err(policy.errors.join("; ").into());
    }
    let mut functions = Functions::default();
    functions.visit_file(&syntax);
    println!("{}", serde_json::to_string(&functions.rows)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}
