use anyhow::{Context, Result};
use atlas_schemas::{
    canonical_json, GraphContext, NavigationEdge, Proposal, Route, ScreenNode,
    CONFIG_SCHEMA_VERSION, EDGE_SCHEMA_VERSION, PROPOSAL_SCHEMA_VERSION, ROUTE_SCHEMA_VERSION,
    SCREEN_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_SKILL_NAME: &str = APP_NAVIGATION_SKILL_NAME;
pub const APP_NAVIGATION_SKILL_NAME: &str = "atlas-app-navigation";
pub const FIRST_RUN_MAPPING_SKILL_NAME: &str = "atlas-first-run-mapping";
pub const GITIGNORE_ENTRIES: &[&str] = &[".atlas/runs/", ".atlas/state/"];

pub const ATLAS_DIRS: &[&str] = &[
    ".atlas",
    ".atlas/graph",
    ".atlas/graph/screens",
    ".atlas/graph/edges",
    ".atlas/routes",
    ".atlas/checks",
    ".atlas/proposals",
    ".atlas/runs",
    ".atlas/state",
];

pub const APP_NAVIGATION_SKILL_BODY: &str = r#"---
name: atlas-app-navigation
description: Use when working in an Android codebase and needing to navigate the launched app, inspect Android layout JSON, use android layout, use android layout --diff, tap UI elements, validate screens, learn routes, reuse known navigation, or update the repo's Atlas graph. Before calling android layout or raw adb tap commands directly, check Atlas first.
metadata:
  author: atlas
  version: "1.0"
---

# Atlas App Navigation Skill

Atlas is this repo's shared navigation memory and soft validation layer for AI agents working in this Android codebase.

Use Atlas before raw Android layout or adb tap commands. Stage learned graph updates, but do not accept or commit them without explicit user approval.
"#;

pub const FIRST_RUN_MAPPING_SKILL_BODY: &str = r#"---
name: atlas-first-run-mapping
description: Use only when the user explicitly asks to perform first-run mapping, create an initial Atlas graph, map a new area of the app, explore a launched Android app for navigation memory, or record known routes from scratch. This is token-intensive and should be bounded, staged, and reviewed.
metadata:
  author: atlas
  version: "1.0"
---

# Atlas First-Run Mapping Skill

Atlas first-run mapping creates initial navigation memory for a launched Android app. This skill is intentionally separate from everyday Atlas navigation because it is expensive: the agent must inspect Android layout JSON, decide what to tap, navigate the app, and record routes before Atlas can reuse the graph.

Stage learned graph updates, but do not accept or commit them without explicit user approval.

## First-Run Mapping Mode

Use this mode when the user asks to map the app, create an initial Atlas graph, do first-run mapping, explore the launched app, record known routes, or build navigation memory from scratch.

Warn the user before starting: first-run mapping is token-intensive. Keep the run bounded by the user's requested scope. If no scope is given, map a small set of high-value flows first, then report what remains.

Use this skill one time for an initial app map, or later only with a specific bounded reason:
- A new feature area has no Atlas route yet.
- A major UI redesign invalidated existing routes.
- A separate app context needs mapping, such as logged-out, logged-in, onboarding, permission-gated, or feature-flagged states.
- The user explicitly asks for additional route coverage.

Prerequisites:
- The Android app is already built, installed, launched, and on the screen where mapping should begin.
- `atlas`, `android`, and `adb` are on PATH.
- Run `atlas init --agents all` if Atlas has not been initialized.
- Run `atlas doctor` and fix blocking environment issues before mapping.

Mapping workflow for each route:
1. Choose a short route name such as `settings`, `article-detail`, or `profile-edit`.
2. Run `atlas map --discover <route-name> --max-actions 5 --stage`.
3. Run `atlas layout` and inspect the current screen.
4. Choose stable selectors in this order: test tag, resource id, accessibility/content description, stable visible text. Avoid coordinate taps unless there is no usable selector.
5. Run `atlas tap --selector "<kind>=<value>" --reason "<why this moves toward the route>"`.
6. Run `atlas layout` after each meaningful transition.
7. Repeat only until the named route target is reached. Avoid unbounded crawling.
8. Run `atlas map --discover <route-name> --max-actions 5 --stage --finish`.
9. Record the proposal id/path for the user. Do not run `atlas accept` unless the user explicitly approves accepting staged graph changes.

After mapping:
- Run `atlas validate --all` when at least one route has been accepted.
- Summarize mapped routes, staged proposals, selectors used, screens reached, and any flows skipped.
- Keep raw layout observations in `.atlas/runs/`; do not commit raw layouts or runtime state.

Failure handling:
- If a selector no longer works, run `atlas drift` or `atlas repair <target> --stage`.
- If the current screen is unknown, stage the proposal and report that review is required.
- If login, onboarding, permissions, or feature flags block navigation, report the required context instead of forcing through it.
"#;

#[derive(Debug, Clone, Serialize)]
pub struct InitChange {
    pub kind: String,
    pub path: String,
    pub status: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InitResult {
    pub ok: bool,
    pub dry_run: bool,
    pub root: String,
    pub agents: Vec<String>,
    pub skill_paths: Vec<String>,
    pub changes: Vec<InitChange>,
}

pub fn default_config() -> Value {
    json!({
        "schema_version": CONFIG_SCHEMA_VERSION,
        "android": {
            "app_package": "",
            "assume_app_launched": true,
            "permissions": []
        },
        "context": {
            "auth_state": "unknown",
            "onboarding_state": "unknown",
            "locale": "unknown",
            "orientation": "unknown",
            "feature_flags": {}
        },
        "storage": {
            "commit_raw_layouts": false,
            "runs_dir": ".atlas/runs",
            "state_dir": ".atlas/state",
            "commit_runtime_telemetry": false,
            "generate_index_cache": true,
            "commit_index_cache": false
        },
        "navigation": {
            "default_mode": "verified",
            "safe_mode_fallback": true,
            "screen_match_confidence_min": 0.78,
            "repair_candidate_confidence_min": 0.65,
            "transition_timeout_ms": 3000
        },
        "normalization": {
            "store_normalized_bounds": true,
            "collapse_repeating_lists": true,
            "strip_dynamic_text_inputs": true
        },
        "redaction": {
            "run_before_hashing": true,
            "default_text_action": "exclude",
            "commit_verbatim_text": false,
            "allowlist_static_text": ["Home", "Settings", "Bookmarks"]
        },
        "skills": {
            "skill_name": DEFAULT_SKILL_NAME,
            "skill_names": [APP_NAVIGATION_SKILL_NAME, FIRST_RUN_MAPPING_SKILL_NAME],
            "install_strategy": "multi-write-detected",
            "install_paths": [".agents/skills", ".codex/skills", ".skills", ".agent/skills", ".claude/skills", ".gemini/skills"]
        }
    })
}

pub fn run_init(root: &Path, dry_run: bool, agents: &str) -> Result<InitResult> {
    let agents = parse_agents(agents)?;
    let skill_paths = skill_paths_for_agents(&agents);
    let changes = plan_init(root, &skill_paths);
    if !dry_run {
        apply_init(root, &changes)?;
    }
    let changes = if dry_run {
        changes
            .into_iter()
            .map(|mut change| {
                if matches!(change.status.as_str(), "create" | "append") {
                    change.status = "planned".to_string();
                }
                change
            })
            .collect()
    } else {
        changes
    };
    Ok(InitResult {
        ok: true,
        dry_run,
        root: root
            .canonicalize()
            .unwrap_or_else(|_| root.to_path_buf())
            .display()
            .to_string(),
        agents,
        skill_paths,
        changes,
    })
}

fn parse_agents(value: &str) -> Result<Vec<String>> {
    let parts: Vec<_> = value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let agents = match parts.as_slice() {
        ["all"] => vec!["codex", "claude", "android-studio", "gemini"],
        ["auto"] => vec!["codex"],
        [] => anyhow::bail!("--agents must not be empty"),
        _ => parts,
    };
    let valid = ["codex", "claude", "android-studio", "gemini"];
    for agent in &agents {
        if !valid.contains(agent) {
            anyhow::bail!("unknown agent: {agent}");
        }
    }
    Ok(agents.into_iter().map(str::to_string).collect())
}

fn skill_paths_for_agents(agents: &[String]) -> Vec<String> {
    let mut paths = Vec::new();
    for agent in agents {
        let roots: &[&str] = match agent.as_str() {
            "codex" => &[".agents/skills", ".codex/skills"],
            "claude" => &[".claude/skills"],
            "android-studio" => &[".skills", ".agent/skills"],
            "gemini" => &[".gemini/skills"],
            _ => &[],
        };
        for root in roots {
            for skill_name in [APP_NAVIGATION_SKILL_NAME, FIRST_RUN_MAPPING_SKILL_NAME] {
                let path = format!("{root}/{skill_name}/SKILL.md");
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
    }
    paths
}

fn plan_init(root: &Path, skill_paths: &[String]) -> Vec<InitChange> {
    let mut changes = Vec::new();
    for dir in ATLAS_DIRS {
        changes.push(InitChange {
            kind: "directory".to_string(),
            path: dir.to_string(),
            status: if root.join(dir).exists() {
                "exists"
            } else {
                "create"
            }
            .to_string(),
            detail: String::new(),
        });
    }
    changes.push(InitChange {
        kind: "config".to_string(),
        path: ".atlas/config.json".to_string(),
        status: if root.join(".atlas/config.json").exists() {
            "exists"
        } else {
            "create"
        }
        .to_string(),
        detail: String::new(),
    });
    let missing = missing_gitignore_entries(root);
    changes.push(InitChange {
        kind: "gitignore".to_string(),
        path: ".gitignore".to_string(),
        status: if missing.is_empty() {
            "exists"
        } else {
            "append"
        }
        .to_string(),
        detail: if missing.is_empty() {
            String::new()
        } else {
            format!("add {}", missing.join(", "))
        },
    });
    for path in skill_paths {
        changes.push(InitChange {
            kind: "skill".to_string(),
            path: path.clone(),
            status: if root.join(path).exists() {
                "exists"
            } else {
                "create"
            }
            .to_string(),
            detail: String::new(),
        });
    }
    changes
}

fn apply_init(root: &Path, changes: &[InitChange]) -> Result<()> {
    for change in changes {
        let path = root.join(&change.path);
        match (change.kind.as_str(), change.status.as_str()) {
            ("directory", "create") => fs::create_dir_all(&path)?,
            ("config", "create") => write_json(&path, &default_config())?,
            ("gitignore", "append") => append_gitignore(root)?,
            ("skill", "create") => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(path, skill_body_for_path(&change.path))?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn skill_body_for_path(path: &str) -> &'static str {
    if path.contains(FIRST_RUN_MAPPING_SKILL_NAME) {
        FIRST_RUN_MAPPING_SKILL_BODY
    } else {
        APP_NAVIGATION_SKILL_BODY
    }
}

pub fn missing_gitignore_entries(root: &Path) -> Vec<String> {
    let content = fs::read_to_string(root.join(".gitignore")).unwrap_or_default();
    let lines: Vec<_> = content.lines().map(str::trim).collect();
    GITIGNORE_ENTRIES
        .iter()
        .filter(|entry| !lines.contains(entry))
        .map(|entry| entry.to_string())
        .collect()
}

fn append_gitignore(root: &Path) -> Result<()> {
    let path = root.join(".gitignore");
    let mut content = fs::read_to_string(&path).unwrap_or_default();
    let missing = missing_gitignore_entries(root);
    if missing.is_empty() {
        return Ok(());
    }
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    if !content.trim().is_empty() {
        content.push('\n');
    }
    content.push_str("# Atlas runtime artifacts\n");
    for entry in missing {
        content.push_str(&entry);
        content.push('\n');
    }
    fs::write(path, content)?;
    Ok(())
}

pub fn load_context(root: &Path) -> GraphContext {
    let value = read_json(&root.join(".atlas/config.json")).unwrap_or_else(|_| default_config());
    let context = value
        .get("context")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    GraphContext(context)
}

pub struct Graph {
    pub screens: BTreeMap<String, ScreenNode>,
    pub edges: BTreeMap<String, NavigationEdge>,
    pub routes: BTreeMap<String, Route>,
}

pub fn load_graph(root: &Path) -> Result<Graph> {
    Ok(Graph {
        screens: load_objects(root.join(".atlas/graph/screens"), SCREEN_SCHEMA_VERSION)?,
        edges: load_objects(root.join(".atlas/graph/edges"), EDGE_SCHEMA_VERSION)?,
        routes: load_objects(root.join(".atlas/routes"), ROUTE_SCHEMA_VERSION)?,
    })
}

fn load_objects<T>(dir: PathBuf, schema: &str) -> Result<BTreeMap<String, T>>
where
    T: for<'de> serde::Deserialize<'de>,
    T: HasObjectId,
{
    let mut objects = BTreeMap::new();
    if !dir.exists() {
        return Ok(objects);
    }
    for entry in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let value = read_json(&path)?;
        let actual = value.get("schema_version").and_then(Value::as_str);
        if actual != Some(schema) {
            anyhow::bail!(
                "{} has unsupported schema_version {actual:?}",
                path.display()
            );
        }
        let object: T = serde_json::from_value(value)?;
        objects.insert(object.object_id(), object);
    }
    Ok(objects)
}

pub trait HasObjectId {
    fn object_id(&self) -> String;
}

impl HasObjectId for ScreenNode {
    fn object_id(&self) -> String {
        self.id.clone()
    }
}

impl HasObjectId for NavigationEdge {
    fn object_id(&self) -> String {
        self.id.clone()
    }
}

impl HasObjectId for Route {
    fn object_id(&self) -> String {
        self.name.clone()
    }
}

pub fn read_json(path: &Path) -> Result<Value> {
    let content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(serde_json::from_str(&content)?)
}

pub fn write_json(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, canonical_json(value))?;
    Ok(())
}

pub fn proposal_path(root: &Path, id: &str) -> PathBuf {
    root.join(".atlas/proposals")
        .join(format!("{}.json", slugify(id)))
}

pub fn stage_proposal_value(root: &Path, proposal: &Value) -> Result<PathBuf> {
    let id = proposal
        .get("id")
        .and_then(Value::as_str)
        .context("proposal must include id")?;
    let path = proposal_path(root, id);
    write_json(&path, proposal)?;
    Ok(path)
}

pub fn accept_proposal(root: &Path, id: &str) -> Result<Vec<PathBuf>> {
    let value = read_json(&proposal_path(root, id))?;
    let proposal: Proposal = serde_json::from_value(value)?;
    if proposal.schema_version != PROPOSAL_SCHEMA_VERSION {
        anyhow::bail!(
            "unsupported proposal schema_version {}",
            proposal.schema_version
        );
    }
    let mut written = Vec::new();
    for change in proposal.changes {
        let object = change
            .get("object")
            .context("proposal change must include object")?;
        let schema = object.get("schema_version").and_then(Value::as_str);
        let path = match schema {
            Some(SCREEN_SCHEMA_VERSION) => {
                let screen: ScreenNode = serde_json::from_value(object.clone())?;
                root.join(".atlas/graph/screens")
                    .join(format!("{}.json", screen_filename(&screen.id)))
            }
            Some(EDGE_SCHEMA_VERSION) => {
                let edge: NavigationEdge = serde_json::from_value(object.clone())?;
                root.join(".atlas/graph/edges")
                    .join(format!("{}.json", edge_filename(&edge.id)))
            }
            Some(ROUTE_SCHEMA_VERSION) => {
                let route: Route = serde_json::from_value(object.clone())?;
                root.join(".atlas/routes")
                    .join(format!("{}.atlas.json", slugify(&route.name)))
            }
            other => anyhow::bail!("unsupported proposal object schema_version {other:?}"),
        };
        write_json(&path, object)?;
        written.push(path);
    }
    Ok(written)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationMetadata {
    pub schema_version: String,
    pub run_id: String,
    pub name: String,
    pub path: String,
    pub status: String,
    pub started_at: String,
    #[serde(default)]
    pub stopped_at: Option<String>,
}

pub fn observe_start(root: &Path, name: &str) -> Result<ObservationMetadata> {
    let runs_dir = root.join(".atlas/runs");
    fs::create_dir_all(&runs_dir)?;
    let timestamp = unix_timestamp();
    let run_id = format!("{timestamp}-{}", slugify(name));
    let run_path = runs_dir.join(&run_id);
    fs::create_dir_all(run_path.join("raw-layouts"))?;
    fs::create_dir_all(run_path.join("layout-deltas"))?;
    fs::create_dir_all(run_path.join("screenshots"))?;
    let metadata = ObservationMetadata {
        schema_version: "atlas.observation_run.v1".to_string(),
        run_id: run_id.clone(),
        name: name.to_string(),
        path: run_path.display().to_string(),
        status: "running".to_string(),
        started_at: timestamp.to_string(),
        stopped_at: None,
    };
    write_json(
        &run_path.join("metadata.json"),
        &serde_json::to_value(&metadata)?,
    )?;
    write_json(&run_path.join("actions.json"), &Value::Array(vec![]))?;
    write_json(&run_path.join("observations.json"), &Value::Array(vec![]))?;
    write_json(
        &runs_dir.join("current.json"),
        &json!({"schema_version": "atlas.current_run.v1", "run_id": run_id}),
    )?;
    Ok(metadata)
}

pub fn observe_current(root: &Path) -> Result<Option<ObservationMetadata>> {
    let current_path = root.join(".atlas/runs/current.json");
    if !current_path.exists() {
        return Ok(None);
    }
    let current = read_json(&current_path)?;
    let Some(run_id) = current.get("run_id").and_then(Value::as_str) else {
        return Ok(None);
    };
    let metadata_path = root.join(".atlas/runs").join(run_id).join("metadata.json");
    if !metadata_path.exists() {
        return Ok(None);
    }
    let metadata: ObservationMetadata = serde_json::from_value(read_json(&metadata_path)?)?;
    if metadata.status == "running" {
        Ok(Some(metadata))
    } else {
        Ok(None)
    }
}

pub fn observe_current_or_latest(root: &Path) -> Result<Option<ObservationMetadata>> {
    if let Some(current) = observe_current(root)? {
        return Ok(Some(current));
    }
    let runs_dir = root.join(".atlas/runs");
    if !runs_dir.exists() {
        return Ok(None);
    }
    let mut metadata = Vec::new();
    for entry in fs::read_dir(runs_dir)? {
        let path = entry?.path();
        let metadata_path = path.join("metadata.json");
        if metadata_path.exists() {
            let parsed: ObservationMetadata = serde_json::from_value(read_json(&metadata_path)?)?;
            metadata.push(parsed);
        }
    }
    metadata.sort_by(|left, right| left.started_at.cmp(&right.started_at));
    Ok(metadata.pop())
}

pub fn observe_stop(root: &Path) -> Result<ObservationMetadata> {
    let Some(mut metadata) = observe_current(root)? else {
        anyhow::bail!("No current observation run");
    };
    metadata.status = "stopped".to_string();
    metadata.stopped_at = Some(unix_timestamp().to_string());
    let metadata_path = PathBuf::from(&metadata.path).join("metadata.json");
    write_json(&metadata_path, &serde_json::to_value(&metadata)?)?;
    let _ = fs::remove_file(root.join(".atlas/runs/current.json"));
    Ok(metadata)
}

pub fn record_observation_event(root: &Path, kind: &str, payload: Value) -> Result<()> {
    let Some(metadata) = observe_current(root)? else {
        return Ok(());
    };
    append_event(
        &PathBuf::from(metadata.path).join("observations.json"),
        kind,
        payload,
    )
}

pub fn record_action_event(
    root: &Path,
    kind: &str,
    payload: Value,
    reason: Option<&str>,
) -> Result<()> {
    let Some(metadata) = observe_current(root)? else {
        return Ok(());
    };
    let mut event_payload = payload;
    if let Some(reason) = reason {
        event_payload["reason"] = Value::String(reason.to_string());
    }
    append_event(
        &PathBuf::from(metadata.path).join("actions.json"),
        kind,
        event_payload,
    )
}

fn append_event(path: &Path, kind: &str, payload: Value) -> Result<()> {
    let mut values = if path.exists() {
        read_json(path)?.as_array().cloned().unwrap_or_default()
    } else {
        vec![]
    };
    values.push(json!({
        "schema_version": "atlas.observation_event.v1",
        "timestamp": unix_timestamp().to_string(),
        "kind": kind,
        "payload": payload
    }));
    write_json(path, &Value::Array(values))
}

pub fn stage_observation_review_proposal(
    root: &Path,
) -> Result<(String, PathBuf, ObservationMetadata)> {
    let Some(metadata) = observe_current_or_latest(root)? else {
        anyhow::bail!("No observation run available");
    };
    let proposal_id = format!("proposal-{}", metadata.run_id);
    let proposal = json!({
        "schema_version": PROPOSAL_SCHEMA_VERSION,
        "id": proposal_id,
        "kind": "observation_run_review",
        "reason": "Review this observation run and convert stable navigation facts into graph objects.",
        "changes": []
    });
    let path = proposal_path(root, proposal_id.as_str());
    write_json(&path, &proposal)?;
    Ok((proposal_id, path, metadata))
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches(&['_', '-', '.'][..]).to_string();
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed
    }
}

fn screen_filename(id: &str) -> String {
    let slug = slugify(id);
    if slug.starts_with("screen_") {
        slug
    } else {
        format!("screen_{slug}")
    }
}

fn edge_filename(id: &str) -> String {
    let slug = slugify(id);
    if slug.starts_with("edge_") {
        slug
    } else {
        format!("edge_{slug}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_dry_run_does_not_write() {
        let temp = tempfile::tempdir().unwrap();
        let result = run_init(temp.path(), true, "all").unwrap();
        assert!(result.ok);
        assert!(!temp.path().join(".atlas").exists());
        assert!(result
            .skill_paths
            .contains(&".agents/skills/atlas-app-navigation/SKILL.md".to_string()));
        assert!(result
            .skill_paths
            .contains(&".agents/skills/atlas-first-run-mapping/SKILL.md".to_string()));
    }

    #[test]
    fn init_is_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        run_init(temp.path(), false, "codex").unwrap();
        let second = run_init(temp.path(), false, "codex").unwrap();
        assert!(second
            .changes
            .iter()
            .any(|change| change.path == ".atlas/config.json" && change.status == "exists"));
        let gitignore = fs::read_to_string(temp.path().join(".gitignore")).unwrap();
        assert_eq!(gitignore.matches(".atlas/runs/").count(), 1);
    }

    #[test]
    fn init_installs_separate_first_run_mapping_skill_guidance() {
        let temp = tempfile::tempdir().unwrap();
        run_init(temp.path(), false, "codex").unwrap();
        let skill = fs::read_to_string(
            temp.path()
                .join(".agents/skills/atlas-first-run-mapping/SKILL.md"),
        )
        .unwrap();
        assert!(skill.contains("name: atlas-first-run-mapping"));
        assert!(skill.contains("First-Run Mapping Mode"));
        assert!(skill.contains("token-intensive"));
        assert!(skill.contains("do not accept or commit"));
        assert!(skill.contains("atlas map --discover <route-name> --max-actions 5 --stage"));
        assert!(skill.contains("Use this skill one time"));

        let navigation_skill = fs::read_to_string(
            temp.path()
                .join(".agents/skills/atlas-app-navigation/SKILL.md"),
        )
        .unwrap();
        assert!(navigation_skill.contains("name: atlas-app-navigation"));
        assert!(!navigation_skill.contains("First-Run Mapping Mode"));
    }
}
