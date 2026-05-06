from __future__ import annotations

import io
import json
import tempfile
import unittest
from pathlib import Path

from atlas.cli import main
from atlas.config import default_config, write_json
from atlas.models import NavigationEdge, Proposal, Route, ScreenNode
from atlas.storage import stage_proposal, write_edge, write_route, write_screen


class CliCommandTests(unittest.TestCase):
    def test_route_command_returns_graph_plan_json(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_screen(root, ScreenNode("screen_article", "article-detail", "sha256:article", aliases=["article"]))
            write_edge(
                root,
                NavigationEdge(
                    id="edge_home_article",
                    from_screen="home",
                    to_screen="article-detail",
                    action={
                        "kind": "tap",
                        "selector_candidates": [
                            {"kind": "test_tag", "value": "article_card", "score": 0.92}
                        ],
                    },
                ),
            )
            write_route(
                root,
                Route(
                    name="read-article",
                    start={"screen": "home"},
                    target={"screen": "article-detail"},
                    preferred_edge_ids=["edge_home_article"],
                ),
            )

            stdout = io.StringIO()
            code = main(["route", "article", "--current-screen", "home", "--json"], cwd=root, stdout=stdout)

            self.assertEqual(code, 0)
            payload = json.loads(stdout.getvalue())
            self.assertEqual(payload["status"], "ok")
            self.assertEqual(payload["data"]["edge_ids"], ["edge_home_article"])
            self.assertTrue(payload["data"]["preferred_path_used"])

    def test_route_command_reports_context_mismatch_exit_code(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = default_config()
            config["context"]["auth_state"] = "logged_out"
            config_path = root / ".atlas" / "config.json"
            config_path.parent.mkdir(parents=True)
            write_json(config_path, config)
            write_route(
                root,
                Route(
                    name="open-account",
                    start={"screen": "home", "context_guard": {"auth_state": "logged_in"}},
                    target={"screen": "account"},
                ),
            )

            stdout = io.StringIO()
            code = main(["route", "open-account", "--current-screen", "home", "--json"], cwd=root, stdout=stdout)

            self.assertEqual(code, 8)
            payload = json.loads(stdout.getvalue())
            self.assertEqual(payload["status"], "context_mismatch")
            self.assertEqual(payload["recommended_action"], "establish required context or choose another route variant")

    def test_observe_and_learn_stage_review_proposal_without_accepting(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            start_stdout = io.StringIO()
            learn_stdout = io.StringIO()

            self.assertEqual(main(["observe", "start", "article-detail", "--json"], cwd=root, stdout=start_stdout), 0)
            learn_code = main(["learn", "--from-current-run", "--stage", "--json"], cwd=root, stdout=learn_stdout)

            self.assertEqual(learn_code, 1)
            payload = json.loads(learn_stdout.getvalue())
            self.assertEqual(payload["status"], "changed_requires_review")
            self.assertTrue(payload["human_approval_required"])
            self.assertTrue((root / payload["proposal_path"]).exists() if not Path(payload["proposal_path"]).is_absolute() else Path(payload["proposal_path"]).exists())
            self.assertFalse((root / ".atlas" / "graph" / "screens").exists())

    def test_accept_command_applies_explicit_proposal(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            screen = ScreenNode("screen_home", "home", "sha256:home")
            proposal = Proposal(
                id="proposal-home",
                kind="screen_added",
                changes=[{"op": "add", "object": screen.to_dict()}],
            )
            stage_proposal(root, proposal)

            stdout = io.StringIO()
            code = main(["accept", "proposal-home", "--json"], cwd=root, stdout=stdout)

            self.assertEqual(code, 0)
            payload = json.loads(stdout.getvalue())
            self.assertEqual(payload["status"], "ok")
            self.assertTrue((root / ".atlas" / "graph" / "screens" / "screen_home.json").exists())

    def test_tap_fast_point_requires_coordinate_guard_before_adb(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            stdout = io.StringIO()
            code = main(["tap", "--point", "1,2", "--mode", "fast", "--json"], cwd=tmp, stdout=stdout)

            self.assertEqual(code, 7)
            payload = json.loads(stdout.getvalue())
            self.assertEqual(payload["status"], "config_error")
            self.assertEqual(payload["error"]["code"], "coordinate_guard_required")


if __name__ == "__main__":
    unittest.main()
