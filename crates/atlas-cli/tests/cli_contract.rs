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
    assert!(payload["skill_paths"]
        .as_array()
        .unwrap()
        .contains(&json!(".agents/skills/atlas-first-run-mapping/SKILL.md")));
}

#[test]
fn claude_plugin_marketplace_declares_atlas_skills() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let marketplace = read_json_path(&repo.join(".claude-plugin/marketplace.json"));
    assert_eq!(marketplace["name"], "atlas");
    assert_eq!(marketplace["owner"]["name"], "Matt McKenna");
    assert_eq!(marketplace["plugins"][0]["name"], "atlas");
    assert_eq!(marketplace["plugins"][0]["author"]["name"], "Matt McKenna");
    assert_eq!(
        marketplace["plugins"][0]["source"],
        "./plugins/atlas-claude-code"
    );

    let plugin = read_json_path(&repo.join("plugins/atlas-claude-code/.claude-plugin/plugin.json"));
    assert_eq!(plugin["name"], "atlas");
    assert_eq!(plugin["author"]["name"], "Matt McKenna");

    assert!(repo
        .join("plugins/atlas-claude-code/skills/atlas-app-navigation/SKILL.md")
        .exists());
    let first_run_skill = fs::read_to_string(
        repo.join("plugins/atlas-claude-code/skills/atlas-first-run-mapping/SKILL.md"),
    )
    .unwrap();
    assert!(first_run_skill.contains("token-intensive"));
    assert!(first_run_skill.contains("Use this skill one time"));
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
fn learn_after_stopped_run_stages_screen_edge_and_route_changes() {
    let temp = tempfile::tempdir().unwrap();
    let bin = fake_bin(temp.path());
    write_executable(
        &bin.join("android"),
        r#"#!/bin/sh
COUNT_FILE="$(dirname "$0")/learn-count"
COUNT=0
if [ -f "$COUNT_FILE" ]; then COUNT=$(cat "$COUNT_FILE"); fi
COUNT=$((COUNT + 1))
printf "%s" "$COUNT" > "$COUNT_FILE"
if [ "$1" = "layout" ]; then
  if [ "$COUNT" = "1" ] || [ "$COUNT" = "2" ]; then
    printf '{"class":"Column","children":[{"text":"Open","bounds":{"left":0,"top":0,"right":100,"bottom":100}}]}'
  else
    printf '{"class":"Column","children":[{"class":"Text","text":"Article body"}]}'
  fi
  exit 0
fi
exit 2
"#,
    );
    write_executable(
        &bin.join("adb"),
        r#"#!/bin/sh
if [ "$1" = "shell" ] && [ "$2" = "input" ] && [ "$3" = "tap" ]; then
  exit 0
fi
exit 2
"#,
    );
    atlas(temp.path())
        .args(["observe", "start", "article-detail"])
        .assert()
        .success();
    atlas(temp.path())
        .env("PATH", prepend_path(&bin))
        .args(["layout"])
        .assert()
        .success();
    atlas(temp.path())
        .env("PATH", prepend_path(&bin))
        .args(["tap", "--selector", "text=Open", "--reason", "open article"])
        .assert()
        .success();
    atlas(temp.path())
        .env("PATH", prepend_path(&bin))
        .args(["layout"])
        .assert()
        .success();
    atlas(temp.path())
        .args(["observe", "stop"])
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
    let proposal_path = PathBuf::from(payload["proposal_path"].as_str().unwrap());
    let proposal_path = if proposal_path.is_absolute() {
        proposal_path
    } else {
        temp.path().join(proposal_path)
    };
    let proposal = read_json_path(&proposal_path);
    assert_eq!(proposal["kind"], "learned_route");
    assert_eq!(proposal["changes"].as_array().unwrap().len(), 4);
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

#[test]
fn go_executes_route_edges_with_fake_android_and_adb() {
    let temp = tempfile::tempdir().unwrap();
    write_json(
        &temp
            .path()
            .join(".atlas/graph/screens/screen_article_detail.json"),
        &json!({
            "schema_version": "atlas.screen.v1",
            "id": "screen_article_detail",
            "name": "article-detail",
            "identity_hash": "sha256:not-fast-path",
            "normalized": {
                "schema_version": "atlas.normalized_layout.v1",
                "elements": [
                    {"role": "Column", "clickable": false, "enabled": true, "path": "0", "sibling_bucket": 0},
                    {"role": "Text", "clickable": false, "enabled": true, "path": "0/0", "sibling_bucket": 0, "text_class": "medium"}
                ],
                "role_distribution": {"Column": 1, "Text": 1},
                "element_count": 2
            }
        }),
    );
    write_json(
        &temp
            .path()
            .join(".atlas/graph/edges/edge_home_article.json"),
        &json!({
            "schema_version": "atlas.edge.v1",
            "id": "edge_home_article",
            "from_screen": "home",
            "to_screen": "article-detail",
            "action": {
                "kind": "tap",
                "selector_candidates": [
                    {"kind": "test_tag", "value": "read_article", "score": 0.92}
                ]
            }
        }),
    );
    write_json(
        &temp.path().join(".atlas/routes/read-article.atlas.json"),
        &json!({
            "schema_version": "atlas.route.v1",
            "name": "read-article",
            "start": {"screen": "home"},
            "target": {"screen": "article-detail"},
            "preferred_edge_ids": ["edge_home_article"]
        }),
    );
    let bin = fake_bin(temp.path());
    write_executable(
        &bin.join("android"),
        r#"#!/bin/sh
COUNT_FILE="$(dirname "$0")/android-count"
COUNT=0
if [ -f "$COUNT_FILE" ]; then COUNT=$(cat "$COUNT_FILE"); fi
COUNT=$((COUNT + 1))
printf "%s" "$COUNT" > "$COUNT_FILE"
if [ "$1" = "layout" ]; then
  if [ "$COUNT" = "1" ]; then
    printf '{"class":"Column","children":[{"testTag":"read_article","bounds":{"left":100,"top":200,"right":300,"bottom":400}}]}'
  else
    printf '{"class":"Column","children":[{"class":"Text","text":"Article body"}]}'
  fi
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
        .args(["go", "read-article", "--current-screen", "home"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let payload: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["summary"], "route executed");
    assert_eq!(payload["executed"][0]["edge"], "edge_home_article");
    assert_eq!(
        payload["executed"][0]["verification"]["matched_screen"],
        "article-detail"
    );
    assert_eq!(payload["metrics"]["layout_calls_total"], 2);
    assert_eq!(payload["metrics"]["layout_json_returned_to_agent"], false);
    assert_eq!(payload["metrics"]["adb_taps_total"], 1);
}

#[test]
fn check_current_matches_known_screen_without_returning_layout_json() {
    let temp = tempfile::tempdir().unwrap();
    write_settings_screen(temp.path());
    let bin = fake_bin(temp.path());
    write_executable(
        &bin.join("android"),
        r#"#!/bin/sh
if [ "$1" = "layout" ]; then
  printf '{"class":"Column","children":[{"class":"Button","testTag":"settings","clickable":true}]}'
  exit 0
fi
exit 2
"#,
    );
    let output = atlas(temp.path())
        .env("PATH", prepend_path(&bin))
        .args(["check", "--current"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let payload: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(payload["current_screen"]["status"], "matched");
    assert_eq!(payload["current_screen"]["matched_screen"], "settings");
    assert_eq!(payload["metrics"]["layout_json_returned_to_agent"], false);
}

#[test]
fn drift_passes_for_known_current_screen() {
    let temp = tempfile::tempdir().unwrap();
    write_settings_screen(temp.path());
    let bin = fake_bin(temp.path());
    write_executable(
        &bin.join("android"),
        r#"#!/bin/sh
if [ "$1" = "layout" ]; then
  printf '{"class":"Column","children":[{"class":"Button","testTag":"settings","clickable":true}]}'
  exit 0
fi
exit 2
"#,
    );
    let output = atlas(temp.path())
        .env("PATH", prepend_path(&bin))
        .args(["drift"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let payload: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(payload["status"], "passed");
    assert_eq!(payload["current_screen"]["matched_screen"], "settings");
}

#[test]
fn validate_reports_screen_unknown_and_stages_proposal() {
    let temp = tempfile::tempdir().unwrap();
    let bin = fake_bin(temp.path());
    write_executable(
        &bin.join("android"),
        r#"#!/bin/sh
if [ "$1" = "layout" ]; then
  printf '{"class":"Column","children":[{"class":"Button","testTag":"unknown","clickable":true}]}'
  exit 0
fi
exit 2
"#,
    );
    let output = atlas(temp.path())
        .env("PATH", prepend_path(&bin))
        .args(["validate"])
        .assert()
        .code(5)
        .get_output()
        .stdout
        .clone();
    let payload: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(payload["status"], "screen_unknown");
    assert!(payload["drift"]["proposal_path"].as_str().is_some());
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

fn read_json_path(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn write_settings_screen(root: &Path) {
    write_json(
        &root.join(".atlas/graph/screens/screen_settings.json"),
        &json!({
            "schema_version": "atlas.screen.v1",
            "id": "screen_settings",
            "name": "settings",
            "identity_hash": "sha256:not-fast-path",
            "normalized": {
                "schema_version": "atlas.normalized_layout.v1",
                "elements": [
                    {"role": "Column", "clickable": false, "enabled": true, "path": "0", "sibling_bucket": 0},
                    {"role": "Button", "clickable": true, "enabled": true, "path": "0/0", "sibling_bucket": 0, "resource_id": "settings"}
                ],
                "role_distribution": {"Button": 1, "Column": 1},
                "element_count": 2
            }
        }),
    );
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
