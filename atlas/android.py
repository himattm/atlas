"""Adapters for the documented Android CLI surface and adb input execution."""

from __future__ import annotations

import json
import re
import subprocess
from dataclasses import dataclass
from typing import Any, Iterable, Mapping, Optional, Protocol, Sequence


@dataclass(frozen=True)
class CommandResult:
    """Result returned by command runners."""

    args: tuple[str, ...]
    returncode: int
    stdout: str = ""
    stderr: str = ""

    def json(self) -> Any:
        try:
            return json.loads(self.stdout)
        except json.JSONDecodeError as exc:
            raise AndroidParseError(
                "Command did not return valid JSON",
                command=self.args,
                details={"stdout": self.stdout},
            ) from exc


class CommandRunner(Protocol):
    """Minimal runner interface used by tests and subprocess execution."""

    def run(self, args: Sequence[str]) -> CommandResult:
        """Run a command and return its captured result."""


class SubprocessCommandRunner:
    """Command runner backed by :mod:`subprocess`."""

    def run(self, args: Sequence[str]) -> CommandResult:
        try:
            completed = subprocess.run(
                list(args),
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
        except FileNotFoundError as exc:
            raise AndroidEnvironmentError(
                f"Executable not found: {args[0]}",
                command=args,
                details={"executable": args[0]},
            ) from exc

        return CommandResult(
            args=tuple(args),
            returncode=completed.returncode,
            stdout=completed.stdout,
            stderr=completed.stderr,
        )


class AndroidError(RuntimeError):
    """Base structured error for Android runtime integration."""

    code = "android_error"

    def __init__(
        self,
        message: str,
        *,
        command: Optional[Sequence[str]] = None,
        details: Optional[Mapping[str, Any]] = None,
    ) -> None:
        super().__init__(message)
        self.message = message
        self.command = tuple(command or ())
        self.details = dict(details or {})

    def to_dict(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "status": "error",
            "error": {"code": self.code, "message": self.message},
        }
        if self.command:
            payload["error"]["command"] = list(self.command)
        if self.details:
            payload["error"]["details"] = self.details
        return payload


class AndroidEnvironmentError(AndroidError):
    """Raised when Android CLI or adb cannot be executed."""

    code = "environment_error"


class AndroidCommandError(AndroidError):
    """Raised when a documented Android CLI or adb command exits non-zero."""

    code = "command_failed"


class AndroidParseError(AndroidError):
    """Raised when a command result cannot be parsed as expected."""

    code = "parse_error"


class ScreenResolveError(AndroidParseError):
    """Raised when ``android screen resolve`` does not return tap input."""

    code = "screen_resolve_error"


@dataclass(frozen=True)
class TapPoint:
    """Absolute screen coordinate for adb input tap."""

    x: int
    y: int


_INPUT_TAP_RE = re.compile(
    r"(?:^|\b)(?:adb\s+shell\s+)?input\s+tap\s+(-?\d+)\s+(-?\d+)(?:\b|$)"
)


def parse_input_tap(value: str) -> TapPoint:
    """Parse ``input tap X Y`` from Android screen resolve output."""

    match = _INPUT_TAP_RE.search(value.strip())
    if not match:
        raise ScreenResolveError(
            "Expected android screen resolve to return 'input tap X Y'",
            details={"output": value},
        )
    return TapPoint(x=int(match.group(1)), y=int(match.group(2)))


class AndroidCliAdapter:
    """Wrapper over documented Android CLI commands used by Atlas."""

    def __init__(
        self,
        runner: Optional[CommandRunner] = None,
        *,
        android_bin: str = "android",
    ) -> None:
        self.runner = runner or SubprocessCommandRunner()
        self.android_bin = android_bin

    def layout(
        self,
        *,
        pretty: bool = False,
        output: Optional[str] = None,
        diff: bool = False,
    ) -> CommandResult:
        args = [self.android_bin, "layout"]
        if pretty:
            args.append("--pretty")
        if output is not None:
            args.append(f"--output={output}")
        if diff:
            args.append("--diff")
        return self._run(args)

    def screen_capture(
        self,
        *,
        output: str,
        annotate: bool = False,
    ) -> CommandResult:
        args = [self.android_bin, "screen", "capture"]
        if annotate:
            args.append("--annotate")
        args.append(f"--output={output}")
        return self._run(args)

    def screen_resolve(self, *, screenshot: str, string: str) -> CommandResult:
        args = [
            self.android_bin,
            "screen",
            "resolve",
            f"--screenshot={screenshot}",
            f"--string={string}",
        ]
        return self._run(args)

    def resolve_tap(self, *, screenshot: str, string: str) -> TapPoint:
        result = self.screen_resolve(screenshot=screenshot, string=string)
        return parse_input_tap(result.stdout)

    def describe(self, *, project_dir: Optional[str] = None) -> CommandResult:
        args = [self.android_bin, "describe"]
        if project_dir is not None:
            args.append(f"--project_dir={project_dir}")
        return self._run(args)

    def info(self) -> CommandResult:
        return self._run([self.android_bin, "info"])

    def _run(self, args: Iterable[str]) -> CommandResult:
        command = tuple(args)
        try:
            result = self.runner.run(command)
        except FileNotFoundError as exc:
            raise AndroidEnvironmentError(
                f"Executable not found: {command[0]}",
                command=command,
                details={"executable": command[0]},
            ) from exc
        if result.returncode != 0:
            raise AndroidCommandError(
                "Android CLI command failed",
                command=command,
                details={
                    "returncode": result.returncode,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                },
            )
        return result


class AdbExecutor:
    """Executor for the documented adb shell input tap fallback."""

    def __init__(
        self,
        runner: Optional[CommandRunner] = None,
        *,
        adb_bin: str = "adb",
    ) -> None:
        self.runner = runner or SubprocessCommandRunner()
        self.adb_bin = adb_bin

    def tap(self, x: int, y: int) -> CommandResult:
        args = [self.adb_bin, "shell", "input", "tap", str(x), str(y)]
        try:
            result = self.runner.run(args)
        except FileNotFoundError as exc:
            raise AndroidEnvironmentError(
                f"Executable not found: {self.adb_bin}",
                command=args,
                details={"executable": self.adb_bin},
            ) from exc
        if result.returncode != 0:
            raise AndroidCommandError(
                "adb tap command failed",
                command=args,
                details={
                    "returncode": result.returncode,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                },
            )
        return result
