"""Route lookup and graph fallback over Atlas navigation edges."""

from __future__ import annotations

import heapq
from dataclasses import dataclass, field
from typing import Mapping

from .models import GraphContext, NavigationEdge, Result, Route, ScreenNode, context_guard_mismatches


@dataclass(frozen=True)
class RoutePlan:
    status: str
    target: str
    start_screen: str | None = None
    route: Route | None = None
    edges: list[NavigationEdge] = field(default_factory=list)
    preferred_path_used: bool = False
    graph_fallback_used: bool = False
    route_confidence: float = 0.0
    reason: str | None = None
    recommended_action: str | None = None
    context_mismatches: list[str] = field(default_factory=list)

    def to_result(self) -> Result:
        data = {
            "target": self.target,
            "start_screen": self.start_screen,
            "edge_ids": [edge.id for edge in self.edges],
            "preferred_path_used": self.preferred_path_used,
            "graph_fallback_used": self.graph_fallback_used,
            "route_confidence": self.route_confidence,
        }
        if self.context_mismatches:
            return Result.context_mismatch(mismatches=self.context_mismatches, message=self.reason)
        if self.status == "ok":
            return Result.ok(data=data)
        if self.status == "route_broken":
            return Result.route_broken(self.reason or "route could not reach target", data=data)
        return Result.not_found(self.reason or "route not found", data=data)


def _screen_matches_target(screen: ScreenNode, target: str) -> bool:
    normalized = target.lower()
    names = {screen.id.lower(), screen.name.lower(), *(alias.lower() for alias in screen.aliases)}
    return normalized in names


def find_target_screen(target: str, screens: Mapping[str, ScreenNode]) -> str | None:
    for screen in screens.values():
        if _screen_matches_target(screen, target):
            return screen.name
    return None


def lookup_route(target: str, routes: Mapping[str, Route], screens: Mapping[str, ScreenNode]) -> Route | None:
    needle = target.lower()
    for route in routes.values():
        haystack = {
            route.name.lower(),
            *(alias.lower() for alias in route.aliases),
        }
        if route.intent:
            haystack.add(route.intent.lower())
        for trigger in route.triggers:
            if isinstance(trigger, Mapping) and trigger.get("kind") == "intent":
                haystack.add(str(trigger.get("value", "")).lower())
        if needle in haystack:
            return route
    screen_name = find_target_screen(target, screens)
    if screen_name:
        for route in routes.values():
            if route.target_screen() == screen_name:
                return route
    return None


def _edge_cost(edge: NavigationEdge, context: GraphContext | Mapping[str, object] | None) -> float:
    cost = 1.0 + edge.selector_fragility_penalty()
    if context_guard_mismatches(context, edge.context_guard):
        cost += 1000.0
    if edge.context_guard and not context:
        cost += 0.25
    transition_confidence = float(edge.confidence_model.get("transition_confidence", 0.75))
    cost += max(0.0, 1.0 - transition_confidence)
    return cost


def _confidence_for_cost(cost: float, edge_count: int) -> float:
    if edge_count <= 0:
        return 0.0
    return max(0.0, min(1.0, 1.0 / (1.0 + (cost - edge_count) / edge_count)))


def _preferred_path(
    route: Route,
    edges: Mapping[str, NavigationEdge],
    context: GraphContext | Mapping[str, object] | None,
) -> tuple[list[NavigationEdge], str | None, list[str]]:
    path: list[NavigationEdge] = []
    mismatches: list[str] = []
    for edge_id in route.preferred_edge_ids:
        edge = edges.get(edge_id)
        if edge is None:
            return [], f"preferred edge {edge_id!r} is missing", mismatches
        edge_mismatches = context_guard_mismatches(context, edge.context_guard)
        if edge_mismatches:
            mismatches.extend(edge_mismatches)
            return [], "preferred path context guard mismatch", mismatches
        path.append(edge)
    if path and route.target_screen() and path[-1].to_screen != route.target_screen():
        return [], "preferred path does not reach route target", mismatches
    return path, None, mismatches


