use rupicola::{check, discover, scan};
use std::{collections::BTreeSet, fs, path::Path, process::Command};
use tempfile::TempDir;

fn put(root: &Path, name: &str, content: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}
fn app(root: &Path, deps: &str) {
    put(
        root,
        "Cargo.toml",
        &format!("[package]\nname='demo'\nversion='0.1.0'\n[dependencies]\n{deps}\n"),
    );
}
fn frameworks(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(std::string::ToString::to_string).collect()
}

#[test]
fn discovers_all_framework_crates_without_building() {
    let root = TempDir::new().unwrap();
    app(
        root.path(),
        "cosmic={package='libcosmic',git='https://example.invalid/cosmic'}\ngpui='*'\niced='*'\ntopcoat='*'\nleptos='*'\nyew='*'\ntauri='*'\ndioxus='*'\neframe='*'\nslint='*'\ngtk4='*'\nlibadwaita='*'\nrelm4='*'",
    );
    put(
        root.path(),
        "build.rs",
        "compile_error!(\"must never execute\");",
    );
    let result = discover::discover(root.path()).unwrap();
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.packages[0].frameworks.len(), 13);
}
#[test]
fn workspace_aliases_and_target_conditions_are_retained() {
    let root = TempDir::new().unwrap();
    put(
        root.path(),
        "Cargo.toml",
        "[workspace]\nmembers=['ui']\n[workspace.dependencies]\nui={package='leptos',version='0.8'}\n",
    );
    put(
        root.path(),
        "ui/Cargo.toml",
        "[package]\nname='web'\nversion='0.1.0'\n[dependencies]\nui={workspace=true,optional=true}\n[target.'cfg(target_os = \"linux\")'.dependencies]\ncosmic={package='libcosmic',git='https://example.invalid/cosmic'}\n",
    );
    let result = discover::discover(root.path()).unwrap();
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let package = &result.packages[0];
    assert_eq!(package.dependencies[0].alias, "ui");
    assert_eq!(package.dependencies[0].package, "leptos");
    assert!(package.dependencies[0].optional);
    assert!(package.dependencies[1].target.is_some());
    assert_eq!(package.dependencies[0].requirement.as_deref(), Some("0.8"));
}
#[test]
fn backend_and_dev_only_ui_are_not_application_frameworks() {
    let root = TempDir::new().unwrap();
    app(root.path(), "tokio='1'\naxum='0.8'");
    let path = root.path().join("Cargo.toml");
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str("\n[dev-dependencies]\nyew='*'\n");
    fs::write(path, text).unwrap();
    assert!(
        discover::discover(root.path()).unwrap().packages[0]
            .frameworks
            .is_empty()
    );
}
#[test]
fn malformed_or_unresolved_manifest_is_a_diagnostic() {
    let root = TempDir::new().unwrap();
    app(root.path(), "ui={workspace=true}");
    assert!(
        !discover::discover(root.path())
            .unwrap()
            .diagnostics
            .is_empty()
    );
    put(root.path(), "Cargo.toml", "[broken");
    assert!(
        !discover::discover(root.path())
            .unwrap()
            .diagnostics
            .is_empty()
    );
}
#[test]
fn source_file_request_finds_enclosing_package() {
    let root = TempDir::new().unwrap();
    app(root.path(), "gpui='*'");
    put(root.path(), "src/main.rs", "fn main() {}");
    let result = discover::discover(&root.path().join("src/main.rs")).unwrap();
    assert!(result.packages[0].frameworks.contains("gpui"));
}
#[test]
fn raw_strings_unicode_comments_and_spans_are_preserved() {
    let text = "// view! { <div class=\"transition-all\"/> }\n/* outer /* nested */ */\nfn main() {\nlet _ = \"é transition-all\";\nlet _ = view! { <div class=r#\"transition-all bg-clip-text\"#/> };\n}";
    let result = scan::source(text, Path::new("page.rs"), &frameworks(&["leptos"]), &[]).unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.iter().all(|f| f.line == 5));
    assert!(result.iter().all(|f| f.column > 1));
}
#[test]
fn web_framework_macros_and_class_composition_are_supported() {
    for (framework, text) in [
        (
            "topcoat",
            "fn main(){ let _ = class!(\"transition-all\", optional); }",
        ),
        (
            "yew",
            "fn main(){ let _ = html!{ <div class=\"transition-all\"/> }; }",
        ),
        (
            "dioxus",
            "fn main(){ let _ = rsx!{ div { class: \"transition-all\" } }; }",
        ),
    ] {
        assert_eq!(
            scan::source(text, Path::new("page.rs"), &frameworks(&[framework]), &[])
                .unwrap()
                .len(),
            1
        );
    }
}
#[test]
fn native_code_and_unrelated_strings_do_not_receive_dom_rules() {
    let text = "fn render(){ let _ = \"transition-all\"; let _ = div().child(\"bg-clip-text\"); let _ = view!{ <div class=\"transition-all\"/> }; }";
    assert!(
        scan::source(text, Path::new("page.rs"), &frameworks(&["gpui"]), &[])
            .unwrap()
            .is_empty()
    );
}
#[test]
fn direct_blocking_calls_are_candidates_only_in_ui_functions() {
    let text = "fn helper(){ std::thread::sleep(duration); }\nimpl App { fn render(&self){ std::fs::read_to_string(path); } }";
    let result = scan::source(text, Path::new("app.rs"), &frameworks(&["gpui"]), &[]).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].rule, "blocking-ui");
    assert_eq!(result[0].line, 2);
}
#[test]
fn standalone_ignore_comments_work_but_string_directives_do_not() {
    let text = "fn main(){\n// rupicola-ignore transition-all\nlet _ = view!{ <div class=\"transition-all bg-clip-text\"/> };\nlet _ = \"// rupicola-ignore transition-all\";\nlet _ = view!{ <div class=\"transition-all\"/> };\n}";
    let result = scan::source(text, Path::new("page.rs"), &frameworks(&["leptos"]), &[]).unwrap();
    assert_eq!(
        result.iter().filter(|f| f.rule == "transition-all").count(),
        1
    );
    assert_eq!(
        result.iter().filter(|f| f.rule == "gradient-text").count(),
        1
    );
}
#[test]
fn unsupported_and_parse_failures_are_visible() {
    let root = TempDir::new().unwrap();
    app(root.path(), "slint='*'");
    put(
        root.path(),
        "src/view.slint",
        "export component App inherits Window {}",
    );
    put(root.path(), "src/main.rs", "fn {");
    let result = scan::scan(root.path(), None, false).unwrap();
    assert_eq!(result.unsupported_files.len(), 1);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.parsed_files, 0);
}
#[test]
fn generated_and_test_directories_are_skipped() {
    let root = TempDir::new().unwrap();
    app(root.path(), "leptos='*'");
    let text = "fn main(){ let _ = view!{ <p class=\"transition-all\"/> }; }";
    for name in ["target/page.rs", "tests/page.rs", "src/page.rs"] {
        put(root.path(), name, text);
    }
    assert_eq!(scan::scan(root.path(), None, false).unwrap().rust_files, 1);
    assert_eq!(scan::scan(root.path(), None, true).unwrap().rust_files, 2);
}
#[test]
fn ignore_paths_are_component_aware_and_config_errors_fail() {
    let root = TempDir::new().unwrap();
    app(root.path(), "leptos='*'");
    let text = "fn main(){ let _ = view!{ <p class=\"transition-all\"/> }; }";
    put(root.path(), "src/skip/page.rs", text);
    put(root.path(), "src/skip-other/page.rs", text);
    put(
        root.path(),
        ".rupicola/config.toml",
        "ignore_paths=['src/skip']",
    );
    assert_eq!(
        scan::scan(root.path(), None, false).unwrap().findings.len(),
        1
    );
    put(
        root.path(),
        ".rupicola/config.toml",
        "ignore_rules=['typo']",
    );
    assert!(scan::scan(root.path(), None, false).is_err());
    put(
        root.path(),
        ".rupicola/config.toml",
        "ignore_paths=['../escape']",
    );
    assert!(scan::scan(root.path(), None, false).is_err());
    put(root.path(), ".rupicola/config.toml", "typo=true");
    assert!(scan::scan(root.path(), None, false).is_err());
}
#[test]
fn catalog_and_document_links_are_valid() {
    let result = check::check(Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())).unwrap();
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}
#[test]
fn catalog_frontmatter_accepts_windows_line_endings() {
    let root = TempDir::new().unwrap();
    let checkout = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let checkout = Path::new(&checkout);
    for source in rupicola::files(checkout, true).unwrap() {
        let relative = source
            .strip_prefix(checkout.canonicalize().unwrap())
            .unwrap();
        let extension = source.extension().and_then(|s| s.to_str()).unwrap_or("");
        if !matches!(extension, "md" | "toml")
            && !relative.starts_with("evals")
            && !relative.starts_with("examples")
        {
            continue;
        }
        let text = fs::read_to_string(&source).unwrap();
        put(
            root.path(),
            relative.to_str().unwrap(),
            &text.replace("\r\n", "\n").replace('\n', "\r\n"),
        );
    }
    let result = check::check(root.path()).unwrap();
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
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
#[cfg(unix)]
#[test]
fn nested_symlink_cycles_are_not_followed() {
    let root = TempDir::new().unwrap();
    app(root.path(), "gpui='*'");
    put(root.path(), "src/main.rs", "fn main(){}");
    std::os::unix::fs::symlink(root.path(), root.path().join("src/cycle")).unwrap();
    assert_eq!(scan::scan(root.path(), None, false).unwrap().rust_files, 1);
}

fn task(root: &Path, id: &str, body: &str) {
    put(root, &format!("evals/tasks/{id}/task.toml"), body);
}
const BUILD: &str = "id = \"t\"\nframework = \"iced\"\nkind = \"build\"\nsplit = \"dev\"\nshould_trigger = true\nprompt = \"p\"\n[[verifier]]\nname = \"c\"\ncommand = \"cargo check\"\n";
fn eval_messages(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    rupicola::evals::validate(root, &mut out);
    out.into_iter().map(|d| d.message).collect()
}
#[test]
fn eval_tasks_validate_and_flag_each_rule() {
    let root = TempDir::new().unwrap();
    task(root.path(), "t", BUILD);
    assert_eq!(eval_messages(root.path()), [] as [std::string::String; 0]);
    for (id, body, needle) in [
        (
            "a",
            BUILD.replace("framework = \"iced\"", "framework = \"nope\""),
            "unknown framework",
        ),
        (
            "b",
            BUILD.replace("split = \"dev\"", "split = \"test\""),
            "split must be",
        ),
        (
            "c",
            BUILD.replace(
                "[[verifier]]\nname = \"c\"\ncommand = \"cargo check\"\n",
                "",
            ),
            "verifier",
        ),
        (
            "d",
            BUILD.replace("kind = \"build\"", "kind = \"review\""),
            "seeded_defect",
        ),
        (
            "e",
            BUILD.replace("should_trigger = true", "should_trigger = false"),
            "should_trigger",
        ),
        ("f", format!("{BUILD}extra = 1\n"), "unknown field"),
    ] {
        let dir = TempDir::new().unwrap();
        task(
            dir.path(),
            id,
            &body.replace("id = \"t\"", &format!("id = \"{id}\"")),
        );
        let messages = eval_messages(dir.path());
        assert!(
            messages.iter().any(|m| m.contains(needle)),
            "{id}: {messages:?}"
        );
    }
}
#[test]
fn eval_id_must_match_directory_and_absent_dir_is_fine() {
    let root = TempDir::new().unwrap();
    assert_eq!(eval_messages(root.path()), [] as [std::string::String; 0]);
    task(root.path(), "other", BUILD);
    assert!(
        eval_messages(root.path())
            .iter()
            .any(|m| m.contains("directory name"))
    );
}

#[test]
fn hardcoded_color_is_a_candidate_only_for_literal_colors_in_ui_functions() {
    let text = "fn ui(){ let a = Color32::from_rgb(10, 20, 30); let b = Color::from_rgb8(1,2,3); let c = Color32::from_rgb(r, 20, 30); let d = Other::from_rgb(1,2,3); }\nfn helper(){ let e = Color32::from_rgb(1,2,3); }\nconst X: u8 = 1;";
    let findings = rupicola::scan::source(
        text,
        std::path::Path::new("a.rs"),
        &frameworks(&["egui"]),
        &[],
    )
    .unwrap();
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "hardcoded-color")
        .map(|f| f.line)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "same-line findings collapse to one: {findings:?}"
    );
    assert_eq!(hits[0], 1);
}
