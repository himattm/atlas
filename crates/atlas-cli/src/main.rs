use anyhow::Result;
use atlas_android::{
    layout_result, tap_label_result, tap_point_result, tap_selector_result, Adb, AndroidCli,
    CommandRunner, SubprocessRunner,
};
use atlas_core::{identity_hash, match_screen, normalize_layout};
use atlas_graph::{exit_code_for_status, resolve_route};
use atlas_repo::{
    accept_proposal, load_context, load_graph, observe_current_or_latest, observe_start,
    observe_stop, read_json, record_action_event, record_observation_event, run_init,
    stage_observation_review_proposal, stage_proposal_value,
};
use atlas_schemas::NavigationEdge;
use atlas_schemas::{canonical_json, AtlasResult, RESULT_SCHEMA_VERSION};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(name = "atlas")]
#[command(
    about = "Shared navigation memory and soft validation for AI agents working in Android codebases."
)]
struct Cli {
    #[arg(long)]
    json: bool,
    #[arg(long)]
    quiet: bool,
    #[arg(long = "no-color")]
    no_color: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Init {
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value = "auto")]
        agents: String,
    },
    Doctor,
    Layout {
        #[arg(long)]
        diff: bool,
    },
    Tap {
        #[arg(long)]
        selector: Option<String>,
        #[arg(long)]
        point: Option<String>,
        #[arg(long)]
        label: Option<i64>,
        #[arg(long)]
        screenshot: Option<String>,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long, default_value = "verified")]
        mode: String,
    },
    Observe {
        #[command(subcommand)]
        command: ObserveCommand,
    },
    Learn {
        #[arg(long)]
        from_current_run: bool,
        #[arg(long)]
        stage: bool,
    },
    Route {
        target: String,
        #[arg(long)]
        current_screen: Option<String>,
    },
    Go {
        target: String,
        #[arg(long)]
        current_screen: Option<String>,
        #[arg(long, default_value = "verified")]
        mode: String,
    },
    Check {
        #[arg(long)]
        current: bool,
        screen: Option<String>,
    },
    Drift,
    Validate {
        #[arg(long)]
        all: bool,
        #[arg(long = "changed-files")]
        changed_files: Option<String>,
    },
    Repair {
        target: String,
        #[arg(long)]
        stage: bool,
    },
    Accept {
        proposal_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum ObserveCommand {
    Start { name: String },
    Stop,
}

fn main() {
    let cli = Cli::parse();
    let code = match run(cli) {
        Ok(code) => code,
        Err(error) => {
            let result = AtlasResult {
                schema_version: RESULT_SCHEMA_VERSION.to_string(),
                status: "config_error".to_string(),
                summary: Some(error.to_string()),
                data: json!({ "error": { "code": "atlas_error", "message": error.to_string() } }),
                recommended_action: None,
            };
            print_json(&serde_json::to_value(result).expect("error JSON"));
            7
        }
    };
    std::process::exit(code);
}

fn run(cli: Cli) -> Result<i32> {
    let root = PathBuf::from(".");
    match cli.command {
        Commands::Init { dry_run, agents } => {
            let result = run_init(&root, dry_run, &agents)?;
            print_json(&serde_json::to_value(result)?);
            Ok(0)
        }
        Commands::Doctor => {
            let result = doctor(&root);
            let ok = result
                .get("ok")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            print_json(&result);
            Ok(if ok { 0 } else { 1 })
        }
        Commands::Layout { diff } => {
            let mut android = AndroidCli::new(SubprocessRunner);
            let result = layout_result(&mut android, diff)?;
            let kind = if diff {
                "android_layout_diff"
            } else {
                "android_layout"
            };
            record_observation_event(&root, kind, result["layout"].clone())?;
            print_json(&result);
            Ok(0)
        }
        Commands::Tap {
            selector,
            point,
            label,
            screenshot,
            reason,
            mode,
        } => {
            let mut android = AndroidCli::new(SubprocessRunner);
            let mut adb = Adb::new(SubprocessRunner);
            let result = if let Some(selector) = selector {
                tap_selector_result(&mut android, &mut adb, &selector, reason.as_deref())?
            } else if let Some(point) = point {
                let (x, y) = parse_point(&point)?;
                tap_point_result(&mut adb, x, y, &mode)?
            } else if let Some(label) = label {
                let screenshot = screenshot
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("--label requires --screenshot"))?;
                tap_label_result(&mut android, &mut adb, label, screenshot)?
            } else {
                anyhow::bail!("tap requires --selector, --point, or --label");
            };
            record_action_event(&root, "tap", result["action"].clone(), reason.as_deref())?;
            print_json(&result);
            Ok(0)
        }
        Commands::Observe { command } => match command {
            ObserveCommand::Start { name } => {
                let run = observe_start(&root, &name)?;
                print_json(&json!({
                    "schema_version": RESULT_SCHEMA_VERSION,
                    "status": "ok",
                    "summary": "observation started",
                    "run": run
                }));
                Ok(0)
            }
            ObserveCommand::Stop => {
                let run = observe_stop(&root)?;
                print_json(&json!({
                    "schema_version": RESULT_SCHEMA_VERSION,
                    "status": "ok",
                    "summary": "observation stopped",
                    "run": run
                }));
                Ok(0)
            }
        },
        Commands::Learn {
            from_current_run,
            stage,
        } => {
            if !from_current_run || !stage {
                anyhow::bail!("learn currently requires --from-current-run --stage");
            }
            let (proposal_id, path, metadata) = stage_learned_or_review_proposal(&root)?;
            print_json(&json!({
                "schema_version": RESULT_SCHEMA_VERSION,
                "status": "changed_requires_review",
                "summary": "staged observation run review proposal",
                "proposal_id": proposal_id,
                "proposal_path": path.display().to_string(),
                "raw_artifact_paths": [metadata.path],
                "human_approval_required": true,
                "recommended_next_command": format!("atlas accept {proposal_id} --json")
            }));
            Ok(1)
        }
        Commands::Route {
            target,
            current_screen,
        } => {
            let graph = load_graph(&root)?;
            let context = load_context(&root);
            let plan = resolve_route(&graph, &target, current_screen.as_deref(), &context);
            let result = plan.to_result();
            let code = exit_code_for_status(&result.status);
            print_json(&serde_json::to_value(result)?);
            Ok(code)
        }
        Commands::Go {
            target,
            current_screen,
            mode,
        } => {
            let graph = load_graph(&root)?;
            let context = load_context(&root);
            let plan = resolve_route(&graph, &target, current_screen.as_deref(), &context);
            let result = plan.to_result();
            if result.status != "ok" {
                let code = exit_code_for_status(&result.status);
                print_json(&serde_json::to_value(result)?);
                return Ok(code);
            }
            let mut android = AndroidCli::new(SubprocessRunner);
            let mut adb = Adb::new(SubprocessRunner);
            let mut executed = Vec::new();
            let mut layout_calls_total = 0;
            let mut adb_taps_total = 0;
            for edge in &plan.edges {
                let selector = selector_for_edge(edge)?;
                let action_result = tap_selector_result(
                    &mut android,
                    &mut adb,
                    &selector,
                    edge.intent
                        .as_deref()
                        .or(edge.action.description.as_deref()),
                )?;
                layout_calls_total += action_result["metrics"]["layout_calls_total"]
                    .as_i64()
                    .unwrap_or(0);
                adb_taps_total += action_result["metrics"]["adb_taps_total"]
                    .as_i64()
                    .unwrap_or(0);
                let action = action_result["action"].clone();
                record_action_event(&root, "tap", action.clone(), edge.intent.as_deref())?;
                let verification = match_current_screen(&root, &mut android)?;
                layout_calls_total += verification["metrics"]["layout_calls_total"]
                    .as_i64()
                    .unwrap_or(0);
                let reached = verification["current_screen"]["matched_screen"]
                    .as_str()
                    .map(|screen| screen == edge.to_screen)
                    .unwrap_or(false);
                if !reached {
                    print_json(&json!({
                        "schema_version": RESULT_SCHEMA_VERSION,
                        "status": "route_broken",
                        "summary": "route edge did not reach expected screen",
                        "edge": edge.id,
                        "expected_screen": edge.to_screen,
                        "observed": verification["current_screen"],
                        "metrics": {
                            "layout_calls_total": layout_calls_total,
                            "layout_json_returned_to_agent": false,
                            "adb_taps_total": adb_taps_total
                        },
                        "recommended_action": "inspect the app and stage a graph repair proposal"
                    }));
                    return Ok(3);
                }
                executed.push(json!({
                    "edge": edge.id,
                    "selector": selector,
                    "action": action,
                    "verification": verification["current_screen"]
                }));
            }
            print_json(&json!({
                "schema_version": RESULT_SCHEMA_VERSION,
                "status": "ok",
                "summary": "route executed",
                "target": target,
                "mode": mode,
                "edge_ids": plan.edges.iter().map(|edge| edge.id.clone()).collect::<Vec<_>>(),
                "executed": executed,
                "preferred_path_used": plan.preferred_path_used,
                "graph_fallback_used": plan.graph_fallback_used,
                "estimated_layout_calls_saved": std::cmp::max(plan.edges.len(), 1),
                "metrics": {
                    "layout_calls_total": layout_calls_total,
                    "layout_json_returned_to_agent": false,
                    "adb_taps_total": adb_taps_total
                }
            }));
            Ok(0)
        }
        Commands::Check { current, screen } => {
            if current {
                let mut android = AndroidCli::new(SubprocessRunner);
                let result = match_current_screen(&root, &mut android)?;
                print_json(&json!({
                    "schema_version": RESULT_SCHEMA_VERSION,
                    "status": "ok",
                    "current_screen": result["current_screen"],
                    "metrics": result["metrics"],
                    "layout_observed": result["layout_observed"]
                }));
                Ok(0)
            } else if let Some(name) = screen {
                let graph = load_graph(&root)?;
                let found = graph.screens.values().find(|candidate| {
                    candidate.id == name
                        || candidate.name == name
                        || candidate.aliases.iter().any(|alias| alias == &name)
                });
                if let Some(screen) = found {
                    let context = load_context(&root);
                    let mismatches = context.mismatches(&screen.context_guard);
                    if mismatches.is_empty() {
                        print_json(&json!({
                            "schema_version": RESULT_SCHEMA_VERSION,
                            "status": "ok",
                            "summary": format!("screen guard check for {}", screen.name),
                            "data": {"screen": screen.name}
                        }));
                        Ok(0)
                    } else {
                        let result = atlas_schemas::AtlasResult::context_mismatch(mismatches);
                        print_json(&serde_json::to_value(result)?);
                        Ok(8)
                    }
                } else {
                    print_json(&json!({
                        "schema_version": RESULT_SCHEMA_VERSION,
                        "status": "screen_unknown",
                        "summary": format!("screen not found: {name}")
                    }));
                    Ok(5)
                }
            } else {
                let mut android = AndroidCli::new(SubprocessRunner);
                let result = match_current_screen(&root, &mut android)?;
                print_json(&json!({
                    "schema_version": RESULT_SCHEMA_VERSION,
                    "status": "ok",
                    "current_screen": result["current_screen"],
                    "metrics": result["metrics"],
                    "layout_observed": result["layout_observed"]
                }));
                Ok(0)
            }
        }
        Commands::Drift => {
            let mut android = AndroidCli::new(SubprocessRunner);
            let result = drift_result(&root, &mut android)?;
            let code = exit_code_for_status(result["status"].as_str().unwrap_or("ok"));
            print_json(&result);
            Ok(code)
        }
        Commands::Validate { all, changed_files } => {
            let mut android = AndroidCli::new(SubprocessRunner);
            let result = validate_result(&root, &mut android, all, changed_files.as_deref())?;
            let code = exit_code_for_status(result["status"].as_str().unwrap_or("ok"));
            print_json(&result);
            Ok(code)
        }
        Commands::Repair { target, stage } => {
            if !stage {
                anyhow::bail!("repair currently requires --stage");
            }
            let proposal = json!({
                "schema_version": "atlas.proposal.v1",
                "id": format!("proposal-repair-{}", target.replace('/', "_")),
                "kind": "selector_drift",
                "reason": format!("Review and repair Atlas graph object {target}."),
                "changes": []
            });
            let path = stage_proposal_value(&root, &proposal)?;
            print_json(&json!({
                "schema_version": RESULT_SCHEMA_VERSION,
                "status": "changed_requires_review",
                "summary": "staged repair proposal",
                "proposal_id": proposal["id"],
                "proposal_path": path.display().to_string(),
                "human_approval_required": true
            }));
            Ok(1)
        }
        Commands::Accept { proposal_id } => {
            let written = accept_proposal(&root, &proposal_id)?;
            print_json(&json!({
                "schema_version": RESULT_SCHEMA_VERSION,
                "status": "ok",
                "summary": "proposal accepted",
                "graph_objects_touched": written.iter().map(|path| path.display().to_string()).collect::<Vec<_>>()
            }));
            Ok(0)
        }
    }
}

