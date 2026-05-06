"""Deterministic storage for Atlas committed graph files."""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path
from typing import Any, Iterable, Mapping

from .models import (
    EDGE_SCHEMA_VERSION,
    PROPOSAL_SCHEMA_VERSION,
    ROUTE_SCHEMA_VERSION,
    SCREEN_SCHEMA_VERSION,
    NavigationEdge,
    Proposal,
    Route,
    ScreenNode,
    model_from_dict,
)


JsonObject = dict[str, Any]


def canonical_dumps(data: Mapping[str, Any]) -> str:
    return json.dumps(data, ensure_ascii=False, sort_keys=True, indent=2, separators=(",", ": ")) + "\n"


def read_json(path: str | Path) -> JsonObject:
    with Path(path).open("r", encoding="utf-8") as handle:
        data = json.load(handle)
    if not isinstance(data, dict):
        raise ValueError(f"Atlas JSON must be an object: {path}")
    return data


def write_json(path: str | Path, data: Mapping[str, Any]) -> None:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(canonical_dumps(data), encoding="utf-8")


def slugify(value: str) -> str:
    slug = re.sub(r"[^a-zA-Z0-9_.-]+", "_", value.strip())
    slug = re.sub(r"_+", "_", slug).strip("._-")
    return slug or "unnamed"


def screen_filename(screen_id_or_name: str) -> str:
    slug = slugify(screen_id_or_name)
    return f"{slug if slug.startswith('screen_') else 'screen_' + slug}.json"


def edge_filename(edge_id_or_name: str) -> str:
    slug = slugify(edge_id_or_name)
    return f"{slug if slug.startswith('edge_') else 'edge_' + slug}.json"


def route_filename(route_name: str) -> str:
    return f"{slugify(route_name)}.atlas.json"


def atlas_dir(root: str | Path) -> Path:
    return Path(root) / ".atlas"


def screen_path(root: str | Path, screen_id: str) -> Path:
    return atlas_dir(root) / "graph" / "screens" / screen_filename(screen_id)


def edge_path(root: str | Path, edge_id: str) -> Path:
    return atlas_dir(root) / "graph" / "edges" / edge_filename(edge_id)


def route_path(root: str | Path, route_name: str) -> Path:
    return atlas_dir(root) / "routes" / route_filename(route_name)


def proposal_path(root: str | Path, proposal_id: str) -> Path:
    return atlas_dir(root) / "proposals" / f"{slugify(proposal_id)}.json"


def _iter_json_files(directory: Path, pattern: str = "*.json") -> Iterable[Path]:
    if not directory.exists():
        return ()
    return sorted(path for path in directory.glob(pattern) if path.is_file())


def _load_many(directory: Path, schema_version: str, pattern: str = "*.json") -> list[JsonObject]:
    loaded: list[JsonObject] = []
    for path in _iter_json_files(directory, pattern):
        data = read_json(path)
        if data.get("schema_version") != schema_version:
            raise ValueError(f"{path} has unsupported schema_version {data.get('schema_version')!r}")
        loaded.append(data)
    return loaded


def load_screens(root: str | Path) -> dict[str, ScreenNode]:
    directory = atlas_dir(root) / "graph" / "screens"
    screens = (ScreenNode.from_dict(data) for data in _load_many(directory, SCREEN_SCHEMA_VERSION))
    return {screen.id: screen for screen in screens}


def load_edges(root: str | Path) -> dict[str, NavigationEdge]:
    directory = atlas_dir(root) / "graph" / "edges"
    edges = (NavigationEdge.from_dict(data) for data in _load_many(directory, EDGE_SCHEMA_VERSION))
    return {edge.id: edge for edge in edges}


def load_routes(root: str | Path) -> dict[str, Route]:
    directory = atlas_dir(root) / "routes"
    routes = (Route.from_dict(data) for data in _load_many(directory, ROUTE_SCHEMA_VERSION, "*.atlas.json"))
    return {route.name: route for route in routes}


def load_graph(root: str | Path) -> dict[str, Any]:
    """Load graph objects by scanning object directories, not a central index."""

    return {
        "screens": load_screens(root),
        "edges": load_edges(root),
        "routes": load_routes(root),
    }


def write_screen(root: str | Path, screen: ScreenNode) -> Path:
    path = screen_path(root, screen.id)
    write_json(path, screen.to_dict())
    return path


def write_edge(root: str | Path, edge: NavigationEdge) -> Path:
    path = edge_path(root, edge.id)
    write_json(path, edge.to_dict())
    return path


def write_route(root: str | Path, route: Route) -> Path:
    path = route_path(root, route.name)
    write_json(path, route.to_dict())
    return path


def proposal_id_for(data: Mapping[str, Any]) -> str:
    digest = hashlib.sha256(canonical_dumps(data).encode("utf-8")).hexdigest()[:16]
    return f"proposal-{digest}"


def stage_proposal(root: str | Path, proposal: Proposal | Mapping[str, Any]) -> Path:
    data = proposal.to_dict() if isinstance(proposal, Proposal) else dict(proposal)
    data.setdefault("schema_version", PROPOSAL_SCHEMA_VERSION)
    data.setdefault("id", proposal_id_for(data))
    path = proposal_path(root, str(data["id"]))
    write_json(path, data)
    return path


def _write_object_change(root: str | Path, obj: Mapping[str, Any]) -> Path:
    parsed = model_from_dict(obj)
    if isinstance(parsed, ScreenNode):
        return write_screen(root, parsed)
    if isinstance(parsed, NavigationEdge):
        return write_edge(root, parsed)
    if isinstance(parsed, Route):
        return write_route(root, parsed)
    raise ValueError(f"proposal object cannot be accepted into graph storage: {obj.get('schema_version')!r}")


def accept_proposal(root: str | Path, proposal: str | Path | Proposal | Mapping[str, Any]) -> list[Path]:
    if isinstance(proposal, Proposal):
        data = proposal.to_dict()
    elif isinstance(proposal, Mapping):
        data = dict(proposal)
    else:
        path = Path(proposal)
        if not path.exists():
            path = proposal_path(root, str(proposal))
        data = read_json(path)

    if data.get("schema_version") != PROPOSAL_SCHEMA_VERSION:
        raise ValueError(f"unsupported proposal schema_version: {data.get('schema_version')!r}")

    written: list[Path] = []
    for change in data.get("changes", []):
        op = change.get("op", "upsert")
        if op not in {"add", "update", "upsert"}:
            raise ValueError(f"unsupported proposal change op: {op!r}")
        obj = change.get("object")
        if not isinstance(obj, Mapping):
            raise ValueError("proposal change must include an object")
        written.append(_write_object_change(root, obj))
    return written
