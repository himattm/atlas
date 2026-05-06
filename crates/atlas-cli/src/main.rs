use anyhow::Result;
use atlas_android::{
    layout_result, tap_label_result, tap_point_result, tap_selector_result, Adb, AndroidCli,
    SubprocessRunner,
};
use atlas_core::{identity_hash, match_screen, normalize_layout};
use atlas_graph::{exit_code_for_status, resolve_route};
use atlas_repo::{
    accept_proposal, load_context, load_graph, observe_start, observe_stop, record_action_event,
    record_observation_event, run_init, stage_observation_review_proposal,
};
use atlas_schemas::NavigationEdge;
use atlas_schemas::{canonical_json, AtlasResult, RESULT_SCHEMA_VERSION};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "atlas-rs")]
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
            let (proposal_id, path, metadata) = stage_observation_review_proposal(&root)?;
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
                executed.push(json!({
                    "edge": edge.id,
                    "selector": selector,
                    "action": action
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
            if current || screen.is_none() {
                let mut android = AndroidCli::new(SubprocessRunner);
                let layout = layout_result(&mut android, false)?;
                let graph = load_graph(&root)?;
                let normalized = normalize_layout(&layout["layout"]);
                let identity = identity_hash(&normalized);
                let screen_match = match_screen(&normalized, graph.screens.into_values());
                print_json(&json!({
                    "schema_version": RESULT_SCHEMA_VERSION,
                    "status": "ok",
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
                }));
                Ok(0)
            } else {
                let graph = load_graph(&root)?;
                let name = screen.unwrap();
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
            }
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
