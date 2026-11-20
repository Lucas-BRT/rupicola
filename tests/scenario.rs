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
        Command::new(env!("CARGO_BIN_EXE_ferris-atelier"))
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

#[test]
fn review_attaches_known_artifacts_and_preserves_run_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("manifest.json");
    let initial = serde_json::json!({
        "version": 1,
        "status": "complete",
        "inspection": "pending",
        "created_at_ms": 123,
        "checkpoints": [{ "id": "before", "path": "before.png" }],
        "clips": []
    });
    fs::write(&path, serde_json::to_vec(&initial).unwrap()).unwrap();
    fs::write(tmp.path().join("before.png"), b"fixture").unwrap();

    let review = |note, evidence| {
        Command::new(env!("CARGO_BIN_EXE_ferris-atelier"))
            .arg("capture-review")
            .arg(&path)
            .args(["--note", note, "--evidence", evidence])
            .output()
            .unwrap()
    };
    assert_eq!(
        review("Bad reference", "../outside.png").status.code(),
        Some(1)
    );
    assert_eq!(manifest(&path), initial);
    assert!(review("Focus is visible.", "before.png").status.success());
    assert!(review("No label overlap.", "before.png").status.success());

    let evidence = manifest(&path);
    assert_eq!(evidence["inspection"], "reviewed");
    assert_eq!(evidence["status"], "complete");
    assert_eq!(evidence["created_at_ms"], 123);
    assert_eq!(evidence["findings"][0]["note"], "Focus is visible.");
    assert_eq!(evidence["findings"][1]["evidence"], "before.png");
    assert!(!tmp.path().join(".review.lock").exists());
}
