from __future__ import annotations

import difflib
from dataclasses import dataclass
from pathlib import Path

from .config import DEFAULT_SKILL_NAME, config_path, default_config, write_json


ATLAS_DIRS = [
    ".atlas",
    ".atlas/graph",
    ".atlas/graph/screens",
    ".atlas/graph/edges",
    ".atlas/routes",
    ".atlas/checks",
    ".atlas/proposals",
    ".atlas/runs",
    ".atlas/state",
]

GITIGNORE_ENTRIES = [".atlas/runs/", ".atlas/state/"]

AGENT_SKILL_ROOTS: dict[str, list[str]] = {
    "codex": [".agents/skills", ".codex/skills"],
    "claude": [".claude/skills"],
    "android-studio": [".skills", ".agent/skills"],
    "gemini": [".gemini/skills"],
}

ALL_AGENTS = tuple(AGENT_SKILL_ROOTS)


SKILL_BODY = """---
name: atlas-app-navigation
description: Use when working in an Android codebase and needing to navigate the launched app, inspect Android layout JSON, use android layout, use android layout --diff, tap UI elements, validate screens, learn routes, reuse known navigation, or update the repo's Atlas graph. Before calling android layout or raw adb tap commands directly, check Atlas first.
metadata:
  author: atlas
  version: "1.0"
---

# Atlas App Navigation Skill

Atlas is this repo's shared navigation memory and soft validation layer for AI agents working in this Android codebase.

Use Atlas whenever you need to navigate, inspect, tap, check, validate, or learn the launched Android app UI.

## Core rule

Before using raw Android layout or adb tap commands:

1. Ask Atlas whether the repo already knows the route.
2. If Atlas knows the route, use Atlas to navigate.
3. If Atlas does not know the route, explore through Atlas wrappers so the run can be learned and shared.
4. Stage learned graph updates, but do not accept or commit them without explicit user approval.

## Navigate to a known screen

```bash
atlas route <target> --json
```

If a route exists:

```bash
atlas go <target> --mode verified --json
atlas check <target> --json
```

## Explore an unknown screen

```bash
atlas observe start <task-name> --json
atlas layout --json
```

Read the compact layout summary, choose a selector, and call:

```bash
atlas tap --selector "<best selector>" --reason "<why this tap is needed>" --json
atlas layout --diff --json
atlas check --current --json
```

When the target is reached:

```bash
atlas observe stop --json
atlas learn --from-current-run --stage --json
```

Summarize proposed graph updates. Do not accept or commit them unless the user explicitly approves.

## Validate after code changes

After Android UI changes and after the app is built and launched:

```bash
atlas validate --json
```

Report context mismatch or system overlay blockers first, then broken known routes, failed checks, new or changed screens, selector drift, and proposed graph updates.

Do not run `atlas accept`, `atlas update-baseline`, or commit `.atlas/` changes without explicit user approval.

## Fallback

Only use raw Android CLI layout or adb tap commands if Atlas cannot perform the needed action. If you bypass Atlas, explain why and prefer adding the observed navigation back into Atlas afterward.
"""


@dataclass(frozen=True)
class Change:
    kind: str
    path: str
    status: str
    detail: str = ""
    diff: str = ""


@dataclass(frozen=True)
class InitResult:
    ok: bool
    dry_run: bool
    root: str
    agents: list[str]
    skill_paths: list[str]
    changes: list[Change]

    def to_json(self) -> dict[str, object]:
        return {
            "ok": self.ok,
            "dry_run": self.dry_run,
            "root": self.root,
            "agents": self.agents,
            "skill_paths": self.skill_paths,
            "changes": [change.__dict__ for change in self.changes],
        }


def parse_agents(value: str) -> list[str]:
    normalized = [part.strip() for part in value.split(",") if part.strip()]
    if not normalized:
        raise ValueError("--agents must not be empty")
    if len(normalized) > 1 and ("auto" in normalized or "all" in normalized):
        raise ValueError("--agents auto/all cannot be combined with named agents")
    unknown = [agent for agent in normalized if agent not in {"auto", "all", *ALL_AGENTS}]
    if unknown:
        raise ValueError(f"unknown agent(s): {', '.join(unknown)}")
    return normalized


def selected_agents(root: Path, agents_value: str) -> tuple[list[str], bool]:
    requested = parse_agents(agents_value)
    if requested == ["all"]:
        return list(ALL_AGENTS), False
    if requested != ["auto"]:
        return requested, False

    detected: list[str] = []
    for agent, roots in AGENT_SKILL_ROOTS.items():
        if any((root / skill_root).parent.exists() or (root / skill_root).exists() for skill_root in roots):
            detected.append(agent)

    if detected:
        return detected, False

    return ["codex"], True


