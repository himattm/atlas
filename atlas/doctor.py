from __future__ import annotations

import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .config import DEFAULT_SKILL_NAME, load_config
from .init_cmd import AGENT_SKILL_ROOTS, ATLAS_DIRS, GITIGNORE_ENTRIES, missing_gitignore_entries


@dataclass(frozen=True)
class Check:
    name: str
    status: str
    detail: str = ""
    data: dict[str, Any] | None = None

    def to_json(self) -> dict[str, Any]:
        result: dict[str, Any] = {"name": self.name, "status": self.status}
        if self.detail:
            result["detail"] = self.detail
        if self.data:
            result["data"] = self.data
        return result


def run_doctor(root: Path) -> dict[str, Any]:
    root = root.resolve()
    checks = [
        check_config(root),
        check_graph_dirs(root),
        check_gitignore(root),
        check_skills(root),
        check_android_cli(),
        check_adb(),
    ]

    summary = {
        "pass": sum(1 for check in checks if check.status == "pass"),
        "warn": sum(1 for check in checks if check.status == "warn"),
        "fail": sum(1 for check in checks if check.status == "fail"),
    }
    return {
        "ok": summary["fail"] == 0,
        "root": str(root),
        "summary": summary,
        "checks": [check.to_json() for check in checks],
    }


def check_config(root: Path) -> Check:
    result = load_config(root)
    if not result.exists:
        return Check("config", "fail", ".atlas/config.json is missing")
    if result.errors:
        return Check("config", "fail", "; ".join(result.errors))

    android = result.config.get("android", {}) if result.config else {}
    if not android.get("app_package"):
        return Check("config", "warn", "android.app_package is not configured", {"path": str(result.path)})
    return Check("config", "pass", data={"path": str(result.path)})


def check_graph_dirs(root: Path) -> Check:
    required = [
        ".atlas/graph/screens",
        ".atlas/graph/edges",
        ".atlas/routes",
        ".atlas/checks",
        ".atlas/proposals",
        ".atlas/runs",
        ".atlas/state",
    ]
    missing = [path for path in required if not (root / path).is_dir()]
    if missing:
        return Check("graph_dirs", "fail", "missing required directories", {"missing": missing})
    return Check("graph_dirs", "pass", data={"checked": required})


def check_gitignore(root: Path) -> Check:
    missing = missing_gitignore_entries(root / ".gitignore")
    if missing:
        return Check("gitignore", "fail", "Atlas runtime artifacts are not fully gitignored", {"missing": missing})
    return Check("gitignore", "pass", data={"entries": GITIGNORE_ENTRIES})


def check_skills(root: Path) -> Check:
    cfg = load_config(root)
    skill_name = DEFAULT_SKILL_NAME
    if cfg.config and isinstance(cfg.config.get("skills"), dict):
        skill_name = cfg.config["skills"].get("skill_name") or skill_name

    coverage: dict[str, list[str]] = {}
    installed: list[str] = []
    for agent, roots in AGENT_SKILL_ROOTS.items():
        coverage[agent] = []
        for skill_root in roots:
            rel_path = f"{skill_root}/{skill_name}/SKILL.md"
            if (root / rel_path).is_file():
                coverage[agent].append(rel_path)
                installed.append(rel_path)

    if not installed:
        return Check("skills", "fail", "no Atlas skill files found", {"coverage": coverage})

    missing_agents = [agent for agent, paths in coverage.items() if not paths]
    status = "warn" if missing_agents else "pass"
    detail = "some supported agents are not covered" if missing_agents else ""
    return Check("skills", status, detail, {"coverage": coverage, "missing_agents": missing_agents})


def check_android_cli() -> Check:
    path = shutil.which("android")
    if not path:
        return Check("android_cli", "warn", "android CLI is not on PATH")
    return Check("android_cli", "pass", data={"path": path})


def check_adb() -> Check:
    path = shutil.which("adb")
    if not path:
        return Check("adb", "warn", "adb is not on PATH")
    return Check("adb", "pass", data={"path": path})
