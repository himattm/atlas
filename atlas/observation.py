"""Observation run recorder for raw Atlas runtime artifacts."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Mapping, Optional, Union


ATLAS_GITIGNORE_PATTERNS = (".atlas/runs/", ".atlas/state/")


def required_gitignore_patterns() -> tuple[str, ...]:
    """Patterns init should place in the repo gitignore."""

    return ATLAS_GITIGNORE_PATTERNS


def _utc_now() -> datetime:
    return datetime.now(timezone.utc)


def _timestamp(value: datetime) -> str:
    return value.isoformat(timespec="milliseconds").replace("+00:00", "Z")


def _slug(value: str) -> str:
    normalized = re.sub(r"[^A-Za-z0-9_.-]+", "-", value.strip()).strip("-")
    return normalized[:80] or "run"


@dataclass(frozen=True)
class RunMetadata:
    """Metadata for an observation run stored under ``.atlas/runs``."""

    run_id: str
    name: str
    path: Path
    status: str
    started_at: str
    stopped_at: Optional[str] = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": "atlas.observation_run.v1",
            "run_id": self.run_id,
            "name": self.name,
            "path": str(self.path),
            "status": self.status,
            "started_at": self.started_at,
            "stopped_at": self.stopped_at,
        }


class ObservationRun:
    """Recorder for one active run directory."""

    def __init__(
        self,
        path: Path,
        *,
        clock: Callable[[], datetime] = _utc_now,
    ) -> None:
        self.path = path
        self.clock = clock

    @property
    def metadata_path(self) -> Path:
        return self.path / "metadata.json"

    @property
    def actions_path(self) -> Path:
        return self.path / "actions.json"

    @property
    def observations_path(self) -> Path:
        return self.path / "observations.json"

    def metadata(self) -> dict[str, Any]:
        return _read_json_object(self.metadata_path)

    def record_action(
        self,
        *,
        kind: str,
        payload: Mapping[str, Any],
        reason: Optional[str] = None,
    ) -> dict[str, Any]:
        event = self._event(kind=kind, payload=payload)
        if reason is not None:
            event["reason"] = reason
        _append_json_array(self.actions_path, event)
        return event

    def record_observation(
        self,
        *,
        kind: str,
        payload: Mapping[str, Any],
    ) -> dict[str, Any]:
        event = self._event(kind=kind, payload=payload)
        _append_json_array(self.observations_path, event)
        return event

    def _event(self, *, kind: str, payload: Mapping[str, Any]) -> dict[str, Any]:
        return {
            "schema_version": "atlas.observation_event.v1",
            "timestamp": _timestamp(self.clock()),
            "kind": kind,
            "payload": dict(payload),
        }


class ObservationRecorder:
    """Manage current observation runs under a repository root."""

    def __init__(
        self,
        repo_root: Union[str, Path] = ".",
        *,
        clock: Callable[[], datetime] = _utc_now,
    ) -> None:
        self.repo_root = Path(repo_root)
        self.clock = clock

    @property
    def runs_dir(self) -> Path:
        return self.repo_root / ".atlas" / "runs"

    @property
    def current_path(self) -> Path:
        return self.runs_dir / "current.json"

    def start(
        self,
        name: str = "run",
        *,
        metadata: Optional[Mapping[str, Any]] = None,
    ) -> ObservationRun:
        self.runs_dir.mkdir(parents=True, exist_ok=True)
        now = self.clock()
        started_at = _timestamp(now)
        run_id = f"{started_at.replace(':', '').replace('.', '-')}-{_slug(name)}"
        run_path = self.runs_dir / run_id
        run_path.mkdir(parents=True, exist_ok=False)
        (run_path / "raw-layouts").mkdir()
        (run_path / "layout-deltas").mkdir()
        (run_path / "screenshots").mkdir()

        run_metadata = {
            "schema_version": "atlas.observation_run.v1",
            "run_id": run_id,
            "name": name,
            "path": str(run_path),
            "status": "running",
            "started_at": started_at,
            "stopped_at": None,
            "metadata": dict(metadata or {}),
        }
        _write_json(run_path / "metadata.json", run_metadata)
        _write_json(run_path / "actions.json", [])
        _write_json(run_path / "observations.json", [])
        _write_json(
            self.current_path,
            {"schema_version": "atlas.current_run.v1", "run_id": run_id},
        )
        return ObservationRun(run_path, clock=self.clock)

    def current(self) -> Optional[ObservationRun]:
        if not self.current_path.exists():
            return None
        pointer = _read_json_object(self.current_path)
        run_id = pointer.get("run_id")
        if not isinstance(run_id, str):
            return None
        run = ObservationRun(self.runs_dir / run_id, clock=self.clock)
        if not run.metadata_path.exists():
            return None
        metadata = run.metadata()
        if metadata.get("status") != "running":
            return None
        return run

    def stop(self) -> dict[str, Any]:
        run = self.current()
        if run is None:
            raise RuntimeError("No current observation run")
        metadata = run.metadata()
        metadata["status"] = "stopped"
        metadata["stopped_at"] = _timestamp(self.clock())
        _write_json(run.metadata_path, metadata)
        self.current_path.unlink(missing_ok=True)
        return metadata

    def record_action(
        self,
        *,
        kind: str,
        payload: Mapping[str, Any],
        reason: Optional[str] = None,
    ) -> dict[str, Any]:
        run = self._require_current()
        return run.record_action(kind=kind, payload=payload, reason=reason)

    def record_observation(
        self,
        *,
        kind: str,
        payload: Mapping[str, Any],
    ) -> dict[str, Any]:
        run = self._require_current()
        return run.record_observation(kind=kind, payload=payload)

    def _require_current(self) -> ObservationRun:
        run = self.current()
        if run is None:
            raise RuntimeError("No current observation run")
        return run


def _read_json_object(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict):
        raise ValueError(f"Expected JSON object at {path}")
    return value


def _append_json_array(path: Path, event: Mapping[str, Any]) -> None:
    if path.exists():
        with path.open("r", encoding="utf-8") as handle:
            values = json.load(handle)
    else:
        values = []
    if not isinstance(values, list):
        raise ValueError(f"Expected JSON array at {path}")
    values.append(dict(event))
    _write_json(path, values)


def _write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(value, handle, indent=2, sort_keys=True)
        handle.write("\n")
