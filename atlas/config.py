from __future__ import annotations

import copy
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any


CONFIG_SCHEMA_VERSION = "atlas.config.v1"
DEFAULT_SKILL_NAME = "atlas-app-navigation"
ATLAS_DIR = ".atlas"


DEFAULT_CONFIG: dict[str, Any] = {
    "schema_version": CONFIG_SCHEMA_VERSION,
    "android": {
        "app_package": "",
        "assume_app_launched": True,
        "permissions": [],
    },
    "context": {
        "auth_state": "unknown",
        "onboarding_state": "unknown",
        "locale": "unknown",
        "orientation": "unknown",
        "feature_flags": {},
    },
    "storage": {
        "commit_raw_layouts": False,
        "runs_dir": ".atlas/runs",
        "state_dir": ".atlas/state",
        "commit_runtime_telemetry": False,
        "generate_index_cache": True,
        "commit_index_cache": False,
    },
    "navigation": {
        "default_mode": "verified",
        "safe_mode_fallback": True,
        "screen_match_confidence_min": 0.78,
        "repair_candidate_confidence_min": 0.65,
        "transition_timeout_ms": 3000,
    },
    "normalization": {
        "store_normalized_bounds": True,
        "collapse_repeating_lists": True,
        "strip_dynamic_text_inputs": True,
    },
    "redaction": {
        "run_before_hashing": True,
        "default_text_action": "exclude",
        "commit_verbatim_text": False,
        "allowlist_static_text": ["Home", "Settings", "Bookmarks"],
    },
    "skills": {
        "skill_name": DEFAULT_SKILL_NAME,
        "install_strategy": "multi-write-detected",
        "install_paths": [
            ".agents/skills",
            ".codex/skills",
            ".skills",
            ".agent/skills",
            ".claude/skills",
            ".gemini/skills",
        ],
    },
}


@dataclass(frozen=True)
class ConfigLoadResult:
    path: Path
    exists: bool
    config: dict[str, Any] | None
    errors: list[str]

    @property
    def ok(self) -> bool:
        return self.exists and self.config is not None and not self.errors


def default_config() -> dict[str, Any]:
    return copy.deepcopy(DEFAULT_CONFIG)


def config_path(root: Path) -> Path:
    return root / ATLAS_DIR / "config.json"


def load_config(root: Path) -> ConfigLoadResult:
    path = config_path(root)
    if not path.exists():
        return ConfigLoadResult(path=path, exists=False, config=None, errors=["config file is missing"])

    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        return ConfigLoadResult(path=path, exists=True, config=None, errors=[f"invalid JSON: {exc.msg}"])

    errors = validate_config(raw)
    return ConfigLoadResult(path=path, exists=True, config=raw if isinstance(raw, dict) else None, errors=errors)


def validate_config(value: Any) -> list[str]:
    if not isinstance(value, dict):
        return ["config must be a JSON object"]

    errors: list[str] = []
    if value.get("schema_version") != CONFIG_SCHEMA_VERSION:
        errors.append(f"schema_version must be {CONFIG_SCHEMA_VERSION!r}")

    required_sections = [
        "android",
        "context",
        "storage",
        "navigation",
        "normalization",
        "redaction",
        "skills",
    ]
    for section in required_sections:
        if not isinstance(value.get(section), dict):
            errors.append(f"{section} must be an object")

    android = value.get("android")
    if isinstance(android, dict):
        if not isinstance(android.get("app_package", ""), str):
            errors.append("android.app_package must be a string")
        if not isinstance(android.get("assume_app_launched", True), bool):
            errors.append("android.assume_app_launched must be a boolean")
        if not isinstance(android.get("permissions", []), list):
            errors.append("android.permissions must be an array")

    storage = value.get("storage")
    if isinstance(storage, dict):
        for key in ("runs_dir", "state_dir"):
            if not isinstance(storage.get(key), str) or not storage.get(key):
                errors.append(f"storage.{key} must be a non-empty string")

    navigation = value.get("navigation")
    if isinstance(navigation, dict):
        if navigation.get("default_mode") not in {"safe", "verified", "fast"}:
            errors.append("navigation.default_mode must be safe, verified, or fast")
        for key in ("screen_match_confidence_min", "repair_candidate_confidence_min"):
            number = navigation.get(key)
            if not isinstance(number, (int, float)) or not 0 <= number <= 1:
                errors.append(f"navigation.{key} must be a number between 0 and 1")
        timeout = navigation.get("transition_timeout_ms")
        if not isinstance(timeout, int) or timeout <= 0:
            errors.append("navigation.transition_timeout_ms must be a positive integer")

    redaction = value.get("redaction")
    if isinstance(redaction, dict):
        if redaction.get("run_before_hashing") is not True:
            errors.append("redaction.run_before_hashing must be true")
        if redaction.get("commit_verbatim_text") is not False:
            errors.append("redaction.commit_verbatim_text must default to false")

    skills = value.get("skills")
    if isinstance(skills, dict):
        if not isinstance(skills.get("skill_name"), str) or not skills.get("skill_name"):
            errors.append("skills.skill_name must be a non-empty string")
        install_paths = skills.get("install_paths")
        if not isinstance(install_paths, list) or not all(isinstance(path, str) for path in install_paths):
            errors.append("skills.install_paths must be an array of strings")

    return errors


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