fn parse_point(point: &str) -> Result<(i64, i64)> {
    let (x, y) = point
        .split_once(',')
        .ok_or_else(|| anyhow::anyhow!("--point must be X,Y"))?;
    Ok((x.trim().parse()?, y.trim().parse()?))
}

fn stage_learned_or_review_proposal(
    root: &Path,
) -> Result<(String, PathBuf, atlas_repo::ObservationMetadata)> {
    let Some(metadata) = observe_current_or_latest(root)? else {
        anyhow::bail!("No observation run available");
    };
    if let Some(proposal) = learned_proposal_for_run(&metadata)? {
        let proposal_id = proposal["id"]
            .as_str()
            .unwrap_or("proposal-learned")
            .to_string();
        let path = stage_proposal_value(root, &proposal)?;
        Ok((proposal_id, path, metadata))
    } else {
        stage_observation_review_proposal(root)
    }
}

fn learned_proposal_for_run(
    metadata: &atlas_repo::ObservationMetadata,
) -> Result<Option<serde_json::Value>> {
    let run_path = PathBuf::from(&metadata.path);
    let observations = read_json(&run_path.join("observations.json"))?;
    let actions = read_json(&run_path.join("actions.json"))?;
    let observation_values = observations.as_array().cloned().unwrap_or_default();
    let action_values = actions.as_array().cloned().unwrap_or_default();
    if observation_values.len() < 2 || action_values.is_empty() {
        return Ok(None);
    }
    let before_layout = observation_values[0]["payload"].clone();
    let after_layout = observation_values[observation_values.len() - 1]["payload"].clone();
    let action = action_values[0]["payload"].clone();
    let Some(selector) = action["selector"].as_str() else {
        return Ok(None);
    };
    let (selector_kind, selector_value) = selector
        .split_once('=')
        .map(|(kind, value)| (kind.to_string(), value.to_string()))
        .unwrap_or_else(|| ("selector".to_string(), selector.to_string()));
    let run_slug = slug_for_id(&metadata.name);
    let start_screen_id = format!("screen_{}_start", run_slug);
    let target_screen_id = format!("screen_{}", run_slug);
    let edge_id = format!("edge_{}_start__{}", run_slug, run_slug);
    let before_normalized = normalize_layout(&before_layout);
    let after_normalized = normalize_layout(&after_layout);
    let start_screen = json!({
        "schema_version": "atlas.screen.v1",
        "id": start_screen_id,
        "name": format!("{}-start", run_slug),
        "identity_hash": identity_hash(&before_normalized),
        "normalized": before_normalized,
        "aliases": []
    });
    let target_screen = json!({
        "schema_version": "atlas.screen.v1",
        "id": target_screen_id,
        "name": metadata.name,
        "identity_hash": identity_hash(&after_normalized),
        "normalized": after_normalized,
        "aliases": []
    });
    let edge = json!({
        "schema_version": "atlas.edge.v1",
        "id": edge_id,
        "from_screen": start_screen["name"],
        "to_screen": target_screen["name"],
        "intent": format!("navigate to {}", metadata.name),
        "action": {
            "kind": "tap",
            "description": action["reason"].as_str().unwrap_or("learned tap"),
            "selector_candidates": [{
                "kind": selector_kind,
                "value": selector_value,
                "score": 0.7
            }]
        },
        "expectations": [{"kind": "screen_reached", "screen": target_screen["name"]}],
        "learned_from": {"source": "observation_run", "run_id": metadata.run_id}
    });
    let route = json!({
        "schema_version": "atlas.route.v1",
        "name": metadata.name,
        "start": {"screen": start_screen["name"]},
        "target": {"screen": target_screen["name"]},
        "preferred_edge_ids": [edge["id"]],
        "allow_graph_fallback": true
    });
    Ok(Some(json!({
        "schema_version": "atlas.proposal.v1",
        "id": format!("proposal-learn-{}", metadata.run_id),
        "kind": "learned_route",
        "reason": "Learn screen, edge, and route objects from an observation run.",
        "changes": [
            {"op": "add", "object": start_screen},
            {"op": "add", "object": target_screen},
            {"op": "add", "object": edge},
            {"op": "add", "object": route}
        ]
    })))
}

