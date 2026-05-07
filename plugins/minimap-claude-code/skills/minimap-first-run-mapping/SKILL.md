---
name: minimap-first-run-mapping
description: Use only when the user explicitly asks to perform first-run mapping, create an initial Minimap graph, map a new area of the app, explore a launched Android app for navigation memory, or record known routes from scratch. This is token-intensive and should be bounded, staged, and reviewed.
metadata:
  author: minimap
  version: "1.0"
---

# Minimap First-Run Mapping Skill

Minimap first-run mapping creates initial navigation memory for a launched Android app. This skill is intentionally separate from everyday Minimap navigation because it is expensive: the agent must inspect Android layout JSON, decide what to tap, navigate the app, and record routes before Minimap can reuse the graph.

Stage learned graph updates, but do not accept or commit them without explicit user approval.

## First-Run Mapping Mode

Use this mode when the user asks to map the app, create an initial Minimap graph, do first-run mapping, explore the launched app, record known routes, or build navigation memory from scratch.

Warn the user before starting: first-run mapping is token-intensive. Keep the run bounded by the user's requested scope. If no scope is given, map a small set of high-value flows first, then report what remains.

Use this skill one time for an initial app map, or later only with a specific bounded reason:
- A new feature area has no Minimap route yet.
- A major UI redesign invalidated existing routes.
- A separate app context needs mapping, such as logged-out, logged-in, onboarding, permission-gated, or feature-flagged states.
- The user explicitly asks for additional route coverage.

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
