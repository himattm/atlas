from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Mapping
from typing import TextIO

from . import __version__
from .android import AdbExecutor, AndroidCliAdapter, AndroidError
from .config import load_config
from .doctor import run_doctor
from .init_cmd import run_init
from .matching import match_screen
from .models import GraphContext, Proposal
from .normalization import normalized_identity
from .observation import ObservationRecorder
from .operations import (
    CoordinateGuardError,
    SelectorNotFoundError,
    check_current_screen,
    execute_route,
    layout as runtime_layout,
    tap_label,
    tap_point,
    tap_selector,
)
from .routing import RoutePlan, resolve_route
from .storage import accept_proposal, load_graph, stage_proposal


def main(
    argv: list[str] | None = None,
    *,
    cwd: str | Path | None = None,
    stdout: TextIO | None = None,
    stderr: TextIO | None = None,
) -> int:
    stdout = stdout or sys.stdout
    stderr = stderr or sys.stderr
    root = Path(cwd) if cwd is not None else Path.cwd()

    parser = build_parser()
    try:
        args = parser.parse_args(argv)
        ensure_global_flags(args)
        if args.command == "init":
            result = run_init(root, dry_run=args.dry_run, yes=args.yes, agents_value=args.agents)
            emit(args, result.to_json(), human_init(result), stdout)
            return 0 if result.ok else 2
        if args.command == "doctor":
            result = run_doctor(root)
            emit(args, result, human_doctor(result), stdout)
            return 0 if result["ok"] else 1
        if args.command == "layout":
            result = runtime_layout(AndroidCliAdapter(), diff=args.diff, observation=current_observation(root))
            emit(args, result, human_status(result), stdout)
            return 0
        if args.command == "tap":
            result = run_tap(args, root)
            emit(args, result, human_status(result), stdout)
            return 0
        if args.command == "observe":
            result = run_observe(args, root)
            emit(args, result, human_status(result), stdout)
            return 0
        if args.command == "route":
            result = route_result(args.target, root=root, current_screen=args.current_screen).to_result().to_dict()
            emit(args, result, human_status(result), stdout)
            return exit_code_for_status(str(result["status"]))
        if args.command == "go":
            result = run_go(args, root)
            emit(args, result, human_status(result), stdout)
            return exit_code_for_status(str(result["status"]))
        if args.command == "check":
            result = run_check(args, root)
            emit(args, result, human_status(result), stdout)
            return exit_code_for_status(str(result["status"]))
        if args.command == "learn":
            result = run_learn(args, root)
            emit(args, result, human_status(result), stdout)
            return 1 if result.get("human_approval_required") else 0
        if args.command == "accept":
            result = run_accept(args, root)
            emit(args, result, human_status(result), stdout)
            return 0
        parser.print_help(stdout)
        return 0
    except (AndroidError, SelectorNotFoundError, CoordinateGuardError) as exc:
        ensure_global_flags(locals().get("args", argparse.Namespace()))
        result = exception_payload(exc)
        if getattr(locals().get("args", argparse.Namespace()), "json", False):
            print(json.dumps(result, sort_keys=True, separators=(",", ":")), file=stdout)
        else:
            print(f"atlas: {result['summary']}", file=stderr)
        return exit_code_for_status(str(result["status"]))
    except (ValueError, RuntimeError) as exc:
        ensure_global_flags(locals().get("args", argparse.Namespace()))
        result = error_payload("atlas_error", str(exc))
        if getattr(locals().get("args", argparse.Namespace()), "json", False):
            print(json.dumps(result, sort_keys=True, separators=(",", ":")), file=stdout)
        else:
            print(f"atlas: {exc}", file=stderr)
        return exit_code_for_status(str(result["status"]))


