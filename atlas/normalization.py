"""Normalize Android layout-ish JSON into stable, redacted signatures."""

from __future__ import annotations

import hashlib
from collections import Counter
from typing import Any, Mapping

from .redaction import DEFAULT_POLICY, RedactionPolicy, redact_layout
from .storage import canonical_dumps


VOLATILE_KEYS = {
    "timestamp",
    "time",
    "elapsedRealtime",
    "frame",
    "counter",
    "index",
    "focused",
    "selected",
    "pressed",
    "scrollX",
    "scrollY",
    "animation",
    "bounds",
    "raw_bounds",
}

CHILD_KEYS = ("children", "nodes", "elements")


def _first(data: Mapping[str, Any], keys: tuple[str, ...], default: Any = None) -> Any:
    for key in keys:
        if key in data:
            return data[key]
    return default


def _bounds_bucket(bounds: Any) -> str | None:
    if isinstance(bounds, Mapping):
        left = bounds.get("left", bounds.get("x", 0))
        top = bounds.get("top", bounds.get("y", 0))
        right = bounds.get("right", left + bounds.get("width", 0))
        bottom = bounds.get("bottom", top + bounds.get("height", 0))
        try:
            width = max(float(right) - float(left), 0.0)
            height = max(float(bottom) - float(top), 0.0)
        except (TypeError, ValueError):
            return None
        area = width * height
        if area <= 0:
            return None
        if area < 0.01:
            return "tiny"
        if area < 0.08:
            return "small"
        if area < 0.35:
            return "medium"
        return "large"
    return None


def _stable_resource_id(value: Any) -> str | None:
    if not isinstance(value, str) or not value:
        return None
    lowered = value.lower()
    dynamic_markers = ("generated", "uuid", "random", "timestamp", "session")
    if any(marker in lowered for marker in dynamic_markers):
        return None
    return value


def _text_class(value: Any) -> str | None:
    if isinstance(value, Mapping) and value.get("redacted"):
        return str(value.get("text_class", "redacted"))
    if isinstance(value, str):
        stripped = value.strip()
        if not stripped:
            return "empty"
        if stripped.isdigit():
            return "numeric"
        if len(stripped) <= 3:
            return "short"
        if len(stripped) <= 24:
            return "medium"
        return "long"
    return None


def _child_list(node: Mapping[str, Any]) -> list[Any]:
    for key in CHILD_KEYS:
        value = node.get(key)
        if isinstance(value, list):
            return value
    return []


def _signature_for_node(node: Mapping[str, Any], path: str, sibling_index: int) -> dict[str, Any]:
    role = _first(node, ("role", "class", "className", "type"), "unknown")
    signature: dict[str, Any] = {
        "role": str(role),
        "clickable": bool(node.get("clickable", False)),
        "enabled": bool(node.get("enabled", True)),
        "path": path,
        "sibling_bucket": min(sibling_index, 9),
    }

    resource_id = _stable_resource_id(_first(node, ("resource_id", "resourceId", "id", "testTag")))
    if resource_id:
        signature["resource_id"] = resource_id

    text_class = _text_class(_first(node, ("text", "label", "contentDescription", "content_description")))
    if text_class:
        signature["text_class"] = text_class

    bounds_bucket = _bounds_bucket(_first(node, ("bounds_normalized", "normalized_bounds")))
    if bounds_bucket:
        signature["bounds_bucket"] = bounds_bucket

    return signature


def _walk(node: Any, path: str = "0", sibling_index: int = 0) -> list[dict[str, Any]]:
    if not isinstance(node, Mapping):
        return []
    signatures = [_signature_for_node(node, path, sibling_index)]
    role_counts: Counter[str] = Counter()
    for index, child in enumerate(_child_list(node)):
        child_path = f"{path}/{index}"
        child_sigs = _walk(child, child_path, index)
        signatures.extend(child_sigs)
        if child_sigs:
            role_counts[child_sigs[0]["role"]] += 1
    if role_counts and len(role_counts) == 1 and sum(role_counts.values()) >= 6:
        signatures[0]["repeating_children_collapsed"] = True
    return signatures


def _strip_volatile(value: Any) -> Any:
    if isinstance(value, Mapping):
        return {
            str(key): _strip_volatile(child)
            for key, child in value.items()
            if str(key) not in VOLATILE_KEYS
        }
    if isinstance(value, list):
        return [_strip_volatile(child) for child in value]
    return value


def normalize_layout(
    layout: Mapping[str, Any],
    *,
    redaction_policy: RedactionPolicy = DEFAULT_POLICY,
) -> dict[str, Any]:
    redacted = redact_layout(layout, redaction_policy)
    stripped = _strip_volatile(redacted)
    elements = _walk(stripped)
    role_distribution = dict(sorted(Counter(element["role"] for element in elements).items()))
    return {
        "schema_version": "atlas.normalized_layout.v1",
        "elements": elements,
        "role_distribution": role_distribution,
        "element_count": len(elements),
    }


def element_key(element: Mapping[str, Any]) -> str:
    parts = [
        str(element.get("role", "")),
        str(element.get("resource_id", "")),
        str(element.get("text_class", "")),
        str(element.get("clickable", "")),
        str(element.get("enabled", "")),
        str(element.get("bounds_bucket", "")),
    ]
    return "|".join(parts)


def identity_hash(normalized: Mapping[str, Any]) -> str:
    digest = hashlib.sha256(canonical_dumps(normalized).encode("utf-8")).hexdigest()
    return f"sha256:{digest}"


def normalized_identity(layout: Mapping[str, Any], policy: RedactionPolicy = DEFAULT_POLICY) -> tuple[dict[str, Any], str]:
    normalized = normalize_layout(layout, redaction_policy=policy)
    return normalized, identity_hash(normalized)