fn slug_for_id(value: &str) -> String {
    let slug: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    slug.trim_matches('_').to_string()
}

fn match_current_screen<R: CommandRunner>(
    root: &Path,
    android: &mut AndroidCli<R>,
) -> Result<serde_json::Value> {
    let layout = layout_result(android, false)?;
    let graph = load_graph(root)?;
    let normalized = normalize_layout(&layout["layout"]);
    let identity = identity_hash(&normalized);
    let screen_match = match_screen(&normalized, graph.screens.into_values());
    Ok(json!({
        "current_screen": {
            "status": screen_match.status,
            "matched_screen": screen_match.matched_screen,
            "match_confidence": screen_match.match_confidence,
            "hash_matched": screen_match.hash_matched,
            "identity_hash": identity
        },
        "metrics": {
            "layout_calls_total": 1,
            "layout_json_returned_to_agent": false,
            "adb_taps_total": 0
        },
        "layout_observed": layout["layout"].is_object()
    }))
}

fn drift_result<R: CommandRunner>(
    root: &Path,
    android: &mut AndroidCli<R>,
) -> Result<serde_json::Value> {
    let current = match_current_screen(root, android)?;
    let current_screen = &current["current_screen"];
    let status = match current_screen["status"]
        .as_str()
        .unwrap_or("screen_unknown")
    {
        "matched" => {
            let graph = load_graph(root)?;
            let context = load_context(root);
            let matched = current_screen["matched_screen"].as_str();
            let mismatches = matched
                .and_then(|screen_name| {
                    graph
                        .screens
                        .values()
                        .find(|screen| screen.name == screen_name)
                        .map(|screen| context.mismatches(&screen.context_guard))
                })
                .unwrap_or_default();
            if mismatches.is_empty() {
                "passed"
            } else {
                "context_mismatch"
            }
        }
        "repair_candidate" => "selector_drift",
        _ => "screen_unknown",
    };
    let mut result = json!({
        "schema_version": RESULT_SCHEMA_VERSION,
        "status": status,
        "summary": match status {
            "passed" => "current app state matches committed graph",
            "context_mismatch" => "current context does not satisfy matched screen guard",
            "selector_drift" => "current screen is similar but below match threshold",
            _ => "current app state is not known in committed graph"
        },
        "current_screen": current_screen,
        "metrics": current["metrics"],
        "human_approval_required": false
    });
    if matches!(status, "selector_drift" | "screen_unknown") {
        let proposal = json!({
            "schema_version": "atlas.proposal.v1",
            "id": format!("proposal-drift-{}", current_screen["identity_hash"].as_str().unwrap_or("unknown").replace(':', "_")),
            "kind": status,
            "reason": "Review current app state drift against committed Atlas graph.",
            "changes": []
        });
        let path = stage_proposal_value(root, &proposal)?;
        result["proposal_id"] = proposal["id"].clone();
        result["proposal_path"] = json!(path.display().to_string());
        result["human_approval_required"] = json!(true);
    }
    Ok(result)
}

