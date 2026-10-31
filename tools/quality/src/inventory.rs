//! Resolve target entrypoints and literal source edges, without directory guesses.
use crate::{Result, driver::Inventory, source};
use quote::ToTokens;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use syn::{
    ext::IdentExt,
    visit::{self, Visit},
};

#[derive(Clone)]
struct Context {
    file: PathBuf,
    modules: PathBuf,
    paths: PathBuf,
    test: bool,
}
#[derive(Default)]
pub struct Resolver {
    files: Inventory,
    visited: BTreeSet<(PathBuf, PathBuf, bool)>,
}
impl Resolver {
    pub fn target(&mut self, path: &Path, test: bool) -> Result<()> {
        let file = path.canonicalize()?;
        let dir = parent(&file)?;
        self.load(Context {
            file,
            modules: dir.clone(),
            paths: dir,
            test,
        })
    }
    pub fn finish(self) -> Inventory {
        self.files
    }
    fn load(&mut self, mut context: Context) -> Result<()> {
        context.file = context.file.canonicalize()?;
        let key = (context.file.clone(), context.modules.clone(), context.test);
        if !self.visited.insert(key) {
            return Ok(());
        }
        let syntax = syn::parse_file(&fs::read_to_string(&context.file)?)?;
        context.test |= source::is_test(&syntax.attrs);
        *self.files.entry(context.file.clone()).or_insert(true) &= context.test;
        self.scan(context, &syntax)
    }
    fn scan(&mut self, context: Context, syntax: &syn::File) -> Result<()> {
        let mut edges = Edges {
            context,
            next: Vec::new(),
            errors: Vec::new(),
        };
        edges.visit_file(syntax);
        if !edges.errors.is_empty() {
            return Err(edges.errors.join("; ").into());
        }
        for child in edges.next {
            self.load(child)?;
        }
        Ok(())
    }
}
fn parent(path: &Path) -> Result<PathBuf> {
    Ok(path.parent().ok_or("source has no parent")?.to_path_buf())
}
fn module_dir(path: &Path) -> Result<PathBuf> {
    let dir = parent(path)?;
    if path.file_name().is_some_and(|n| n == "mod.rs") {
        return Ok(dir);
    }
    Ok(dir.join(path.file_stem().ok_or("module has no filename")?))
}
fn explicit_path(attrs: &[syn::Attribute]) -> Result<Option<PathBuf>> {
    let mut result = None;
    for attr in attrs {
        reject_conditional_path(attr)?;
        if path_is(attr.path(), "path") {
            result = Some(path_value(attr)?);
        }
    }
    Ok(result)
}
fn reject_conditional_path(attr: &syn::Attribute) -> Result<()> {
    if path_is(attr.path(), "cfg_attr") && attr.to_token_stream().to_string().contains("path") {
        return Err(
            "conditional path attributes are unsupported; use explicit source modules".into(),
        );
    }
    Ok(())
}
fn path_value(attr: &syn::Attribute) -> Result<PathBuf> {
    let value = &attr.meta.require_name_value()?.value;
    if let syn::Expr::Lit(expr) = value
        && let syn::Lit::Str(lit) = &expr.lit
    {
        return Ok(PathBuf::from(lit.value()));
    }
    Err("module path must be a string literal".into())
}
fn ordinary_module(dir: &Path, name: &str) -> Result<PathBuf> {
    let candidates = [
        dir.join(format!("{name}.rs")),
        dir.join(name).join("mod.rs"),
    ];
    let found: Vec<_> = candidates.into_iter().filter(|p| p.is_file()).collect();
    match found.as_slice() {
        [path] => Ok(path.clone()),
        _ => Err(format!("missing/ambiguous module {name} in {}", dir.display()).into()),
    }
}
struct Edges {
    context: Context,
    next: Vec<Context>,
    errors: Vec<String>,
}
impl Edges {
    fn module(&mut self, item: &syn::ItemMod) -> Result<()> {
        let explicit = explicit_path(&item.attrs)?;
        let test = self.context.test || source::is_test(&item.attrs);
        if let Some((_, items)) = &item.content {
            return self.inline(item, items, explicit, test);
        }
        let (file, is_explicit) = self.outline(item, explicit)?;
        let dir = parent(&file)?;
        // Unlike ordinary foo.rs modules, #[path] modules resolve children
        // directly beside the selected source file.
        let modules = if is_explicit {
            dir.clone()
        } else {
            module_dir(&file)?
        };
        self.next.push(Context {
            modules,
            paths: dir,
            file,
            test,
        });
        Ok(())
    }
    fn outline(&self, item: &syn::ItemMod, explicit: Option<PathBuf>) -> Result<(PathBuf, bool)> {
        match explicit {
            Some(path) => Ok((self.context.paths.join(path).canonicalize()?, true)),
            None => ordinary_module(&self.context.modules, &item.ident.unraw().to_string())
                .map(|file| (file, false)),
        }
    }
    fn inline(
        &mut self,
        item: &syn::ItemMod,
        items: &[syn::Item],
        explicit: Option<PathBuf>,
        test: bool,
    ) -> Result<()> {
        let dir = self
            .context
            .modules
            .join(explicit.unwrap_or_else(|| PathBuf::from(item.ident.unraw().to_string())));
        let previous = self.context.clone();
        self.context = Context {
            modules: dir.clone(),
            paths: dir,
            test,
            ..previous.clone()
        };
        for item in items {
            self.visit_item(item);
        }
        self.context = previous;
        Ok(())
    }
    fn include(&mut self, item: &syn::ItemMacro) -> Result<()> {
        let literal: syn::LitStr = syn::parse2(item.mac.tokens.clone()).map_err(
            |_| "include! source must be a literal path; dynamic generated source is unsupported",
        )?;
        let file = parent(&self.context.file)?
            .join(literal.value())
            .canonicalize()?;
        let dir = parent(&file)?;
        self.next.push(Context {
            file,
            modules: dir.clone(),
            paths: dir,
            test: self.context.test || source::is_test(&item.attrs),
        });
        Ok(())
    }
}
impl<'ast> Visit<'ast> for Edges {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if let Err(error) = self.module(item) {
            self.errors.push(error.to_string());
        }
    }
    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if path_is(&item.mac.path, "include") {
            if let Err(error) = self.include(item) {
                self.errors.push(error.to_string());
            }
        } else {
            visit::visit_item_macro(self, item);
        }
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if path_is(&mac.path, "include") {
            self.errors.push("non-item include! is unsupported".into());
        }
        visit::visit_macro(self, mac);
    }
}

fn path_is(path: &syn::Path, name: &str) -> bool {
    path.get_ident().is_some_and(|ident| ident.unraw() == name)
}
