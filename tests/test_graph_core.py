from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from atlas.matching import REPAIR_CANDIDATE_THRESHOLD, match_screen
from atlas.models import GraphContext, NavigationEdge, Route, ScreenNode
from atlas.normalization import identity_hash, normalize_layout, normalized_identity
from atlas.redaction import policy_from_config
from atlas.routing import resolve_route
from atlas.storage import (
    canonical_dumps,
    edge_path,
    load_graph,
    route_path,
    screen_path,
    write_edge,
    write_json,
    write_route,
    write_screen,
)


class GraphCoreTests(unittest.TestCase):
    def test_redaction_runs_before_hashing_and_excludes_verbatim_text(self) -> None:
        first_layout = {
            "class": "Column",
            "children": [
                {"class": "Text", "text": "alice@example.com", "bounds_normalized": {"width": 0.5, "height": 0.1}},
                {"class": "Button", "text": "Reset password", "clickable": True},
            ],
        }
        second_layout = {
            "class": "Column",
            "children": [
                {"class": "Text", "text": "bob@example.com", "bounds_normalized": {"width": 0.5, "height": 0.1}},
                {"class": "Button", "text": "Reset token=abcd1234", "clickable": True},
            ],
        }

        first_normalized, first_hash = normalized_identity(first_layout)
        second_normalized, second_hash = normalized_identity(second_layout)

        self.assertEqual(first_hash, second_hash)
        committed = canonical_dumps(first_normalized)
        self.assertNotIn("alice@example.com", committed)
        self.assertNotIn("Reset password", committed)
        self.assertNotIn("bob@example.com", canonical_dumps(second_normalized))

    def test_allowlisted_static_text_does_not_force_raw_layout_storage(self) -> None:
        policy = policy_from_config({"allowlist_static_text": ["Home"]})
        normalized = normalize_layout({"class": "Text", "text": "Home"}, redaction_policy=policy)

        self.assertIn("text_class", normalized["elements"][0])
        self.assertNotIn("Home", canonical_dumps(normalized))

    def test_canonical_json_is_stable_and_sorted(self) -> None:
        data = {"z": 1, "a": {"b": 2, "a": 1}}

        self.assertEqual(
            canonical_dumps(data),
            '{\n  "a": {\n    "a": 1,\n    "b": 2\n  },\n  "z": 1\n}\n',
        )

        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "object.json"
            write_json(path, data)
            self.assertEqual(path.read_text(encoding="utf-8"), canonical_dumps(data))

    def test_context_guard_mismatch_returns_context_mismatch_not_route_broken(self) -> None:
        route = Route(
            name="open-settings",
            start={"screen": "home", "context_guard": {"auth_state": "logged_in"}},
            target={"screen": "settings"},
            preferred_edge_ids=["edge_home_settings"],
        )
        edge = NavigationEdge(
            id="edge_home_settings",
            from_screen="home",
            to_screen="settings",
            action={"kind": "tap", "selector_candidates": [{"kind": "test_tag", "value": "settings", "score": 0.9}]},
        )
        plan = resolve_route(
            "open-settings",
            current_screen="home",
            screens={},
            routes={route.name: route},
            edges={edge.id: edge},
            context=GraphContext({"auth_state": "logged_out"}),
        )

        result = plan.to_result()
        self.assertEqual(result.status, "context_mismatch")
        self.assertEqual(result.recommended_action, "establish required context or choose another route variant")

    def test_matching_uses_similarity_thresholds_after_hash_fast_path(self) -> None:
        baseline = {
            "schema_version": "atlas.normalized_layout.v1",
            "elements": [
                {"role": "Root", "path": "0", "clickable": False, "enabled": True},
                {"role": "Button", "path": "0/0", "resource_id": "home", "clickable": True, "enabled": True},
                {"role": "Text", "path": "0/1", "text_class": "medium", "clickable": False, "enabled": True},
                {"role": "Image", "path": "0/2", "clickable": False, "enabled": True},
                {"role": "Toolbar", "path": "0/3", "clickable": False, "enabled": True},
            ],
            "role_distribution": {"Button": 1, "Image": 1, "Root": 1, "Text": 1, "Toolbar": 1},
            "element_count": 5,
        }
        screen = ScreenNode(
            id="screen_home",
            name="home",
            identity_hash=identity_hash(baseline),
            normalized=baseline,
        )

        hash_result = match_screen(baseline, [screen])
        self.assertTrue(hash_result.hash_matched)
        self.assertEqual(hash_result.status, "matched")

        similar = {
            **baseline,
            "elements": [
                baseline["elements"][0],
                baseline["elements"][1],
                baseline["elements"][2],
                {"role": "Icon", "path": "0/2", "clickable": False, "enabled": True},
                baseline["elements"][4],
            ],
            "role_distribution": {"Button": 1, "Icon": 1, "Root": 1, "Text": 1, "Toolbar": 1},
        }
        similar_result = match_screen(similar, [screen])
        self.assertFalse(similar_result.hash_matched)
        self.assertEqual(similar_result.status, "matched")
        self.assertGreaterEqual(similar_result.match_confidence, 0.78)

        repairish = {
            **baseline,
            "elements": [
                baseline["elements"][0],
                baseline["elements"][1],
                {"role": "Text", "path": "0/1", "text_class": "long", "clickable": False, "enabled": True},
                {"role": "Image", "path": "0/2", "clickable": True, "enabled": True},
                baseline["elements"][4],
            ],
            "role_distribution": {"Button": 1, "Image": 1, "Root": 1, "Text": 1, "Toolbar": 1},
        }
        repair_result = match_screen(repairish, [screen])
        self.assertEqual(repair_result.status, "repair_candidate")
        self.assertGreaterEqual(repair_result.match_confidence, REPAIR_CANDIDATE_THRESHOLD)
        self.assertLess(repair_result.match_confidence, 0.78)

    def test_graph_fallback_routing_uses_edges_when_preferred_path_missing(self) -> None:
        article = ScreenNode("screen_article", "article", "sha256:article", aliases=["story"])
        route = Route(
            name="read-article",
            start={"screen": "home"},
            target={"screen": "article"},
            preferred_edge_ids=["edge_missing"],
            allow_graph_fallback=True,
            path_constraints={"max_edges": 3},
        )
        first = NavigationEdge(
            id="edge_home_feed",
            from_screen="home",
            to_screen="feed",
            action={"kind": "tap", "selector_candidates": [{"kind": "test_tag", "value": "feed", "score": 0.9}]},
            confidence_model={"transition_confidence": 0.9},
        )
        second = NavigationEdge(
            id="edge_feed_article",
            from_screen="feed",
            to_screen="article",
            action={"kind": "tap", "selector_candidates": [{"kind": "accessibility", "value": "article", "score": 0.8}]},
            confidence_model={"transition_confidence": 0.9},
        )

        plan = resolve_route(
            "read-article",
            current_screen="home",
            screens={article.id: article},
            routes={route.name: route},
            edges={first.id: first, second.id: second},
            context=GraphContext({}),
        )

        self.assertEqual(plan.status, "ok")
        self.assertTrue(plan.graph_fallback_used)
        self.assertFalse(plan.preferred_path_used)
        self.assertEqual([edge.id for edge in plan.edges], ["edge_home_feed", "edge_feed_article"])

    def test_graph_storage_loads_without_central_index(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            screen = ScreenNode("screen_home", "home", "sha256:home")
            edge = NavigationEdge(
                id="edge_home_settings",
                from_screen="home",
                to_screen="settings",
                action={"kind": "tap", "selector_candidates": [{"kind": "test_tag", "value": "settings", "score": 0.9}]},
            )
            route = Route(name="open-settings", start={"screen": "home"}, target={"screen": "settings"})
            write_screen(root, screen)
            write_edge(root, edge)
            write_route(root, route)

            index = root / ".atlas" / "graph" / "index.json"
            index.write_text(json.dumps({"broken": True}), encoding="utf-8")

            graph = load_graph(root)

            self.assertFalse((root / ".atlas" / "graph" / "index-cache.json").exists())
            self.assertIn(screen.id, graph["screens"])
            self.assertIn(edge.id, graph["edges"])
            self.assertIn(route.name, graph["routes"])
            self.assertEqual(screen_path(root, "screen_home").name, "screen_home.json")
            self.assertEqual(edge_path(root, "edge_home_settings").name, "edge_home_settings.json")
            self.assertEqual(route_path(root, "open-settings").name, "open-settings.atlas.json")


if __name__ == "__main__":
    unittest.main()
