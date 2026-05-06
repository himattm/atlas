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