def skill_paths_for_agents(agents: list[str], skill_name: str = DEFAULT_SKILL_NAME) -> list[str]:
    roots: list[str] = []
    for agent in agents:
        roots.extend(AGENT_SKILL_ROOTS[agent])

    seen: set[str] = set()
    paths: list[str] = []
    for root in roots:
        path = f"{root}/{skill_name}/SKILL.md"
        if path not in seen:
            seen.add(path)
            paths.append(path)
    return paths


def run_init(root: Path, *, dry_run: bool, yes: bool, agents_value: str) -> InitResult:
    root = root.resolve()
    agents, used_auto_fallback = selected_agents(root, agents_value)
    skill_paths = skill_paths_for_agents(agents)
    changes = plan_changes(root, skill_paths, yes=yes, used_auto_fallback=used_auto_fallback)
    has_conflict = any(change.status == "conflict" for change in changes)

    if not dry_run and not has_conflict:
        apply_changes(root, changes)

    if dry_run:
        changes = [
            Change(change.kind, change.path, "planned" if change.status in {"create", "append", "overwrite"} else change.status, change.detail, change.diff)
            for change in changes
        ]

    return InitResult(
        ok=not has_conflict,
        dry_run=dry_run,
        root=str(root),
        agents=agents,
        skill_paths=skill_paths,
        changes=changes,
    )


def plan_changes(root: Path, skill_paths: list[str], *, yes: bool, used_auto_fallback: bool) -> list[Change]:
    changes: list[Change] = []

    for directory in ATLAS_DIRS:
        path = root / directory
        if path.exists():
            changes.append(Change("directory", directory, "exists"))
        else:
            changes.append(Change("directory", directory, "create"))

    cfg_path = config_path(root)
    if cfg_path.exists():
        changes.append(Change("config", ".atlas/config.json", "exists"))
    else:
        changes.append(Change("config", ".atlas/config.json", "create"))

    gitignore_path = root / ".gitignore"
    missing_entries = missing_gitignore_entries(gitignore_path)
    if missing_entries:
        detail = "add " + ", ".join(missing_entries)
        changes.append(Change("gitignore", ".gitignore", "append", detail))
    else:
        changes.append(Change("gitignore", ".gitignore", "exists"))

    if used_auto_fallback:
        changes.append(Change("agent-selection", "codex", "selected", "auto detected no agent directories; selected codex repo-local skill paths"))

    for rel_path in skill_paths:
        path = root / rel_path
        if not path.exists():
            changes.append(Change("skill", rel_path, "create"))
            continue

        existing = path.read_text(encoding="utf-8")
        if existing == SKILL_BODY:
            changes.append(Change("skill", rel_path, "exists"))
        elif yes:
            changes.append(Change("skill", rel_path, "overwrite", diff=unified_diff(existing, SKILL_BODY, rel_path)))
        else:
            changes.append(Change("skill", rel_path, "conflict", "existing skill differs; rerun with --yes to overwrite", unified_diff(existing, SKILL_BODY, rel_path)))

    return changes


def apply_changes(root: Path, changes: list[Change]) -> None:
    for change in changes:
        path = root / change.path
        if change.kind == "directory" and change.status == "create":
            path.mkdir(parents=True, exist_ok=True)
        elif change.kind == "config" and change.status == "create":
            path.parent.mkdir(parents=True, exist_ok=True)
            write_json(path, default_config())
        elif change.kind == "gitignore" and change.status == "append":
            append_gitignore_entries(path)
        elif change.kind == "skill" and change.status in {"create", "overwrite"}:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(SKILL_BODY, encoding="utf-8")


def missing_gitignore_entries(path: Path) -> list[str]:
    if not path.exists():
        return list(GITIGNORE_ENTRIES)
    lines = {line.strip() for line in path.read_text(encoding="utf-8").splitlines()}
    return [entry for entry in GITIGNORE_ENTRIES if entry not in lines]


def append_gitignore_entries(path: Path) -> None:
    existing = path.read_text(encoding="utf-8") if path.exists() else ""
    missing = missing_gitignore_entries(path)
    if not missing:
        return

    pieces = [existing]
    if existing and not existing.endswith("\n"):
        pieces.append("\n")
    if existing and existing.strip():
        pieces.append("\n")
    pieces.append("# Atlas runtime artifacts\n")
    pieces.extend(f"{entry}\n" for entry in missing)
    path.write_text("".join(pieces), encoding="utf-8")


def unified_diff(before: str, after: str, rel_path: str) -> str:
    diff = difflib.unified_diff(
        before.splitlines(),
        after.splitlines(),
        fromfile=f"{rel_path} (existing)",
        tofile=f"{rel_path} (atlas)",
        lineterm="",
    )
    return "\n".join(diff)
