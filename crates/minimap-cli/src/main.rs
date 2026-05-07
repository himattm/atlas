use anyhow::Result;
use clap::{Parser, Subcommand};
use minimap_android::{
    layout_result, tap_label_result, tap_point_result, tap_selector_result, Adb, AndroidCli,
    CommandRunner, SubprocessRunner,
};
use minimap_core::{identity_hash, match_screen, normalize_layout};
use minimap_graph::{exit_code_for_status, resolve_route};
use minimap_repo::{
    accept_proposal, load_context, load_graph, observe_current, observe_current_or_latest,
    observe_start, observe_stop, read_json, record_action_event, record_observation_event,
    run_init, stage_observation_review_proposal, stage_proposal_value,
};
use minimap_schemas::NavigationEdge;
use minimap_schemas::{canonical_json, MinimapResult, RESULT_SCHEMA_VERSION};
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(name = "minimap")]
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
    /// Initialize Minimap in this repo (creates .minimap/ layout and agent skills).
    Init {
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value = "auto")]
        agents: String,
    },
    /// Diagnose the Minimap environment (config, graph dirs, android/adb on PATH).
    Doctor,
    /// Capture the current Android UI as redacted layout JSON; pass --diff for an in-session diff.
    Layout {
        #[arg(long)]
        diff: bool,
    },
    /// Tap a UI element by --selector kind=value, --point X,Y, or --label N (with --screenshot); pass --reason to record intent.
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
    /// Start or stop a named observation run that records layouts and taps under .minimap/runs/.
    Observe {
        #[command(subcommand)]
        command: ObserveCommand,
    },
    /// Stage a learned graph proposal (screens, edges, route) from the current observation run; requires --from-current-run --stage.
    Learn {
        #[arg(long)]
        from_current_run: bool,
        #[arg(long)]
        stage: bool,
    },
    /// Resolve the planned navigation path to a screen or route from the current screen (no device action).
    Route {
        target: String,
        #[arg(long)]
        current_screen: Option<String>,
    },
    /// Resolve and execute a route to the target, verifying each step and aborting on drift.
    Go {
        target: String,
        #[arg(long)]
        current_screen: Option<String>,
        #[arg(long, default_value = "verified")]
        mode: String,
    },
    /// Match the current Android screen against the committed graph, or check a named screen's context guard.
    Check {
        #[arg(long)]
        current: bool,
        screen: Option<String>,
    },
    /// Compare the current app state to the committed graph; stages a review proposal if drifted.
    Drift,
    /// Validate routes against the live device; --all, --changed-files <path>, or --execute --current-screen <name> for live runs.
    Validate {
        #[arg(long)]
        all: bool,
        #[arg(long = "changed-files")]
        changed_files: Option<String>,
        #[arg(long)]
        execute: bool,
        #[arg(long)]
        current_screen: Option<String>,
    },
    /// Stage a proposal to repair a drifted graph object (selector, screen, edge); requires --stage.
    Repair {
        target: String,
        #[arg(long)]
        stage: bool,
    },
    /// Bounded first-run discovery loop; requires --discover <name> --stage. Pass --finish to stop the run and stage the learned proposal.
    Map {
        #[arg(long)]
        discover: Option<String>,
        #[arg(long = "max-actions", default_value_t = 10)]
        max_actions: usize,
        #[arg(long)]
        stage: bool,
        #[arg(long)]
        finish: bool,
    },
    /// Accept a staged proposal by id; the only command that mutates the committed graph.
    Accept {
        proposal_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum ObserveCommand {
    /// Begin a named observation run; subsequent layout and tap calls record into it.
    Start { name: String },
    /// Stop the active observation run and finalize its artifacts under .minimap/runs/.
    Stop,
}

fn main() {
    let cli = Cli::parse();
    let code = match run(cli) {
        Ok(code) => code,
        Err(error) => {
            let result = MinimapResult {
                schema_version: RESULT_SCHEMA_VERSION.to_string(),
                status: "config_error".to_string(),
                summary: Some(error.to_string()),
                data: json!({ "error": { "code": "minimap_error", "message": error.to_string() } }),
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
                "recommended_next_command": format!("minimap accept {proposal_id} --json")
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
            let result = execute_route_plan(&root, &plan, &target, &mode, &mut android, &mut adb)?;
            let code = exit_code_for_status(result["status"].as_str().unwrap_or("ok"));
            print_json(&result);
            Ok(code)
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
                        let result = minimap_schemas::MinimapResult::context_mismatch(mismatches);
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
        Commands::Validate {
            all,
            changed_files,
            execute,
            current_screen,
        } => {
            let mut android = AndroidCli::new(SubprocessRunner);
            let mut adb = Adb::new(SubprocessRunner);
            let result = validate_result(
                &root,
                &mut android,
                &mut adb,
                all,
                changed_files.as_deref(),
                execute,
                current_screen.as_deref(),
            )?;
            let code = exit_code_for_status(result["status"].as_str().unwrap_or("ok"));
            print_json(&result);
            Ok(code)
        }
        Commands::Repair { target, stage } => {
            if !stage {
                anyhow::bail!("repair currently requires --stage");
            }
            let proposal = json!({
                "schema_version": "minimap.proposal.v1",
                "id": format!("proposal-repair-{}", target.replace('/', "_")),
                "kind": "selector_drift",
                "reason": format!("Review and repair Minimap graph object {target}."),
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
        Commands::Map {
            discover,
            max_actions,
            stage,
            finish,
        } => {
            let target =
                discover.ok_or_else(|| anyhow::anyhow!("map requires --discover <name>"))?;
            if !stage {
                anyhow::bail!("map --discover currently requires --stage");
            }
            let result = map_discover_result(&root, &target, max_actions, finish)?;
            let code = if result["status"] == "budget_exhausted" {
                1
            } else if result["status"] == "needs_agent_action" {
                0
            } else {
                exit_code_for_status(result["status"].as_str().unwrap_or("ok"))
            };
            print_json(&result);
            Ok(code)
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
) -> Result<(String, PathBuf, minimap_repo::ObservationMetadata)> {
    let Some(metadata) = observe_current_or_latest(root)? else {
        anyhow::bail!("No observation run available");
    };
    if let Some(proposal) = learned_proposal_for_run(root, &metadata)? {
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
    root: &Path,
    metadata: &minimap_repo::ObservationMetadata,
) -> Result<Option<serde_json::Value>> {
    let run_path = PathBuf::from(&metadata.path);
    let observations = read_json(&run_path.join("observations.json"))?;
    let actions = read_json(&run_path.join("actions.json"))?;
    let observation_values = observations.as_array().cloned().unwrap_or_default();
    let action_values = actions.as_array().cloned().unwrap_or_default();
    if observation_values.len() < 2 || action_values.is_empty() {
        return Ok(None);
    }
    let transition_count = std::cmp::min(action_values.len(), observation_values.len() - 1);
    if transition_count == 0 {
        return Ok(None);
    }
    if action_values
        .iter()
        .take(transition_count)
        .any(|action| action["payload"]["selector"].as_str().is_none())
    {
        return Ok(None);
    }

    let graph = load_graph(root)?;
    let run_slug = slug_for_id(&metadata.name);
    let mut changes = Vec::new();
    let mut screen_names = Vec::new();
    for (index, observation) in observation_values
        .iter()
        .enumerate()
        .take(transition_count + 1)
    {
        let layout = observation["payload"].clone();
        let normalized = normalize_layout(&layout);
        let matched = match_screen(&normalized, graph.screens.values().cloned());
        if matched.status == "matched" {
            if let Some(name) = matched.matched_screen {
                screen_names.push(name);
                continue;
            }
        }
        let (id, name) =
            learned_screen_id_and_name(&run_slug, &metadata.name, index, transition_count);
        screen_names.push(name.clone());
        changes.push(json!({
            "op": "add",
            "object": {
                "schema_version": "minimap.screen.v1",
                "id": id,
                "name": name,
                "identity_hash": identity_hash(&normalized),
                "normalized": normalized,
                "aliases": []
            }
        }));
    }

    let mut edge_ids = Vec::new();
    for index in 0..transition_count {
        let action = action_values[index]["payload"].clone();
        let selector = action["selector"].as_str().unwrap_or_default();
        let (selector_kind, selector_value) = selector
            .split_once('=')
            .map(|(kind, value)| (kind.to_string(), value.to_string()))
            .unwrap_or_else(|| ("selector".to_string(), selector.to_string()));
        let edge_id = format!("edge_{}_{}__{}", run_slug, index, index + 1);
        edge_ids.push(edge_id.clone());
        changes.push(json!({
            "op": "add",
            "object": {
                "schema_version": "minimap.edge.v1",
                "id": edge_id,
                "from_screen": screen_names[index],
                "to_screen": screen_names[index + 1],
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
                "expectations": [{"kind": "screen_reached", "screen": screen_names[index + 1]}],
                "learned_from": {"source": "observation_run", "run_id": metadata.run_id, "step": index}
            }
        }));
    }

    let route = json!({
        "schema_version": "minimap.route.v1",
        "name": metadata.name,
        "start": {"screen": screen_names.first().cloned().unwrap_or_else(|| format!("{}-start", run_slug))},
        "target": {"screen": screen_names.last().cloned().unwrap_or_else(|| metadata.name.clone())},
        "preferred_edge_ids": edge_ids,
        "allow_graph_fallback": true
    });
    changes.push(json!({"op": "add", "object": route}));

    Ok(Some(json!({
        "schema_version": "minimap.proposal.v1",
        "id": format!("proposal-learn-{}", metadata.run_id),
        "kind": "learned_route",
        "reason": "Learn screens, edges, and route objects from an observation run.",
        "changes": changes
    })))
}

fn learned_screen_id_and_name(
    run_slug: &str,
    route_name: &str,
    index: usize,
    final_index: usize,
) -> (String, String) {
    if index == 0 {
        (
            format!("screen_{}_start", run_slug),
            format!("{}-start", run_slug),
        )
    } else if index == final_index {
        (format!("screen_{}", run_slug), route_name.to_string())
    } else {
        (
            format!("screen_{}_step_{}", run_slug, index + 1),
            format!("{}-step-{}", run_slug, index + 1),
        )
    }
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

fn execute_route_plan<AR: CommandRunner, DR: CommandRunner>(
    root: &Path,
    plan: &minimap_graph::RoutePlan,
    target: &str,
    mode: &str,
    android: &mut AndroidCli<AR>,
    adb: &mut Adb<DR>,
) -> Result<serde_json::Value> {
    let mut executed = Vec::new();
    let mut layout_calls_total = 0;
    let mut adb_taps_total = 0;
    let mut final_screen = serde_json::Value::Null;
    for edge in &plan.edges {
        let selector = selector_for_edge(edge)?;
        let action_result = tap_selector_result(
            android,
            adb,
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
        record_action_event(root, "tap", action.clone(), edge.intent.as_deref())?;
        let verification = match_current_screen(root, android)?;
        layout_calls_total += verification["metrics"]["layout_calls_total"]
            .as_i64()
            .unwrap_or(0);
        final_screen = verification["current_screen"].clone();
        let reached = final_screen["matched_screen"]
            .as_str()
            .map(|screen| screen == edge.to_screen)
            .unwrap_or(false);
        if !reached {
            return Ok(json!({
                "schema_version": RESULT_SCHEMA_VERSION,
                "status": "route_broken",
                "summary": "route edge did not reach expected screen",
                "target": target,
                "mode": mode,
                "edge": edge.id,
                "expected_screen": edge.to_screen,
                "observed": final_screen,
                "executed": executed,
                "metrics": {
                    "layout_calls_total": layout_calls_total,
                    "layout_json_returned_to_agent": false,
                    "adb_taps_total": adb_taps_total
                },
                "recommended_action": "inspect the app and stage a graph repair proposal"
            }));
        }
        executed.push(json!({
            "edge": edge.id,
            "selector": selector,
            "action": action,
            "verification": final_screen
        }));
    }
    Ok(json!({
        "schema_version": RESULT_SCHEMA_VERSION,
        "status": "ok",
        "summary": "route executed",
        "target": target,
        "mode": mode,
        "edge_ids": plan.edges.iter().map(|edge| edge.id.clone()).collect::<Vec<_>>(),
        "executed": executed,
        "final_screen": final_screen,
        "preferred_path_used": plan.preferred_path_used,
        "graph_fallback_used": plan.graph_fallback_used,
        "estimated_layout_calls_saved": std::cmp::max(plan.edges.len(), 1),
        "metrics": {
            "layout_calls_total": layout_calls_total,
            "layout_json_returned_to_agent": false,
            "adb_taps_total": adb_taps_total
        }
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
            "schema_version": "minimap.proposal.v1",
            "id": format!("proposal-drift-{}", current_screen["identity_hash"].as_str().unwrap_or("unknown").replace(':', "_")),
            "kind": status,
            "reason": "Review current app state drift against committed Minimap graph.",
            "changes": []
        });
        let path = stage_proposal_value(root, &proposal)?;
        result["proposal_id"] = proposal["id"].clone();
        result["proposal_path"] = json!(path.display().to_string());
        result["human_approval_required"] = json!(true);
    }
    Ok(result)
}

fn validate_result<AR: CommandRunner, DR: CommandRunner>(
    root: &Path,
    android: &mut AndroidCli<AR>,
    adb: &mut Adb<DR>,
    all: bool,
    changed_files: Option<&str>,
    execute: bool,
    current_screen: Option<&str>,
) -> Result<serde_json::Value> {
    let drift = drift_result(root, android)?;
    let selected_routes = selected_routes_for_validation(root, all, changed_files)?;
    let status = drift["status"].as_str().unwrap_or("passed");
    let mut result = json!({
        "schema_version": RESULT_SCHEMA_VERSION,
        "status": status,
        "summary": if status == "passed" { "validation passed" } else { "validation found graph drift" },
        "drift": drift,
        "selected_routes": selected_routes,
        "impact_analysis": {
            "mode": if all { "all" } else if changed_files.is_some() { "changed_files" } else { "current" },
            "precise": execute
        }
    });
    if !execute || status != "passed" {
        return Ok(result);
    }

    let graph = load_graph(root)?;
    let context = load_context(root);
    let mut active_screen = current_screen.map(str::to_string).or_else(|| {
        result["drift"]["current_screen"]["matched_screen"]
            .as_str()
            .map(str::to_string)
    });
    let mut route_results = Vec::new();
    let mut skipped_routes = Vec::new();
    let mut aggregate_status = "passed".to_string();

    for route_name in selected_routes_for_validation(root, all, changed_files)? {
        let Some(route) = graph.routes.get(&route_name) else {
            skipped_routes.push(json!({"route": route_name, "reason": "route_unresolved"}));
            continue;
        };
        let Some(screen) = active_screen.as_deref() else {
            skipped_routes.push(json!({"route": route_name, "reason": "current_screen_unknown"}));
            continue;
        };
        if route.start_screen() != Some(screen) {
            skipped_routes.push(json!({
                "route": route_name,
                "reason": "start_screen_mismatch",
                "expected_start": route.start_screen(),
                "current_screen": screen
            }));
            continue;
        }
        let plan = resolve_route(&graph, &route_name, Some(screen), &context);
        if !plan.context_mismatches.is_empty() {
            skipped_routes.push(json!({
                "route": route_name,
                "reason": "context_mismatch",
                "mismatches": plan.context_mismatches
            }));
            continue;
        }
        if plan.status != "ok" {
            skipped_routes.push(json!({
                "route": route_name,
                "reason": "route_unresolved",
                "status": plan.status
            }));
            continue;
        }
        let route_result = execute_route_plan(root, &plan, &route_name, "verified", android, adb)?;
        if route_result["status"] != "ok" {
            aggregate_status = route_result["status"]
                .as_str()
                .unwrap_or("route_broken")
                .to_string();
        }
        if let Some(final_screen) = route_result["final_screen"]["matched_screen"].as_str() {
            active_screen = Some(final_screen.to_string());
        }
        route_results.push(json!({
            "route": route_name,
            "result": route_result
        }));
        if aggregate_status != "passed" {
            break;
        }
    }
    result["status"] = json!(aggregate_status);
    result["summary"] = json!(if aggregate_status == "passed" {
        "validation passed"
    } else {
        "validation route execution failed"
    });
    result["route_results"] = json!(route_results);
    result["skipped_routes"] = json!(skipped_routes);
    result["final_screen"] = json!(active_screen);
    Ok(result)
}

fn map_discover_result(
    root: &Path,
    target: &str,
    max_actions: usize,
    finish: bool,
) -> Result<serde_json::Value> {
    if max_actions == 0 {
        anyhow::bail!("--max-actions must be greater than zero");
    }
    if let Some(metadata) = observe_current(root)? {
        if metadata.name != target {
            anyhow::bail!(
                "observation run {} is already active for {}",
                metadata.run_id,
                metadata.name
            );
        }
        let action_count = observation_action_count(&metadata)?;
        if finish {
            let stopped = observe_stop(root)?;
            let (proposal_id, path, _) = stage_learned_or_review_proposal(root)?;
            return Ok(json!({
                "schema_version": RESULT_SCHEMA_VERSION,
                "status": "changed_requires_review",
                "summary": "finished discovery run and staged proposal",
                "target": target,
                "run": stopped,
                "actions_recorded": action_count,
                "max_actions": max_actions,
                "proposal_id": proposal_id,
                "proposal_path": path.display().to_string(),
                "human_approval_required": true
            }));
        }
        if action_count >= max_actions {
            return Ok(json!({
                "schema_version": RESULT_SCHEMA_VERSION,
                "status": "budget_exhausted",
                "summary": "discovery action budget exhausted",
                "target": target,
                "run": metadata,
                "actions_recorded": action_count,
                "max_actions": max_actions,
                "recommended_next_command": format!("minimap map --discover {target} --max-actions {max_actions} --stage --finish")
            }));
        }
        return Ok(json!({
            "schema_version": RESULT_SCHEMA_VERSION,
            "status": "needs_agent_action",
            "summary": "continue bounded discovery with minimap layout and minimap tap",
            "target": target,
            "run": metadata,
            "actions_recorded": action_count,
            "max_actions": max_actions,
            "budget_remaining": max_actions - action_count,
            "allowed_next_commands": [
                "minimap layout",
                "minimap tap --selector \"<kind>=<value>\" --reason \"<why>\"",
                format!("minimap map --discover {target} --max-actions {max_actions} --stage --finish")
            ]
        }));
    }

    if finish {
        anyhow::bail!("no active discovery run to finish");
    }
    let metadata = observe_start(root, target)?;
    let mut android = AndroidCli::new(SubprocessRunner);
    let layout = layout_result(&mut android, false)?;
    record_observation_event(root, "android_layout", layout["layout"].clone())?;
    Ok(json!({
        "schema_version": RESULT_SCHEMA_VERSION,
        "status": "needs_agent_action",
        "summary": "started bounded discovery run",
        "target": target,
        "run": metadata,
        "actions_recorded": 0,
        "max_actions": max_actions,
        "budget_remaining": max_actions,
        "layout_observed": layout["layout"].is_object(),
        "allowed_next_commands": [
            "minimap layout",
            "minimap tap --selector \"<kind>=<value>\" --reason \"<why>\"",
            format!("minimap map --discover {target} --max-actions {max_actions} --stage --finish")
        ]
    }))
}

fn observation_action_count(metadata: &minimap_repo::ObservationMetadata) -> Result<usize> {
    let actions = read_json(&PathBuf::from(&metadata.path).join("actions.json"))?;
    Ok(actions.as_array().map(Vec::len).unwrap_or(0))
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
    let config_exists = root.join(".minimap/config.json").exists();
    let graph_dirs_exist = [
        ".minimap/graph/screens",
        ".minimap/graph/edges",
        ".minimap/routes",
        ".minimap/checks",
        ".minimap/proposals",
        ".minimap/runs",
        ".minimap/state",
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