def graph_fallback_path(
    start_screen: str,
    target_screen: str,
    edges: Mapping[str, NavigationEdge],
    context: GraphContext | Mapping[str, object] | None = None,
    *,
    max_edges: int = 8,
    avoid_screens: set[str] | None = None,
) -> tuple[list[NavigationEdge], float, list[str]]:
    avoid = avoid_screens or set()
    queue: list[tuple[float, int, str, list[NavigationEdge]]] = [(0.0, 0, start_screen, [])]
    best_cost: dict[str, float] = {start_screen: 0.0}
    skipped_mismatches: list[str] = []

    outgoing: dict[str, list[NavigationEdge]] = {}
    for edge in edges.values():
        outgoing.setdefault(edge.from_screen, []).append(edge)

    while queue:
        cost, hops, screen, path = heapq.heappop(queue)
        if screen == target_screen:
            return path, cost, skipped_mismatches
        if hops >= max_edges:
            continue
        for edge in sorted(outgoing.get(screen, []), key=lambda item: item.id):
            if edge.to_screen in avoid:
                continue
            mismatches = context_guard_mismatches(context, edge.context_guard)
            if mismatches:
                skipped_mismatches.extend(mismatches)
                continue
            next_cost = cost + _edge_cost(edge, context)
            if next_cost >= best_cost.get(edge.to_screen, float("inf")):
                continue
            best_cost[edge.to_screen] = next_cost
            heapq.heappush(queue, (next_cost, hops + 1, edge.to_screen, [*path, edge]))
    return [], float("inf"), skipped_mismatches


def resolve_route(
    target: str,
    *,
    current_screen: str | None,
    screens: Mapping[str, ScreenNode],
    edges: Mapping[str, NavigationEdge],
    routes: Mapping[str, Route],
    context: GraphContext | Mapping[str, object] | None = None,
) -> RoutePlan:
    route = lookup_route(target, routes, screens)
    target_screen = route.target_screen() if route else find_target_screen(target, screens)
    if target_screen is None:
        return RoutePlan(status="not_found", target=target, reason="no matching route or screen")

    start_screen = current_screen or (route.start_screen() if route else None)
    if start_screen is None:
        return RoutePlan(status="not_found", target=target, reason="no start screen available")

    if route:
        route_mismatches = context_guard_mismatches(context, route.combined_context_guard())
        if route_mismatches:
            return RoutePlan(
                status="context_mismatch",
                target=target_screen,
                start_screen=start_screen,
                route=route,
                reason="route context guard mismatch",
                recommended_action="establish required context or choose another route variant",
                context_mismatches=route_mismatches,
            )
        preferred, reason, edge_mismatches = _preferred_path(route, edges, context)
        if edge_mismatches:
            return RoutePlan(
                status="context_mismatch",
                target=target_screen,
                start_screen=start_screen,
                route=route,
                reason=reason,
                recommended_action="establish required context or choose another route variant",
                context_mismatches=edge_mismatches,
            )
        if preferred:
            cost = sum(_edge_cost(edge, context) for edge in preferred)
            return RoutePlan(
                status="ok",
                target=target_screen,
                start_screen=start_screen,
                route=route,
                edges=preferred,
                preferred_path_used=True,
                route_confidence=_confidence_for_cost(cost, len(preferred)),
            )
        if not route.allow_graph_fallback:
            return RoutePlan(
                status="route_broken",
                target=target_screen,
                start_screen=start_screen,
                route=route,
                reason=reason or "preferred path unavailable and graph fallback disabled",
            )

    constraints = route.path_constraints if route else {}
    max_edges = int(constraints.get("max_edges", 8)) if isinstance(constraints, Mapping) else 8
    avoid = set(constraints.get("avoid_screens", [])) if isinstance(constraints, Mapping) else set()
    fallback, cost, mismatches = graph_fallback_path(
        start_screen,
        target_screen,
        edges,
        context,
        max_edges=max_edges,
        avoid_screens=avoid,
    )
    if fallback:
        return RoutePlan(
            status="ok",
            target=target_screen,
            start_screen=start_screen,
            route=route,
            edges=fallback,
            graph_fallback_used=True,
            route_confidence=_confidence_for_cost(cost, len(fallback)),
            reason="preferred path skipped" if route else "graph path found",
        )
    if mismatches:
        return RoutePlan(
            status="context_mismatch",
            target=target_screen,
            start_screen=start_screen,
            route=route,
            reason="all candidate graph paths failed context guards",
            context_mismatches=sorted(set(mismatches)),
        )
    return RoutePlan(
        status="route_broken" if route else "not_found",
        target=target_screen,
        start_screen=start_screen,
        route=route,
        reason="no graph path reaches target",
    )
