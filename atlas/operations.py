"""High-level Android runtime operations for Atlas commands."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable, Mapping, Optional, Protocol, Sequence, Union

from atlas.android import AdbExecutor, AndroidCliAdapter, TapPoint, parse_input_tap
from atlas.observation import ObservationRecorder


class SelectorNotFoundError(RuntimeError):
    """Raised when a selector cannot be resolved in the current layout."""

    code = "selector_not_found"

    def __init__(self, selector: str) -> None:
        super().__init__(f"Selector not found: {selector}")
        self.selector = selector

    def to_dict(self) -> dict[str, Any]:
        return {
            "status": "error",
            "error": {
                "code": self.code,
                "message": str(self),
                "selector": self.selector,
            },
        }


class CoordinateGuardError(RuntimeError):
    """Raised when a fast coordinate accelerator is used without a guard."""

    code = "coordinate_guard_required"


class ScreenMatcher(Protocol):
    """Optional graph-core-compatible matcher protocol."""

    def match(self, layout_json: Mapping[str, Any]) -> Mapping[str, Any]:
        """Return a compact current-screen match result."""


@dataclass(frozen=True)
class RuntimeMetrics:
    """Metrics that keep JSON responses compact but useful to agents."""

    layout_calls_total: int = 0
    layout_json_returned_to_agent: bool = False
    adb_taps_total: int = 0

    def to_dict(self) -> dict[str, Any]:
        return {
            "layout_calls_total": self.layout_calls_total,
            "layout_json_returned_to_agent": self.layout_json_returned_to_agent,
            "adb_taps_total": self.adb_taps_total,
        }


def layout(
    android: AndroidCliAdapter,
    *,
    diff: bool = False,
    observation: Optional[ObservationRecorder] = None,
) -> dict[str, Any]:
    """Return Android layout JSON or Android's in-session layout delta."""

    result = android.layout(diff=diff)
    payload = _json_or_raw(result.stdout)
    if observation is not None:
        kind = "android_layout_diff" if diff else "android_layout"
        observation.record_observation(kind=kind, payload={"output": payload})
    response = {
        "status": "ok",
        "kind": "android_layout_diff" if diff else "android_layout",
        "layout": payload,
        "metrics": RuntimeMetrics(
            layout_calls_total=1,
            layout_json_returned_to_agent=True,
        ).to_dict(),
    }
    if diff:
        response["diff_scope"] = "android_in_session"
    return response


def tap_selector(
    android: AndroidCliAdapter,
    adb: AdbExecutor,
    *,
    selector: str,
    reason: Optional[str] = None,
    observation: Optional[ObservationRecorder] = None,
) -> dict[str, Any]:
    """Resolve a layout selector and execute it with ``adb shell input tap``."""

    result = android.layout()
    layout_json = _require_json_object(result.stdout)
    point = resolve_selector_point(layout_json, selector)
    adb.tap(point.x, point.y)

    action = {
        "kind": "tap",
        "path": "layout_selector",
        "selector": selector,
        "point": {"x": point.x, "y": point.y},
    }
    if observation is not None:
        observation.record_observation(
            kind="android_layout",
            payload={"output": layout_json},
        )
        observation.record_action(kind="tap", payload=action, reason=reason)

    return {
        "status": "ok",
        "action": action,
        "metrics": RuntimeMetrics(
            layout_calls_total=1,
            layout_json_returned_to_agent=False,
            adb_taps_total=1,
        ).to_dict(),
    }


def tap_point(
    adb: AdbExecutor,
    *,
    x: int,
    y: int,
    mode: str = "verified",
    fast_coordinate_guard: Optional[Mapping[str, Any]] = None,
    observation: Optional[ObservationRecorder] = None,
) -> dict[str, Any]:
    """Execute an explicit point tap through adb."""

    if mode == "fast" and not fast_coordinate_guard:
        raise CoordinateGuardError("fast coordinate taps require a guard")
    adb.tap(x, y)
    action = {
        "kind": "tap",
        "path": "coordinate",
        "mode": mode,
        "point": {"x": x, "y": y},
    }
    if fast_coordinate_guard:
        action["fast_coordinate_guard"] = dict(fast_coordinate_guard)
    if observation is not None:
        observation.record_action(kind="tap", payload=action)
    return {
        "status": "ok",
        "action": action,
        "metrics": RuntimeMetrics(adb_taps_total=1).to_dict(),
    }


def tap_label(
    android: AndroidCliAdapter,
    adb: AdbExecutor,
    *,
    label: int,
    screenshot: Union[str, Path],
    observation: Optional[ObservationRecorder] = None,
) -> dict[str, Any]:
    """Resolve an annotated screenshot label and execute it with adb."""

    screenshot_path = str(screenshot)
    android.screen_capture(output=screenshot_path, annotate=True)
    resolve_result = android.screen_resolve(
        screenshot=screenshot_path,
        string=f"input tap #{label}",
    )
    point = parse_input_tap(resolve_result.stdout)
    adb.tap(point.x, point.y)
    action = {
        "kind": "tap",
        "path": "annotated_screenshot_label",
        "label": label,
        "screenshot": screenshot_path,
        "point": {"x": point.x, "y": point.y},
    }
    if observation is not None:
        observation.record_observation(
            kind="android_annotated_screenshot",
            payload={"path": screenshot_path, "label": label},
        )
        observation.record_action(kind="tap", payload=action)
    return {
        "status": "ok",
        "action": action,
        "metrics": RuntimeMetrics(adb_taps_total=1).to_dict(),
    }


