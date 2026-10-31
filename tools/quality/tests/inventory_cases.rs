use crewmoor_quality::{coverage, driver, inventory::Resolver};
use std::{fs, path::Path};

fn package(root: &Path, source: &str) -> std::path::PathBuf {
    fs::create_dir_all(root.join("src")).unwrap();
    let manifest = root.join("Cargo.toml");
    fs::write(
        &manifest,
        "[package]\nname='inventory-case'\nversion='0.0.0'\nedition='2024'\n",
    )
    .unwrap();
    fs::write(root.join("src/lib.rs"), source).unwrap();
    driver::command(&[
        "generate-lockfile".into(),
        "--manifest-path".into(),
        manifest.display().to_string(),
    ])
    .unwrap();
    manifest
}

#[test]
fn outlined_path_and_item_include_are_owned_sources() {
    let temp = tempfile::tempdir().unwrap();
    let outside = temp.path().join("outside.rs");
    let included = temp.path().join("included.rs");
    fs::write(
        &outside,
        "#[allow(clippy::all)] pub fn risky(x: bool) { if x {} if x {} if x {} }",
    )
    .unwrap();
    fs::write(&included, "pub fn extra() {}\n").unwrap();
    let manifest = package(
        &temp.path().join("pkg"),
        "#[path=\"../../outside.rs\"] pub mod outside;\ninclude!(\"../../included.rs\");\npub fn keep() {}\n",
    );
    let files = driver::inventory(&manifest).unwrap();
    assert_eq!(files.get(&outside), Some(&false));
    assert_eq!(files.get(&included), Some(&false));
    assert!(driver::policy(&manifest).is_err());
    fs::write(
        &outside,
        "pub fn risky(x: bool) { if x {} if x {} if x {} }",
    )
    .unwrap();
    assert!(driver::policy(&manifest).is_ok());
    fs::write(
        &included,
        "pub fn extra(x: Option<u8>) -> u8 { x.unwrap() }",
    )
    .unwrap();
    assert!(driver::policy(&manifest).is_err());
}

#[test]
fn build_and_root_binary_do_not_reclassify_test_targets() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = package(temp.path(), "pub fn keep() {}\n");
    fs::write(temp.path().join("build.rs"), "fn main() {}\n").unwrap();
    fs::write(temp.path().join("main.rs"), "fn main() {}\n").unwrap();
    let text = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        format!("{text}\n[[bin]]\nname='root-bin'\npath='main.rs'\n"),
    )
    .unwrap();
    fs::create_dir(temp.path().join("tests")).unwrap();
    let test = temp.path().join("tests/it.rs");
    fs::write(&test, "fn helper() -> u8 { \"1\".parse::<u8>().unwrap() }\n#[test] fn works() { assert_eq!(helper(), 1); }\n").unwrap();
    let files = driver::policy(&manifest).unwrap();
    assert_eq!(files.get(&test), Some(&true));
    assert_eq!(files.get(&temp.path().join("build.rs")), Some(&false));
    driver::command(&[
        "test".into(),
        "--manifest-path".into(),
        manifest.display().to_string(),
    ])
    .unwrap();
}

#[test]
fn test_context_propagates_through_external_and_shared_modules() {
    let temp = tempfile::tempdir().unwrap();
    let entry = temp.path().join("lib.rs");
    fs::write(&entry, "#[cfg(test)] mod tests;\n").unwrap();
    fs::write(
        temp.path().join("tests.rs"),
        "fn helper() { x.unwrap(); }\n",
    )
    .unwrap();
    let mut resolver = Resolver::default();
    resolver.target(&entry, false).unwrap();
    assert_eq!(
        resolver.finish().get(&temp.path().join("tests.rs")),
        Some(&true)
    );
    let mut resolver = Resolver::default();
    resolver.target(&entry, false).unwrap();
    resolver
        .target(&temp.path().join("tests.rs"), false)
        .unwrap();
    assert_eq!(
        resolver.finish().get(&temp.path().join("tests.rs")),
        Some(&false)
    );
}

#[test]
fn missing_dynamic_and_ambiguous_source_edges_fail() {
    let temp = tempfile::tempdir().unwrap();
    let entry = temp.path().join("lib.rs");
    for text in [
        "mod missing;",
        "include!(concat!(env!(\"OUT_DIR\"), \"/generated.rs\"));",
        "fn f() { include!(\"value.rs\"); }",
        "#[cfg_attr(test,path=\"other.rs\")] mod foo;",
    ] {
        fs::write(&entry, text).unwrap();
        assert!(Resolver::default().target(&entry, false).is_err());
    }
    fs::create_dir(temp.path().join("foo")).unwrap();
    fs::write(temp.path().join("foo.rs"), "").unwrap();
    fs::write(temp.path().join("foo/mod.rs"), "").unwrap();
    fs::write(&entry, "mod foo;").unwrap();
    assert!(Resolver::default().target(&entry, false).is_err());
}

#[test]
fn unknown_coverage_file_is_not_silently_filtered() {
    let temp = tempfile::tempdir().unwrap();
    let wanted = temp.path().join("lib.rs");
    let outside = temp.path().join("outside.rs");
    fs::write(&wanted, "fn f() {}\n").unwrap();
    fs::write(&outside, "fn omitted() {}\n").unwrap();
    let json = serde_json::json!({"type":"llvm.coverage.json.export","data":[{"files":[
        {"filename":wanted,"segments":[[1,1,1,true,true,false]]},
        {"filename":outside,"segments":[[1,1,0,true,true,false]]}]}]});
    assert!(coverage::parse_for(&json.to_string(), std::slice::from_ref(&wanted)).is_err());
    // Only an explicit third-party root can be excluded, never an unknown file.
    assert!(coverage::parse_scoped(&json.to_string(), &[wanted], &[outside]).is_ok());
}

#[test]
fn ancestor_clippy_and_nonstable_toolchain_are_rejected() {
    use crewmoor_quality::config;
    let temp = tempfile::tempdir().unwrap();
    assert!(config::ancestor_clippy(temp.path()).is_ok());
    fs::write(
        temp.path().join("clippy.toml"),
        "too-many-arguments-threshold=100",
    )
    .unwrap();
    assert!(config::ancestor_clippy(temp.path()).is_err());
    assert!(
        config::environment([("RUSTUP_TOOLCHAIN".into(), "1.80.0".into())].into_iter()).is_err()
    );
    assert!(
        config::environment(
            [(
                "RUSTUP_TOOLCHAIN".into(),
                "stable-x86_64-unknown-linux-gnu".into()
            )]
            .into_iter()
        )
        .is_ok()
    );
}

#[test]
fn uncovered_outside_function_fails_crap() {
    let temp = tempfile::tempdir().unwrap();
    let outside = temp.path().join("outside.rs");
    fs::write(
        &outside,
        "pub fn risky(x: bool) { if x {} if x {} if x {} }\n",
    )
    .unwrap();
    let manifest = package(
        &temp.path().join("pkg"),
        "#[path=\"../../outside.rs\"] pub mod outside;\npub fn keep() {}\n",
    );
    let entry = temp.path().join("pkg/src/lib.rs");
    let report = temp.path().join("coverage.json");
    let json = serde_json::json!({"type":"llvm.coverage.json.export","data":[{"files":[
        {"filename":entry,"segments":[[2,1,1,true,true,false]]},
        {"filename":outside,"segments":[[1,1,0,true,true,false]]}]}]});
    fs::write(&report, json.to_string()).unwrap();
    let error = driver::crap(&manifest, &report).unwrap_err().to_string();
    assert!(error.contains("CRAP 20 > 10"), "{error}");
}
