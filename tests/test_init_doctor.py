from __future__ import annotations

import io
import json
import tempfile
import unittest
from pathlib import Path

from atlas.cli import main
from atlas.init_cmd import SKILL_BODY


class InitDoctorTests(unittest.TestCase):
    def test_init_dry_run_reports_without_writing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            stdout = io.StringIO()
            code = main(["init", "--dry-run", "--json", "--agents", "all"], cwd=tmp, stdout=stdout)

            self.assertEqual(code, 0)
            payload = json.loads(stdout.getvalue())
            self.assertTrue(payload["ok"])
            self.assertTrue(payload["dry_run"])
            self.assertFalse((Path(tmp) / ".atlas").exists())
            self.assertIn(".agents/skills/atlas-app-navigation/SKILL.md", payload["skill_paths"])
            self.assertTrue(any(change["status"] == "planned" for change in payload["changes"]))

    def test_init_creates_expected_tree_gitignore_and_skill_idempotently(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            first = main(["init", "--json", "--agents", "codex"], cwd=root, stdout=io.StringIO())
            second = main(["init", "--json", "--agents", "codex"], cwd=root, stdout=io.StringIO())

            self.assertEqual(first, 0)
            self.assertEqual(second, 0)
            for rel_path in [
                ".atlas/config.json",
                ".atlas/graph/screens",
                ".atlas/graph/edges",
                ".atlas/routes",
                ".atlas/checks",
                ".atlas/proposals",
                ".atlas/runs",
                ".atlas/state",
            ]:
                self.assertTrue((root / rel_path).exists(), rel_path)

            gitignore = (root / ".gitignore").read_text(encoding="utf-8")
            self.assertEqual(gitignore.count(".atlas/runs/"), 1)
            self.assertEqual(gitignore.count(".atlas/state/"), 1)
            self.assertEqual((root / ".agents/skills/atlas-app-navigation/SKILL.md").read_text(encoding="utf-8"), SKILL_BODY)
            self.assertEqual((root / ".codex/skills/atlas-app-navigation/SKILL.md").read_text(encoding="utf-8"), SKILL_BODY)

    def test_init_does_not_overwrite_different_skill_without_yes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            skill = root / ".agents/skills/atlas-app-navigation/SKILL.md"
            skill.parent.mkdir(parents=True)
            skill.write_text("custom instructions\n", encoding="utf-8")

            stdout = io.StringIO()
            code = main(["init", "--json", "--agents", "codex"], cwd=root, stdout=stdout)

            self.assertEqual(code, 2)
            self.assertEqual(skill.read_text(encoding="utf-8"), "custom instructions\n")
            payload = json.loads(stdout.getvalue())
            self.assertFalse(payload["ok"])
            self.assertTrue(any(change["status"] == "conflict" for change in payload["changes"]))

            code = main(["init", "--json", "--yes", "--agents", "codex"], cwd=root, stdout=io.StringIO())
            self.assertEqual(code, 0)
            self.assertEqual(skill.read_text(encoding="utf-8"), SKILL_BODY)

    def test_doctor_json_after_init(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            main(["init", "--agents", "all"], cwd=root, stdout=io.StringIO())

            stdout = io.StringIO()
            code = main(["doctor", "--json"], cwd=root, stdout=stdout)

            self.assertEqual(code, 0)
            payload = json.loads(stdout.getvalue())
            self.assertTrue(payload["ok"])
            statuses = {check["name"]: check["status"] for check in payload["checks"]}
            self.assertEqual(statuses["graph_dirs"], "pass")
            self.assertEqual(statuses["gitignore"], "pass")
            self.assertEqual(statuses["skills"], "pass")
            self.assertIn(statuses["config"], {"pass", "warn"})
            self.assertIn(statuses["android_cli"], {"pass", "warn"})
            self.assertIn(statuses["adb"], {"pass", "warn"})


if __name__ == "__main__":
    unittest.main()
