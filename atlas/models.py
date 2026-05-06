"""Dataclasses and helpers for Atlas committed graph objects."""

from __future__ import annotations

from dataclasses import dataclass, field, fields, is_dataclass
from typing import Any, Mapping


SCREEN_SCHEMA_VERSION = "atlas.screen.v1"
EDGE_SCHEMA_VERSION = "atlas.edge.v1"
ROUTE_SCHEMA_VERSION = "atlas.route.v1"
CHECK_SCHEMA_VERSION = "atlas.check.v1"
PROPOSAL_SCHEMA_VERSION = "atlas.proposal.v1"
RESULT_SCHEMA_VERSION = "atlas.result.v1"
CONTEXT_SCHEMA_VERSION = "atlas.context.v1"


JsonObject = dict[str, Any]


def _without_none(value: Any) -> Any:
    if is_dataclass(value):
        return {
            f.name: _without_none(getattr(value, f.name))
            for f in fields(value)
            if getattr(value, f.name) is not None
        }
    if isinstance(value, dict):
        return {str(k): _without_none(v) for k, v in value.items() if v is not None}
    if isinstance(value, list):
        return [_without_none(v) for v in value]
    return value


def _guard_allows_unknown(required: str) -> bool:
    return required == "unknown" or required.endswith("_or_unknown")


def _match_scalar(current: Any, required: Any) -> bool:
    if required in (None, "any"):
        return True
    if isinstance(required, (list, tuple, set, frozenset)):
        return any(_match_scalar(current, option) for option in required)
    if isinstance(required, bool):
        return current is required
    if current in (None, ""):
        return False
    if isinstance(required, str):
        if _guard_allows_unknown(required) and current == "unknown":
            return True
        if required.endswith("_or_unknown"):
            return current == required.removesuffix("_or_unknown")
    return current == required


def _mismatch_path(prefix: str, key: str) -> str:
    return f"{prefix}.{key}" if prefix else key


def context_guard_mismatches(
    current: Mapping[str, Any] | "GraphContext" | None,
    guard: Mapping[str, Any] | "GraphContext" | None,
    *,
    _prefix: str = "",
) -> list[str]:
    """Return guard keys that the current context cannot satisfy."""

    current_values = current.values if isinstance(current, GraphContext) else dict(current or {})
    guard_values = guard.values if isinstance(guard, GraphContext) else dict(guard or {})
    mismatches: list[str] = []

    for key, required in guard_values.items():
        if required in (None, "any"):
            continue
        current_value = current_values.get(key)
        path = _mismatch_path(_prefix, key)
        if isinstance(required, Mapping):
            if not isinstance(current_value, Mapping):
                mismatches.append(path)
                continue
            mismatches.extend(
                context_guard_mismatches(current_value, required, _prefix=path)
            )
        elif not _match_scalar(current_value, required):
            mismatches.append(path)
    return mismatches


@dataclass(frozen=True)
class GraphContext:
    values: JsonObject = field(default_factory=dict)
    schema_version: str = CONTEXT_SCHEMA_VERSION

    def satisfies(self, guard: Mapping[str, Any] | "GraphContext" | None) -> bool:
        return not context_guard_mismatches(self, guard)

    def mismatches(self, guard: Mapping[str, Any] | "GraphContext" | None) -> list[str]:
        return context_guard_mismatches(self, guard)

    def to_dict(self) -> JsonObject:
        data = dict(self.values)
        data["schema_version"] = self.schema_version
        return data

    @classmethod
    def from_dict(cls, data: Mapping[str, Any] | None) -> "GraphContext":
        values = dict(data or {})
        schema_version = values.pop("schema_version", CONTEXT_SCHEMA_VERSION)
        return cls(values=values, schema_version=schema_version)


@dataclass
class TapRecipe:
    kind: str = "tap"
    description: str | None = None
    selector_candidates: list[JsonObject] = field(default_factory=list)
    tap_cache: JsonObject | None = None

    def to_dict(self) -> JsonObject:
        return _without_none(self)

    @classmethod
    def from_dict(cls, data: Mapping[str, Any] | None) -> "TapRecipe":
        return cls(**dict(data or {}))


@dataclass
class Check:
    kind: str
    selector: JsonObject | None = None
    screen: str | None = None
    required: bool = True
    context_guard: JsonObject = field(default_factory=dict)
    schema_version: str = CHECK_SCHEMA_VERSION

    def context_result(self, context: GraphContext | Mapping[str, Any] | None) -> "Result":
        mismatches = context_guard_mismatches(context, self.context_guard)
        if mismatches:
            return Result.context_mismatch(mismatches=mismatches)
        return Result.ok(data={"check": self.kind})

    def to_dict(self) -> JsonObject:
        return _without_none(self)

    @classmethod
    def from_dict(cls, data: Mapping[str, Any]) -> "Check":
        return cls(**dict(data))


@dataclass
class ScreenNode:
    id: str
    name: str
    identity_hash: str
    context_guard: JsonObject = field(default_factory=dict)
    match_profile: JsonObject = field(default_factory=dict)
    checks: list[JsonObject] = field(default_factory=list)
    aliases: list[str] = field(default_factory=list)
    source: JsonObject = field(default_factory=dict)
    normalized: JsonObject | None = None
    schema_version: str = SCREEN_SCHEMA_VERSION

    def context_result(self, context: GraphContext | Mapping[str, Any] | None) -> "Result":
        mismatches = context_guard_mismatches(context, self.context_guard)
        if mismatches:
            return Result.context_mismatch(mismatches=mismatches)
        return Result.ok(data={"screen": self.name})

    def to_dict(self) -> JsonObject:
        return _without_none(self)

    @classmethod
    def from_dict(cls, data: Mapping[str, Any]) -> "ScreenNode":
        return cls(**dict(data))


