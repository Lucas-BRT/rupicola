use serde_json::Value;
use std::{fs, path::Path, process::Command};

const SCENARIO: &str = r#"
version = 1
name = "Fixture"
url = "http://127.0.0.1:1"
[project]
id = "fixture"
label = "Fixture project"
[profile]
width = 320
height = 240
scale = 1.0
locale = "en"
color_scheme = "light"
[[steps]]
kind = "checkpoint"
id = "before"
"#;

fn manifest(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn capture_rejects_bad_scenarios_before_creating_output_and_records_runtime_blockers() {
    let tmp = tempfile::tempdir().unwrap();
    let scenario = tmp.path().join("scenario.toml");
    let output = tmp.path().join("run");
    let capture = || {
        Command::new(env!("CARGO_BIN_EXE_rupicola"))
            .arg("capture")
            .arg(&scenario)
            .arg("--out")
            .arg(&output)
            .arg("--playwright-module")
            .arg(tmp.path().join("missing-module"))
            .output()
            .unwrap()
    };

    fs::write(&scenario, SCENARIO.replace("version = 1", "version = 2")).unwrap();
    let rejected = capture();
    assert_eq!(rejected.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("unsupported scenario version 2"));
    assert!(!output.exists());

    fs::write(&scenario, SCENARIO).unwrap();
    let blocked = capture();
    assert_eq!(blocked.status.code(), Some(1));
    let path = output.join("manifest.json");
    let evidence = manifest(&path);
    assert_eq!(evidence["version"], 1);
    assert_eq!(evidence["status"], "blocked");
    assert_eq!(evidence["inspection"], "pending");
    assert!(
        evidence["blocker"]
            .as_str()
            .unwrap()
            .contains("Playwright module path")
    );

    assert_eq!(capture().status.code(), Some(1));
    assert_eq!(manifest(&path), evidence);
}
