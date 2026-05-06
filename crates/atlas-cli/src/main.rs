use anyhow::Result;
use atlas_graph::{exit_code_for_status, resolve_route};
use atlas_repo::{accept_proposal, load_context, load_graph, run_init};
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
    Route {
        target: String,
        #[arg(long)]
        current_screen: Option<String>,
    },
    Accept {
        proposal_id: String,
    },
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
