---
name: minimap-first-run-mapping
description: Use only when the user explicitly asks for a bounded bulk survey of the launched Android app — phrases like "map the whole app", "do first-run mapping", "bulk-map the app", "do an initial pass over <list of flows>", or "explore the app comprehensively." Do NOT fire on "use minimap", "this is a fresh repo", "navigate to X", "build the graph", or "record this route" — those are everyday incremental work and belong to minimap-app-navigation. For incremental mapping (one screen at a time as the user navigates), use minimap-app-navigation instead.
metadata:
  author: minimap
  version: "1.0"
---

# Minimap First-Run Mapping Skill

If you found this skill via a vague trigger like "use minimap on this app", "this repo has no .minimap yet", or "navigate to X", stop and use `minimap-app-navigation` instead — it handles incremental mapping and is the right tool for everyday Minimap work. This skill is only for bounded bulk surveys the user explicitly asked for.

Minimap first-run mapping does a deliberate bulk pass over a launched Android app to seed navigation memory across many flows at once. It is intentionally separate from everyday Minimap navigation because it is expensive: the agent must inspect Android layout JSON, decide what to tap, navigate the app, and record routes in a single sustained session.

Stage learned graph updates, but do not accept or commit them without explicit user approval.

## First-Run Mapping Mode

Use this mode only when the user has explicitly asked for a bulk survey: "map the whole app", "do first-run mapping", "bulk-map the app", "do an initial pass over <flows>", "explore the app comprehensively." Anything narrower — a single route, a fresh repo, "build the graph over time" — belongs to `minimap-app-navigation`.

Warn the user before starting: first-run mapping is token-intensive. Keep the run bounded by the user's requested scope. If no scope is given, map a small set of high-value flows first, then report what remains.

Bounded reasons to invoke this skill:
- The user explicitly requests a bulk initial app map.
- A new feature area needs broad coverage in one pass.
- A major UI redesign invalidated existing routes and a re-survey is requested.
- A separate app context (logged-out, logged-in, onboarding, permission-gated, feature-flagged) needs mapping.
- The user explicitly asks for additional route coverage across multiple flows.

Prerequisites:
- The Android app is already built, installed, launched, and on the screen where mapping should begin.
- `minimap`, `android`, and `adb` are on PATH. Claude Code plugins cannot install binaries, so if `minimap --version` fails, ask the user to install it first:
  - Homebrew: `brew install himattm/minimap/minimap`
  - Cargo: `cargo install minimap-cli`
  - From source: `cargo install --git https://github.com/himattm/minimap minimap-cli`
- Run `minimap init --agents all` if Minimap has not been initialized.
- Run `minimap doctor` and fix blocking environment issues before mapping.

Mapping workflow for each route:
1. Choose a short route name such as `settings`, `article-detail`, or `profile-edit`.
2. Run `minimap map --discover <route-name> --max-actions 5 --stage`.
3. Run `minimap layout` and inspect the current screen.
4. Choose stable selectors in this order: test tag, resource id, accessibility/content description, stable visible text. Avoid coordinate taps unless there is no usable selector.
5. Run `minimap tap --selector "<kind>=<value>" --reason "<why this moves toward the route>"`.
6. Run `minimap layout` after each meaningful transition.
7. Repeat only until the named route target is reached. Avoid unbounded crawling.
8. Run `minimap map --discover <route-name> --max-actions 5 --stage --finish`.
9. Record the proposal id/path for the user. Do not run `minimap accept` unless the user explicitly approves accepting staged graph changes.

After mapping:
- Run `minimap validate --all` when at least one route has been accepted.
- Summarize mapped routes, staged proposals, selectors used, screens reached, and any flows skipped.
- Keep raw layout observations in `.minimap/runs/`; do not commit raw layouts or runtime state.

Failure handling:
- If a selector no longer works, run `minimap drift` or `minimap repair <target> --stage`.
- If the current screen is unknown, stage the proposal and report that review is required.
- If login, onboarding, permissions, or feature flags block navigation, report the required context instead of forcing through it.
