from __future__ import annotations

import json
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

from atlas.android import (
    AdbExecutor,
    AndroidCliAdapter,
    AndroidCommandError,
    AndroidEnvironmentError,
    CommandResult,
    parse_input_tap,
)
from atlas.observation import ObservationRecorder, required_gitignore_patterns
from atlas.operations import SelectorNotFoundError, layout, tap_label, tap_selector


class FakeRunner:
    def __init__(self, outputs=None, fail_missing=False):
        self.outputs = list(outputs or [])
        self.fail_missing = fail_missing
        self.calls = []

    def run(self, args):
        self.calls.append(tuple(args))
        if self.fail_missing:
            raise FileNotFoundError(args[0])
        if self.outputs:
            value = self.outputs.pop(0)
            if isinstance(value, Exception):
                raise value
            return CommandResult(args=tuple(args), **value)
        return CommandResult(args=tuple(args), returncode=0, stdout="", stderr="")


class AndroidRuntimeTests(unittest.TestCase):
    def test_android_cli_adapter_uses_documented_command_surface(self):
        runner = FakeRunner(
            [
                {"returncode": 0, "stdout": "{}", "stderr": ""},
                {"returncode": 0, "stdout": "{}", "stderr": ""},
                {"returncode": 0, "stdout": "input tap 5 6", "stderr": ""},
                {"returncode": 0, "stdout": "{}", "stderr": ""},
                {"returncode": 0, "stdout": "{}", "stderr": ""},
                {"returncode": 0, "stdout": "{}", "stderr": ""},
            ]
        )
        android = AndroidCliAdapter(runner)

        android.layout()
        android.layout(diff=True)
        android.screen_capture(output="/tmp/screen.png", annotate=True)
        android.screen_resolve(screenshot="/tmp/screen.png", string="input tap #4")
        android.describe(project_dir="/repo")
        android.info()

        self.assertEqual(
            runner.calls,
            [
                ("android", "layout"),
                ("android", "layout", "--diff"),
                (
                    "android",
                    "screen",
                    "capture",
                    "--annotate",
                    "--output=/tmp/screen.png",
                ),
                (
                    "android",
                    "screen",
                    "resolve",
                    "--screenshot=/tmp/screen.png",
                    "--string=input tap #4",
                ),
                ("android", "describe", "--project_dir=/repo"),
                ("android", "info"),
            ],
        )

    def test_parse_input_tap_from_screen_resolve_output(self):
        self.assertEqual(parse_input_tap("input tap 500 1000").x, 500)
        self.assertEqual(parse_input_tap("resolved: adb shell input tap 10 20").y, 20)

        with self.assertRaises(Exception):
            parse_input_tap("tap 500 1000")

    def test_adb_tap_execution_uses_shell_input_tap(self):
        runner = FakeRunner([{"returncode": 0, "stdout": "", "stderr": ""}])
        adb = AdbExecutor(runner)

        adb.tap(420, 900)

        self.assertEqual(
            runner.calls, [("adb", "shell", "input", "tap", "420", "900")]
        )

    def test_layout_diff_is_android_in_session_diff_not_graph_drift(self):
        runner = FakeRunner(
            [
                {
                    "returncode": 0,
                    "stdout": json.dumps({"changed": [{"text": "Settings"}]}),
                    "stderr": "",
                }
            ]
        )
        result = layout(AndroidCliAdapter(runner), diff=True)

        self.assertEqual(runner.calls, [("android", "layout", "--diff")])
        self.assertEqual(result["kind"], "android_layout_diff")
        self.assertEqual(result["diff_scope"], "android_in_session")
        self.assertEqual(
            result["metrics"],
            {
                "layout_calls_total": 1,
                "layout_json_returned_to_agent": True,
                "adb_taps_total": 0,
            },
        )

    def test_tap_selector_resolves_layout_bounds_then_executes_adb(self):
        layout_json = {
            "children": [
                {
                    "text": "Settings",
                    "bounds": {
                        "left": 100,
                        "top": 200,
                        "right": 300,
                        "bottom": 400,
                    },
                }
            ]
        }
        android_runner = FakeRunner(
            [{"returncode": 0, "stdout": json.dumps(layout_json), "stderr": ""}]
        )
        adb_runner = FakeRunner([{"returncode": 0, "stdout": "", "stderr": ""}])

        result = tap_selector(
            AndroidCliAdapter(android_runner),
            AdbExecutor(adb_runner),
            selector="text=Settings",
            reason="open settings",
        )

        self.assertEqual(result["action"]["path"], "layout_selector")
        self.assertEqual(result["action"]["point"], {"x": 200, "y": 300})
        self.assertEqual(android_runner.calls, [("android", "layout")])
        self.assertEqual(
            adb_runner.calls, [("adb", "shell", "input", "tap", "200", "300")]
        )
        self.assertIs(result["metrics"]["layout_json_returned_to_agent"], False)

    def test_tap_label_uses_annotated_screenshot_screen_resolve_then_adb(self):
        android_runner = FakeRunner(
            [
                {"returncode": 0, "stdout": "", "stderr": ""},
                {"returncode": 0, "stdout": "input tap 50 75", "stderr": ""},
            ]
        )
        adb_runner = FakeRunner([{"returncode": 0, "stdout": "", "stderr": ""}])

        result = tap_label(
            AndroidCliAdapter(android_runner),
            AdbExecutor(adb_runner),
            label=3,
            screenshot="/tmp/annotated.png",
        )

        self.assertEqual(
            android_runner.calls,
            [
                (
                    "android",
                    "screen",
                    "capture",
                    "--annotate",
                    "--output=/tmp/annotated.png",
                ),
                (
                    "android",
                    "screen",
                    "resolve",
                    "--screenshot=/tmp/annotated.png",
                    "--string=input tap #3",
                ),
            ],
        )
        self.assertEqual(
            adb_runner.calls, [("adb", "shell", "input", "tap", "50", "75")]
        )
        self.assertEqual(result["action"]["path"], "annotated_screenshot_label")

    def test_observation_recording_stays_under_atlas_runs(self):
        now = datetime(2026, 5, 6, 12, 0, tzinfo=timezone.utc)

        def clock():
            nonlocal now
            value = now
            now = now + timedelta(seconds=1)
            return value

        with tempfile.TemporaryDirectory() as directory:
            tmp_path = Path(directory)
            recorder = ObservationRecorder(tmp_path, clock=clock)
            run = recorder.start("settings flow", metadata={"agent": "test"})
            recorder.record_observation(kind="android_layout", payload={"raw": {"x": 1}})
            recorder.record_action(
                kind="tap",
                payload={"selector": "text=Settings", "point": {"x": 1, "y": 2}},
                reason="open settings",
            )
            stopped = recorder.stop()

            self.assertIn(".atlas/runs/", required_gitignore_patterns())
            self.assertIn(".atlas/state/", required_gitignore_patterns())
            self.assertTrue(str(run.path).startswith(str(tmp_path / ".atlas" / "runs")))
            self.assertEqual(stopped["status"], "stopped")
            self.assertIsNone(recorder.current())

            actions = json.loads((run.path / "actions.json").read_text())
            observations = json.loads((run.path / "observations.json").read_text())
            self.assertEqual(actions[0]["reason"], "open settings")
            self.assertEqual(observations[0]["payload"], {"raw": {"x": 1}})
            self.assertFalse((tmp_path / ".atlas" / "graph").exists())

    def test_selector_not_found_is_structured_error(self):
        android_runner = FakeRunner(
            [{"returncode": 0, "stdout": json.dumps({"children": []}), "stderr": ""}]
        )

        with self.assertRaises(SelectorNotFoundError) as context:
            tap_selector(
                AndroidCliAdapter(android_runner),
                AdbExecutor(FakeRunner()),
                selector="text=Missing",
            )

        self.assertEqual(context.exception.to_dict()["error"]["code"], "selector_not_found")

    def test_environment_and_command_errors_are_structured(self):
        missing = AndroidCliAdapter(FakeRunner(fail_missing=True))
        with self.assertRaises(AndroidEnvironmentError) as missing_error:
            missing.info()
        self.assertEqual(
            missing_error.exception.to_dict()["error"]["code"], "environment_error"
        )

        failing = AndroidCliAdapter(
            FakeRunner([{"returncode": 2, "stdout": "", "stderr": "no device"}])
        )
        with self.assertRaises(AndroidCommandError) as command_error:
            failing.layout()
        payload = command_error.exception.to_dict()
        self.assertEqual(payload["error"]["code"], "command_failed")
        self.assertEqual(payload["error"]["details"]["stderr"], "no device")


if __name__ == "__main__":
    unittest.main()
