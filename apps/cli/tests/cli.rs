use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn put(root: &Path, name: &str, content: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn binary_json_and_exit_codes_are_stable() {
    let root = TempDir::new().unwrap();
    let compiled_binary = Path::new(env!("CARGO_BIN_EXE_rupicola"));
    let binary = compiled_binary
        .strip_prefix(env!("CARGO_MANIFEST_DIR"))
        .map_or_else(
            |_| compiled_binary.to_path_buf(),
            |relative| Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join(relative),
        );
    put(
        root.path(),
        "page.rs",
        r#"fn main(){let _=view!{ <p class="transition-all"/> };}"#,
    );
    let file = root.path().join("page.rs");
    let output = Command::new(&binary)
        .args([
            "scan",
            file.to_str().unwrap(),
            "--framework",
            "leptos",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["findings"][0]["rule"], "transition-all");
    let output = Command::new(&binary)
        .args(["scan", file.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    put(root.path(), "page.rs", "fn broken {");
    let output = Command::new(&binary)
        .args([
            "scan",
            file.to_str().unwrap(),
            "--framework",
            "gpui",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let output = Command::new(&binary)
        .args(["scan", file.to_str().unwrap(), "--framework", "made-up"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}
