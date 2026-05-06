use assert_cmd::Command;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn init_dry_run_reports_without_writing() {
    let temp = tempfile::tempdir().unwrap();
    let output = atlas(temp.path())
        .args(["init", "--dry-run", "--agents", "all"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let payload: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["dry_run"], true);
    assert!(!temp.path().join(".atlas").exists());
    assert!(payload["skill_paths"]
        .as_array()
        .unwrap()
        .contains(&json!(".agents/skills/atlas-app-navigation/SKILL.md")));
}

#[test]
fn route_reports_context_mismatch_exit_code() {
    let temp = tempfile::tempdir().unwrap();
    write_json(
        &temp.path().join(".atlas/config.json"),
        &json!({
            "schema_version": "atlas.config.v1",
            "context": {"auth_state": "logged_out"}
        }),
    );
    write_json(
        &temp.path().join(".atlas/routes/open-account.atlas.json"),
        &json!({
            "schema_version": "atlas.route.v1",
            "name": "open-account",
            "start": {"screen": "home", "context_guard": {"auth_state": "logged_in"}},
            "target": {"screen": "account"}
        }),
    );
    let output = atlas(temp.path())
        .args(["route", "open-account", "--current-screen", "home"])
        .assert()
        .code(8)
        .get_output()
        .stdout
        .clone();
    let payload: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(payload["status"], "context_mismatch");
}

#[test]
fn observe_and_learn_stage_review_proposal() {
    let temp = tempfile::tempdir().unwrap();
    atlas(temp.path())
        .args(["observe", "start", "article-detail"])
        .assert()
        .success();
    let output = atlas(temp.path())
        .args(["learn", "--from-current-run", "--stage"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let payload: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(payload["status"], "changed_requires_review");
    assert_eq!(payload["human_approval_required"], true);
    let proposal_path = PathBuf::from(payload["proposal_path"].as_str().unwrap());
    let proposal_path = if proposal_path.is_absolute() {
        proposal_path
    } else {
        temp.path().join(proposal_path)
    };
    assert!(proposal_path.exists());
}

#[test]
fn layout_diff_uses_fake_android_cli() {
    let temp = tempfile::tempdir().unwrap();
    let bin = fake_bin(temp.path());
    write_executable(
        &bin.join("android"),
        r#"#!/bin/sh
if [ "$1" = "layout" ] && [ "$2" = "--diff" ]; then
  printf '{"changed":[{"text":"Settings"}]}'
  exit 0
fi
exit 2
"#,
    );
    let output = atlas(temp.path())
        .env("PATH", prepend_path(&bin))
        .args(["layout", "--diff"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let payload: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(payload["kind"], "android_layout_diff");
    assert_eq!(payload["diff_scope"], "android_in_session");
}

#[test]
fn tap_selector_uses_fake_android_and_adb() {
    let temp = tempfile::tempdir().unwrap();
    let bin = fake_bin(temp.path());
    write_executable(
        &bin.join("android"),
        r#"#!/bin/sh
if [ "$1" = "layout" ]; then
  printf '{"children":[{"text":"Settings","bounds":{"left":100,"top":200,"right":300,"bottom":400}}]}'
  exit 0
fi
exit 2
"#,
    );
    write_executable(
        &bin.join("adb"),
        r#"#!/bin/sh
if [ "$1" = "shell" ] && [ "$2" = "input" ] && [ "$3" = "tap" ] && [ "$4" = "200" ] && [ "$5" = "300" ]; then
  exit 0
fi
exit 2
"#,
    );
    let output = atlas(temp.path())
        .env("PATH", prepend_path(&bin))
        .args([
            "tap",
            "--selector",
            "text=Settings",
            "--reason",
            "open settings",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let payload: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(payload["action"]["path"], "layout_selector");
    assert_eq!(payload["action"]["point"], json!({"x": 200, "y": 300}));
}

fn atlas(dir: &Path) -> Command {
    let mut command = Command::cargo_bin("atlas").unwrap();
    command.current_dir(dir);
    command
}

fn write_json(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

fn fake_bin(root: &Path) -> PathBuf {
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    bin
}

fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }
}

fn prepend_path(bin: &Path) -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    format!("{}:{current}", bin.display())
}