def check_current_screen(
    android: AndroidCliAdapter,
    *,
    matcher: Optional[ScreenMatcher] = None,
) -> dict[str, Any]:
    """Capture layout and optionally delegate current-screen matching to graph core."""

    result = android.layout()
    layout_json = _require_json_object(result.stdout)
    match = matcher.match(layout_json) if matcher is not None else None
    return {
        "status": "ok",
        "current_screen": match or {"status": "unknown"},
        "metrics": RuntimeMetrics(
            layout_calls_total=1,
            layout_json_returned_to_agent=False,
        ).to_dict(),
    }


def execute_route(
    android: AndroidCliAdapter,
    adb: AdbExecutor,
    *,
    route: Mapping[str, Any],
    observation: Optional[ObservationRecorder] = None,
) -> dict[str, Any]:
    """Route execution skeleton for graph-core route plans."""

    steps = route.get("steps", [])
    if not isinstance(steps, Sequence):
        raise ValueError("route steps must be a sequence")

    executed: list[dict[str, Any]] = []
    layout_calls_total = 0
    adb_taps_total = 0
    for step in steps:
        if not isinstance(step, Mapping):
            raise ValueError("route step must be an object")
        action = step.get("action", {})
        if not isinstance(action, Mapping):
            raise ValueError("route step action must be an object")
        if action.get("kind") != "tap":
            raise ValueError(f"unsupported route action: {action.get('kind')}")

        if "selector" in action:
            result = tap_selector(
                android,
                adb,
                selector=str(action["selector"]),
                reason=str(step.get("reason", "")) or None,
                observation=observation,
            )
            layout_calls_total += 1
            adb_taps_total += 1
        elif "point" in action and isinstance(action["point"], Mapping):
            point = action["point"]
            result = tap_point(
                adb,
                x=int(point["x"]),
                y=int(point["y"]),
                observation=observation,
            )
            adb_taps_total += 1
        else:
            raise ValueError("tap action requires selector or point")
        executed.append(result["action"])

    return {
        "status": "ok",
        "route": route.get("name"),
        "mode": route.get("mode", "verified"),
        "executed": executed,
        "metrics": RuntimeMetrics(
            layout_calls_total=layout_calls_total,
            layout_json_returned_to_agent=False,
            adb_taps_total=adb_taps_total,
        ).to_dict(),
    }


def resolve_selector_point(layout_json: Mapping[str, Any], selector: str) -> TapPoint:
    """Resolve a simple Atlas selector against Android layout JSON."""

    key, expected = _parse_selector(selector)
    matches = []
    for node in _walk_nodes(layout_json):
        if _node_value(node, key) == expected:
            matches.append(node)
    if not matches:
        raise SelectorNotFoundError(selector)
    return _center_of(matches[0], selector)


def _parse_selector(selector: str) -> tuple[str, str]:
    if "=" not in selector:
        raise ValueError("selectors must use key=value syntax")
    key, value = selector.split("=", 1)
    key = key.strip()
    value = value.strip()
    aliases = {
        "content_desc": "contentDescription",
        "content_description": "contentDescription",
        "desc": "contentDescription",
        "id": "resourceId",
        "resource_id": "resourceId",
        "text": "text",
        "test_tag": "testTag",
    }
    return aliases.get(key, key), value


def _node_value(node: Mapping[str, Any], key: str) -> Optional[str]:
    value = node.get(key)
    if value is None and key == "contentDescription":
        value = node.get("content-desc")
    if value is None and key == "resourceId":
        value = node.get("resource-id")
    if value is None:
        return None
    return str(value)


def _walk_nodes(value: Any) -> Iterable[Mapping[str, Any]]:
    if isinstance(value, Mapping):
        yield value
        for child_key in ("children", "nodes"):
            children = value.get(child_key)
            if isinstance(children, list):
                for child in children:
                    yield from _walk_nodes(child)
    elif isinstance(value, list):
        for item in value:
            yield from _walk_nodes(item)


def _center_of(node: Mapping[str, Any], selector: str) -> TapPoint:
    bounds = node.get("bounds")
    if isinstance(bounds, Mapping):
        left = _first_number(bounds, "left", "x", "minX")
        top = _first_number(bounds, "top", "y", "minY")
        right = _first_number(bounds, "right", "maxX")
        bottom = _first_number(bounds, "bottom", "maxY")
        width = _first_number(bounds, "width")
        height = _first_number(bounds, "height")
        if right is None and left is not None and width is not None:
            right = left + width
        if bottom is None and top is not None and height is not None:
            bottom = top + height
        if None not in (left, top, right, bottom):
            return TapPoint(x=round((left + right) / 2), y=round((top + bottom) / 2))
    if isinstance(bounds, list) and len(bounds) == 4:
        left, top, right, bottom = [float(value) for value in bounds]
        return TapPoint(x=round((left + right) / 2), y=round((top + bottom) / 2))
    raise SelectorNotFoundError(f"{selector} (matched node has no tap bounds)")


def _first_number(mapping: Mapping[str, Any], *keys: str) -> Optional[float]:
    for key in keys:
        value = mapping.get(key)
        if isinstance(value, (int, float)):
            return float(value)
    return None


def _json_or_raw(value: str) -> Any:
    try:
        return json.loads(value)
    except json.JSONDecodeError:
        return value


def _require_json_object(value: str) -> Mapping[str, Any]:
    parsed = json.loads(value)
    if not isinstance(parsed, Mapping):
        raise ValueError("expected Android layout to return a JSON object")
    return parsed