def build_parser() -> argparse.ArgumentParser:
    global_flags = argparse.ArgumentParser(add_help=False)
    global_flags.add_argument("--json", action="store_true", default=argparse.SUPPRESS, help="emit JSON")
    global_flags.add_argument("--quiet", action="store_true", default=argparse.SUPPRESS, help="suppress human output")
    global_flags.add_argument("--no-color", action="store_true", default=argparse.SUPPRESS, help="disable color")

    parser = argparse.ArgumentParser(
        prog="atlas",
        description="Shared navigation memory and soft validation for AI agents working in Android codebases.",
        parents=[global_flags],
    )
    parser.add_argument("--version", action="version", version=f"atlas {__version__}")
    subparsers = parser.add_subparsers(dest="command")

    init_parser = subparsers.add_parser("init", parents=[global_flags], help="initialize Atlas repo storage and agent skills")
    init_parser.add_argument("--dry-run", action="store_true", help="report planned changes without writing")
    init_parser.add_argument("--yes", action="store_true", help="overwrite existing Atlas skill files when they differ")
    init_parser.add_argument(
        "--agents",
        default="auto",
        help="agent targets: auto, all, or comma-separated codex,claude,android-studio,gemini",
    )

    subparsers.add_parser("doctor", parents=[global_flags], help="check Atlas repository setup and Android tool availability")

    layout_parser = subparsers.add_parser("layout", parents=[global_flags], help="wrap android layout with Atlas observation")
    layout_parser.add_argument("--diff", action="store_true", help="use Android in-session layout diff, not graph drift")

    tap_parser = subparsers.add_parser("tap", parents=[global_flags], help="resolve and execute a tap through adb")
    tap_target = tap_parser.add_mutually_exclusive_group(required=True)
    tap_target.add_argument("--selector", help="layout selector such as text=Settings or resource_id=settings")
    tap_target.add_argument("--point", help="absolute tap point as X,Y")
    tap_target.add_argument("--label", type=int, help="annotated screenshot label number")
    tap_parser.add_argument("--screenshot", help="screenshot path for --label")
    tap_parser.add_argument("--reason", default="", help="why this tap is needed")
    tap_parser.add_argument("--mode", choices=["safe", "verified", "fast"], default="verified")

    observe_parser = subparsers.add_parser("observe", parents=[global_flags], help="record an observation run")
    observe_subparsers = observe_parser.add_subparsers(dest="observe_command", required=True)
    observe_start = observe_subparsers.add_parser("start", parents=[global_flags], help="start recording")
    observe_start.add_argument("name")
    observe_subparsers.add_parser("stop", parents=[global_flags], help="stop current recording")

    route_parser = subparsers.add_parser("route", parents=[global_flags], help="plan a route through committed graph memory")
    route_parser.add_argument("target")
    route_parser.add_argument("--current-screen", help="current screen name when already known")

    go_parser = subparsers.add_parser("go", parents=[global_flags], help="execute a committed route")
    go_parser.add_argument("target")
    go_parser.add_argument("--current-screen", help="current screen name when already known")
    go_parser.add_argument("--mode", choices=["safe", "verified", "fast"], default="verified")

    check_parser = subparsers.add_parser("check", parents=[global_flags], help="check current or named screen")
    check_target = check_parser.add_mutually_exclusive_group()
    check_target.add_argument("--current", action="store_true")
    check_target.add_argument("screen", nargs="?")

    learn_parser = subparsers.add_parser("learn", parents=[global_flags], help="stage graph updates from a recorded run")
    learn_parser.add_argument("--from-current-run", action="store_true", required=True)
    learn_parser.add_argument("--stage", action="store_true", required=True)

    accept_parser = subparsers.add_parser("accept", parents=[global_flags], help="explicitly accept a staged Atlas proposal")
    accept_parser.add_argument("proposal_id")

    return parser


def ensure_global_flags(args: argparse.Namespace) -> None:
    for name in ("json", "quiet", "no_color"):
        if not hasattr(args, name):
            setattr(args, name, False)


def emit(args: argparse.Namespace, payload: dict[str, object], human: str, stdout: TextIO) -> None:
    if args.json:
        print(json.dumps(payload, sort_keys=True, separators=(",", ":")), file=stdout)
    elif not args.quiet:
        print(human, file=stdout)


def human_init(result: object) -> str:
    data = result.to_json()
    created = sum(1 for change in data["changes"] if change["status"] in {"create", "append", "overwrite", "planned"})
    conflicts = sum(1 for change in data["changes"] if change["status"] == "conflict")
    mode = "planned" if data["dry_run"] else "applied"
    lines = [f"Atlas init {mode}: {created} change(s), {conflicts} conflict(s)."]
    for path in data["skill_paths"]:
        lines.append(f"skill: {path}")
    return "\n".join(lines)