@dataclass
class NavigationEdge:
    id: str
    from_screen: str
    to_screen: str
    action: TapRecipe | JsonObject
    intent: str | None = None
    context_guard: JsonObject = field(default_factory=dict)
    expectations: list[JsonObject] = field(default_factory=list)
    learned_from: JsonObject = field(default_factory=dict)
    confidence_model: JsonObject = field(default_factory=dict)
    schema_version: str = EDGE_SCHEMA_VERSION

    def __post_init__(self) -> None:
        if isinstance(self.action, Mapping):
            self.action = TapRecipe.from_dict(self.action)

    def context_result(self, context: GraphContext | Mapping[str, Any] | None) -> "Result":
        mismatches = context_guard_mismatches(context, self.context_guard)
        if mismatches:
            return Result.context_mismatch(mismatches=mismatches)
        return Result.ok(data={"edge": self.id})

    def selector_fragility_penalty(self) -> float:
        candidates = self.action.selector_candidates if isinstance(self.action, TapRecipe) else []
        if not candidates:
            return 0.5
        best_score = max(float(candidate.get("score", 0.0)) for candidate in candidates)
        coordinate_only = all(
            candidate.get("kind") == "normalized_coordinate" for candidate in candidates
        )
        return max(0.0, 1.0 - best_score) + (0.5 if coordinate_only else 0.0)

    def to_dict(self) -> JsonObject:
        return _without_none(self)

    @classmethod
    def from_dict(cls, data: Mapping[str, Any]) -> "NavigationEdge":
        return cls(**dict(data))


@dataclass
class Route:
    name: str
    target: JsonObject
    intent: str | None = None
    start: JsonObject = field(default_factory=dict)
    preferred_edge_ids: list[str] = field(default_factory=list)
    allow_graph_fallback: bool = True
    path_constraints: JsonObject = field(default_factory=dict)
    checks: list[JsonObject] = field(default_factory=list)
    triggers: list[JsonObject] = field(default_factory=list)
    aliases: list[str] = field(default_factory=list)
    context_guard: JsonObject = field(default_factory=dict)
    schema_version: str = ROUTE_SCHEMA_VERSION

    def combined_context_guard(self) -> JsonObject:
        guard = dict(self.context_guard)
        start_guard = self.start.get("context_guard") if isinstance(self.start, Mapping) else None
        if isinstance(start_guard, Mapping):
            guard.update(start_guard)
        return guard

    def context_result(self, context: GraphContext | Mapping[str, Any] | None) -> "Result":
        mismatches = context_guard_mismatches(context, self.combined_context_guard())
        if mismatches:
            return Result.context_mismatch(mismatches=mismatches)
        return Result.ok(data={"route": self.name})

    def target_screen(self) -> str | None:
        screen = self.target.get("screen")
        return str(screen) if screen else None

    def start_screen(self) -> str | None:
        screen = self.start.get("screen")
        return str(screen) if screen else None

    def to_dict(self) -> JsonObject:
        return _without_none(self)

    @classmethod
    def from_dict(cls, data: Mapping[str, Any]) -> "Route":
        return cls(**dict(data))


@dataclass
class Proposal:
    id: str
    kind: str
    changes: list[JsonObject] = field(default_factory=list)
    reason: str | None = None
    schema_version: str = PROPOSAL_SCHEMA_VERSION

    def to_dict(self) -> JsonObject:
        return _without_none(self)

    @classmethod
    def from_dict(cls, data: Mapping[str, Any]) -> "Proposal":
        return cls(**dict(data))


@dataclass
class Result:
    status: str
    data: JsonObject = field(default_factory=dict)
    message: str | None = None
    recommended_action: str | None = None
    schema_version: str = RESULT_SCHEMA_VERSION

    def to_dict(self) -> JsonObject:
        return _without_none(self)

    @classmethod
    def ok(cls, data: JsonObject | None = None, message: str | None = None) -> "Result":
        return cls(status="ok", data=data or {}, message=message)

    @classmethod
    def not_found(cls, message: str, data: JsonObject | None = None) -> "Result":
        return cls(status="not_found", message=message, data=data or {})

    @classmethod
    def route_broken(cls, message: str, data: JsonObject | None = None) -> "Result":
        return cls(
            status="route_broken",
            message=message,
            data=data or {},
            recommended_action="inspect the app and stage a graph repair proposal",
        )

    @classmethod
    def context_mismatch(
        cls,
        *,
        mismatches: list[str],
        message: str | None = None,
    ) -> "Result":
        return cls(
            status="context_mismatch",
            message=message or "current context does not satisfy the graph guard",
            data={"mismatches": mismatches},
            recommended_action="establish required context or choose another route variant",
        )


def model_from_dict(data: Mapping[str, Any]) -> ScreenNode | NavigationEdge | Route | Check | Proposal:
    schema = data.get("schema_version")
    if schema == SCREEN_SCHEMA_VERSION:
        return ScreenNode.from_dict(data)
    if schema == EDGE_SCHEMA_VERSION:
        return NavigationEdge.from_dict(data)
    if schema == ROUTE_SCHEMA_VERSION:
        return Route.from_dict(data)
    if schema == CHECK_SCHEMA_VERSION:
        return Check.from_dict(data)
    if schema == PROPOSAL_SCHEMA_VERSION:
        return Proposal.from_dict(data)
    raise ValueError(f"unsupported Atlas schema_version: {schema!r}")
