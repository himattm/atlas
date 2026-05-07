---
name: minimap-app-navigation
description: Use when working in an Android codebase and needing to navigate the launched app, inspect Android layout JSON, use android layout, use android layout --diff, tap UI elements, validate screens, learn routes, reuse known navigation, or update the repo's Minimap graph. Before calling android layout or raw adb tap commands directly, check Minimap first.
metadata:
  author: minimap
  version: "1.0"
---

# Minimap App Navigation Skill

Minimap is this repo's shared navigation memory and soft validation layer for AI agents working in this Android codebase.

Use Minimap before raw Android layout or adb tap commands. Stage learned graph updates, but do not accept or commit them without explicit user approval.

## Prerequisites

The `minimap` CLI must be on `PATH`. Claude Code plugins cannot install binaries, so if `minimap --version` fails, ask the user to install it before continuing:

- Homebrew: `brew install himattm/minimap/minimap`
- Cargo: `cargo install minimap-cli`
- From source: `cargo install --git https://github.com/himattm/minimap minimap-cli`

`android` and `adb` must also be on `PATH` for any layout or tap commands.
