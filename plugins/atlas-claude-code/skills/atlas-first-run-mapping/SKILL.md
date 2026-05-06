---
name: atlas-first-run-mapping
description: Use only when the user explicitly asks to perform first-run mapping, create an initial Atlas graph, map a new area of the app, explore a launched Android app for navigation memory, or record known routes from scratch. This is token-intensive and should be bounded, staged, and reviewed.
metadata:
  author: atlas
  version: "1.0"
---

# Atlas First-Run Mapping Skill

Atlas first-run mapping creates initial navigation memory for a launched Android app. This skill is intentionally separate from everyday Atlas navigation because it is expensive: the agent must inspect Android layout JSON, decide what to tap, navigate the app, and record routes before Atlas can reuse the graph.

Stage learned graph updates, but do not accept or commit them without explicit user approval.

## First-Run Mapping Mode

Use this mode when the user asks to map the app, create an initial Atlas graph, do first-run mapping, explore the launched app, record known routes, or build navigation memory from scratch.

Warn the user before starting: first-run mapping is token-intensive. Keep the run bounded by the user's requested scope. If no scope is given, map a small set of high-value flows first, then report what remains.

Use this skill one time for an initial app map, or later only with a specific bounded reason:
- A new feature area has no Atlas route yet.
- A major UI redesign invalidated existing routes.
- A separate app context needs mapping, such as logged-out, logged-in, onboarding, permission-gated, or feature-flagged states.
- The user explicitly asks for additional route coverage.

Prerequisites:
- The Android app is already built, installed, launched, and on the screen where mapping should begin.
- `atlas`, `android`, and `adb` are on PATH.
- Run `atlas init --agents all` if Atlas has not been initialized.
- Run `atlas doctor` and fix blocking environment issues before mapping.

Mapping workflow for each route:
1. Choose a short route name such as `settings`, `article-detail`, or `profile-edit`.
2. Run `atlas observe start <route-name>`.
3. Run `atlas layout` and inspect the current screen.
4. Choose stable selectors in this order: test tag, resource id, accessibility/content description, stable visible text. Avoid coordinate taps unless there is no usable selector.
5. Run `atlas tap --selector "<kind>=<value>" --reason "<why this moves toward the route>"`.
6. Run `atlas layout` after each meaningful transition.
7. Repeat only until the named route target is reached. Avoid unbounded crawling.
8. Run `atlas observe stop`.
9. Run `atlas learn --from-current-run --stage`.
10. Record the proposal id/path for the user. Do not run `atlas accept` unless the user explicitly approves accepting staged graph changes.

After mapping:
- Run `atlas validate --all` when at least one route has been accepted.
- Summarize mapped routes, staged proposals, selectors used, screens reached, and any flows skipped.
- Keep raw layout observations in `.atlas/runs/`; do not commit raw layouts or runtime state.

Failure handling:
- If a selector no longer works, run `atlas drift` or `atlas repair <target> --stage`.
- If the current screen is unknown, stage the proposal and report that review is required.
- If login, onboarding, permissions, or feature flags block navigation, report the required context instead of forcing through it.