fn validate_result<R: CommandRunner>(
    root: &Path,
    android: &mut AndroidCli<R>,
    all: bool,
    changed_files: Option<&str>,
) -> Result<serde_json::Value> {
    let drift = drift_result(root, android)?;
    let selected_routes = selected_routes_for_validation(root, all, changed_files)?;
    let status = drift["status"].as_str().unwrap_or("passed");
    Ok(json!({
        "schema_version": RESULT_SCHEMA_VERSION,
        "status": status,
        "summary": if status == "passed" { "validation passed" } else { "validation found graph drift" },
        "drift": drift,
        "selected_routes": selected_routes,
        "impact_analysis": {
            "mode": if all { "all" } else if changed_files.is_some() { "changed_files" } else { "current" },
            "precise": false
        }
    }))
}

fn selected_routes_for_validation(
    root: &Path,
    all: bool,
    changed_files: Option<&str>,
) -> Result<Vec<String>> {
    let graph = load_graph(root)?;
    if all {
        return Ok(graph.routes.keys().cloned().collect());
    }
    let Some(path) = changed_files else {
        return Ok(Vec::new());
    };
    let changed = std::fs::read_to_string(path).unwrap_or_default();
    let mut selected = Vec::new();
    for route in graph.routes.values() {
        let route_text = serde_json::to_string(&route.triggers).unwrap_or_default();
        if changed
            .lines()
            .any(|line| !line.is_empty() && route_text.contains(line))
        {
            selected.push(route.name.clone());
        }
    }
    Ok(selected)
}

