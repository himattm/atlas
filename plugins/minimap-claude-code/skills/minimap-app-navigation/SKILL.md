---
name: minimap-app-navigation
description: Use in an Android codebase for any Minimap work — navigating the launched app, inspecting Android layout JSON, running android layout or android layout --diff, tapping UI elements, validating screens, learning routes, reusing known navigation, or growing the repo's Minimap graph one screen at a time even when no graph exists yet. Before calling android layout or raw adb tap commands directly, check Minimap first.
metadata:
  author: minimap
  version: "1.0"
---

# Minimap App Navigation Skill

Minimap is this repo's shared navigation memory and soft validation layer for AI agents working in this Android codebase.

Use Minimap before raw Android layout or adb tap commands. Stage learned graph updates, but do not accept or commit them without explicit user approval.

## Incremental mapping

Minimap graphs grow one screen at a time. An empty `.minimap/` after `minimap init` is normal — the graph fills in as the user navigates the app. Do not treat "no graph yet" as a reason to fall back to raw `android` or `adb` commands.

When the user asks you to navigate to a route Minimap does not yet know, treat that navigation itself as a chance to record the route. Run the lightweight loop below, stage a proposal, and surface the proposal id. Do not auto-`accept` — wait for user approval.

Lightweight loop for adding one screen:

```bash
minimap observe start <short-route-name>
minimap layout
minimap tap --selector "<kind>=<value>" --reason "<why>"
minimap layout
minimap observe stop
minimap learn --from-current-run --stage
```

Then report the proposal id and stop.

Selector preference (most stable first): test tag, resource id, accessibility/content description, stable visible text. Avoid coordinate taps unless nothing else is usable.

When the graph already has the route, reuse it: `minimap route`, `minimap go`, `minimap check`. Run `minimap drift` or `minimap validate --all` when verifying existing screens.

Always stage. Never `minimap accept` without explicit user approval.

## Prerequisites

The `minimap` CLI must be on `PATH`. Claude Code plugins cannot install binaries, so if `minimap --version` fails, ask the user to install it before continuing:

- Homebrew: `brew install himattm/minimap/minimap`
- Cargo: `cargo install minimap-cli`
- From source: `cargo install --git https://github.com/himattm/minimap minimap-cli`

`android` and `adb` must also be on `PATH` for any layout or tap commands.