def human_doctor(result: dict[str, object]) -> str:
    summary = result["summary"]
    lines = [f"Atlas doctor: pass={summary['pass']} warn={summary['warn']} fail={summary['fail']}"]
    for check in result["checks"]:
        detail = f" - {check['detail']}" if "detail" in check else ""
        lines.append(f"{check['status']}: {check['name']}{detail}")
    return "\n".join(lines)


def human_status(result: Mapping[str, Any]) -> str:
    status = result.get("status", "ok")
    summary = result.get("summary") or result.get("message") or status
    return f"Atlas {status}: {summary}"


def current_observation(root: Path) -> ObservationRecorder | None:
    recorder = ObservationRecorder(root)
    return recorder if recorder.current() is not None else None


def run_tap(args: argparse.Namespace, root: Path) -> dict[str, Any]:
    android = AndroidCliAdapter()
    adb = AdbExecutor()
    observation = current_observation(root)
    if args.selector:
        return tap_selector(android, adb, selector=args.selector, reason=args.reason or None, observation=observation)
    if args.point:
        x, y = parse_point(args.point)
        return tap_point(adb, x=x, y=y, mode=args.mode, observation=observation)
    if args.label is not None:
        if not args.screenshot:
            raise ValueError("--label requires --screenshot")
        return tap_label(android, adb, label=args.label, screenshot=args.screenshot, observation=observation)
    raise ValueError("tap requires selector, point, or label")


def parse_point(value: str) -> tuple[int, int]:
    parts = [part.strip() for part in value.split(",", 1)]
    if len(parts) != 2:
        raise ValueError("--point must be X,Y")
    return int(parts[0]), int(parts[1])


def run_observe(args: argparse.Namespace, root: Path) -> dict[str, Any]:
    recorder = ObservationRecorder(root)
    if args.observe_command == "start":
        run = recorder.start(args.name)
        return {"schema_version": "atlas.result.v1", "status": "ok", "summary": "observation started", "run": run.metadata()}
    if args.observe_command == "stop":
        return {"schema_version": "atlas.result.v1", "status": "ok", "summary": "observation stopped", "run": recorder.stop()}
    raise ValueError(f"unknown observe command: {args.observe_command}")


def graph_context(root: Path) -> GraphContext:
    loaded = load_config(root)
    if loaded.config and isinstance(loaded.config.get("context"), dict):
        return GraphContext(loaded.config["context"])
    return GraphContext({})


def route_result(target: str, *, root: Path, current_screen: str | None) -> RoutePlan:
    graph = load_graph(root)
    return resolve_route(
        target,
        current_screen=current_screen,
        screens=graph["screens"],
        edges=graph["edges"],
        routes=graph["routes"],
        context=graph_context(root),
    )


def run_go(args: argparse.Namespace, root: Path) -> dict[str, Any]:
    plan = route_result(args.target, root=root, current_screen=args.current_screen)
    if plan.status != "ok":
        return plan.to_result().to_dict()

    route_payload = route_execution_payload(plan, args.mode)
    result = execute_route(AndroidCliAdapter(), AdbExecutor(), route=route_payload, observation=current_observation(root))
    result.update(
        {
            "schema_version": "atlas.result.v1",
            "target": plan.target,
            "preferred_path_used": plan.preferred_path_used,
            "graph_fallback_used": plan.graph_fallback_used,
            "route_confidence": plan.route_confidence,
            "estimated_layout_calls_saved": max(len(plan.edges), 1),
        }
    )
    return result


def route_execution_payload(plan: RoutePlan, mode: str) -> dict[str, Any]:
    steps: list[dict[str, Any]] = []
    for edge in plan.edges:
        action = edge.action.to_dict() if hasattr(edge.action, "to_dict") else dict(edge.action)
        selector = selector_from_action(action)
        if selector is None:
            raise ValueError(f"edge {edge.id} has no executable selector candidate")
        steps.append(
            {
                "edge": edge.id,
                "reason": edge.intent or action.get("description") or edge.id,
                "action": {"kind": "tap", "selector": selector},
            }
        )
    return {"name": plan.route.name if plan.route else plan.target, "mode": mode, "steps": steps}