fn selector_for_edge(edge: &NavigationEdge) -> Result<String> {
    let candidate = edge
        .action
        .selector_candidates
        .iter()
        .filter(|candidate| candidate.value.is_some())
        .max_by(|left, right| {
            left.score
                .unwrap_or(0.0)
                .partial_cmp(&right.score.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or_else(|| anyhow::anyhow!("edge {} has no executable selector candidate", edge.id))?;
    let value = candidate.value.as_deref().unwrap_or_default();
    let key = match candidate.kind.as_str() {
        "visible_text" | "visible_text_fuzzy" => "text",
        "accessibility" | "accessibility_or_semantic" => "content_description",
        "resource_id" => "resource_id",
        "test_tag" => "test_tag",
        other => other,
    };
    Ok(format!("{key}={value}"))
}

fn doctor(root: &std::path::Path) -> serde_json::Value {
    let config_exists = root.join(".atlas/config.json").exists();
    let graph_dirs_exist = [
        ".atlas/graph/screens",
        ".atlas/graph/edges",
        ".atlas/routes",
        ".atlas/checks",
        ".atlas/proposals",
        ".atlas/runs",
        ".atlas/state",
    ]
    .iter()
    .all(|path| root.join(path).is_dir());
    let checks = vec![
        json!({"name": "config", "status": if config_exists { "pass" } else { "fail" }}),
        json!({"name": "graph_dirs", "status": if graph_dirs_exist { "pass" } else { "fail" }}),
        json!({"name": "android_cli", "status": if command_on_path("android") { "pass" } else { "warn" }}),
        json!({"name": "adb", "status": if command_on_path("adb") { "pass" } else { "warn" }}),
    ];
    let fail = checks
        .iter()
        .filter(|check| check["status"] == "fail")
        .count();
    let warn = checks
        .iter()
        .filter(|check| check["status"] == "warn")
        .count();
    let pass = checks
        .iter()
        .filter(|check| check["status"] == "pass")
        .count();
    json!({
        "ok": fail == 0,
        "root": root.canonicalize().unwrap_or_else(|_| root.to_path_buf()).display().to_string(),
        "summary": { "pass": pass, "warn": warn, "fail": fail },
        "checks": checks
    })
}

fn command_on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|path| path.join(name).exists()))
        .unwrap_or(false)
}

fn print_json(value: &serde_json::Value) {
    print!("{}", canonical_json(value));
}