def selector_from_action(action: Mapping[str, Any]) -> str | None:
    candidates = action.get("selector_candidates", [])
    if not isinstance(candidates, list):
        return None
    executable = [candidate for candidate in candidates if isinstance(candidate, Mapping) and "value" in candidate]
    if not executable:
        return None
    best = max(executable, key=lambda candidate: float(candidate.get("score", 0.0)))
    kind = str(best.get("kind", "")).strip()
    value = str(best.get("value", "")).strip()
    key_map = {
        "visible_text": "text",
        "visible_text_fuzzy": "text",
        "accessibility": "content_description",
        "accessibility_or_semantic": "content_description",
        "test_tag": "test_tag",
        "resource_id": "resource_id",
    }
    return f"{key_map.get(kind, kind)}={value}" if kind and value else None


class AtlasMatcher:
    def __init__(self, root: Path) -> None:
        self.root = root

    def match(self, layout_json: Mapping[str, Any]) -> Mapping[str, Any]:
        normalized, identity = normalized_identity(layout_json)
        screens = load_graph(self.root)["screens"]
        result = match_screen(normalized, screens)
        return {
            "status": result.status,
            "matched_screen": result.screen.name if result.screen else None,
            "match_confidence": result.match_confidence,
            "hash_matched": result.hash_matched,
            "identity_hash": identity,
        }


def run_check(args: argparse.Namespace, root: Path) -> dict[str, Any]:
    if args.current or not args.screen:
        result = check_current_screen(AndroidCliAdapter(), matcher=AtlasMatcher(root))
        result["schema_version"] = "atlas.result.v1"
        return result

    graph = load_graph(root)
    screen = next(
        (
            candidate
            for candidate in graph["screens"].values()
            if args.screen in {candidate.id, candidate.name, *candidate.aliases}
        ),
        None,
    )
    if screen is None:
        return {"schema_version": "atlas.result.v1", "status": "screen_unknown", "summary": f"screen not found: {args.screen}"}
    context_result = screen.context_result(graph_context(root)).to_dict()
    context_result["summary"] = f"screen guard check for {screen.name}"
    return context_result


def run_learn(args: argparse.Namespace, root: Path) -> dict[str, Any]:
    recorder = ObservationRecorder(root)
    run = recorder.current()
    if run is None:
        raise RuntimeError("No current observation run")
    metadata = run.metadata()
    proposal = Proposal(
        id=f"proposal-{metadata['run_id']}",
        kind="observation_run_review",
        reason="Review this observation run and convert stable navigation facts into graph objects.",
        changes=[],
    )
    path = stage_proposal(root, proposal)
    return {
        "schema_version": "atlas.result.v1",
        "status": "changed_requires_review",
        "summary": "staged observation run review proposal",
        "proposal_id": proposal.id,
        "proposal_path": str(path),
        "raw_artifact_paths": [str(run.path)],
        "human_approval_required": True,
        "recommended_next_command": f"atlas accept {proposal.id} --json",
    }


def run_accept(args: argparse.Namespace, root: Path) -> dict[str, Any]:
    written = accept_proposal(root, args.proposal_id)
    return {
        "schema_version": "atlas.result.v1",
        "status": "ok",
        "summary": "proposal accepted",
        "graph_objects_touched": [str(path) for path in written],
    }


def error_payload(code: str, message: str) -> dict[str, Any]:
    return {
        "schema_version": "atlas.result.v1",
        "status": "config_error" if code == "atlas_error" else "environment_error",
        "summary": message,
        "error": {"code": code, "message": message},
    }


def exception_payload(exc: Exception) -> dict[str, Any]:
    if hasattr(exc, "to_dict"):
        payload = exc.to_dict()  # type: ignore[no-untyped-call]
        message = payload.get("error", {}).get("message", str(exc))
        code = payload.get("error", {}).get("code", "atlas_error")
    else:
        message = str(exc)
        code = getattr(exc, "code", "atlas_error")
        payload = {"error": {"code": code, "message": message}}
    if code == "selector_not_found":
        status = "selector_drift"
    elif code == "coordinate_guard_required":
        status = "config_error"
    else:
        status = "environment_error"
    payload.update({"schema_version": "atlas.result.v1", "status": status, "summary": message})
    return payload


def exit_code_for_status(status: str) -> int:
    return {
        "ok": 0,
        "passed": 0,
        "changed_requires_review": 1,
        "route_broken": 3,
        "selector_drift": 4,
        "screen_unknown": 5,
        "not_found": 5,
        "environment_error": 6,
        "config_error": 7,
        "context_mismatch": 8,
    }.get(status, 2)
