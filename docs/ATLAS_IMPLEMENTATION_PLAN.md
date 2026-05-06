# Atlas Implementation Plan

Atlas is shared navigation memory and soft validation for AI agents working in
Android codebases. The v1 implementation is a Python stdlib CLI that wraps
documented Android CLI commands and `adb`, stores distilled navigation knowledge
under `.atlas/`, and returns compact JSON results for agents.

## Current Repo Assessment

- The repo is greenfield: only `ATLAS_CODEX_PLANNING_BRIEF.md` and agent hook
  config files are present.
- There is no existing package, build system, test runner, Android integration,
  graph schema, or CLI.
- The implementation can choose a conservative stdlib-first Python layout to
  avoid dependency download and make tests runnable immediately.
- `.atlas/runs/` and `.atlas/state/` must be gitignored. Distilled graph files
  are intended to be committed.

## Architecture

- Command layer: `atlas.cli` parses agent-facing commands and emits compact JSON.
- Config layer: `atlas.config` loads `.atlas/config.json`, applies defaults, and
  validates schema basics.
- Init and skills: `atlas.init_cmd` creates `.atlas/`, idempotent `.gitignore`
  entries, repo-local `atlas-app-navigation` skills, and optional guidance.
- Doctor: `atlas.doctor` checks environment, graph layout, gitignore, and skills.
- Android adapters: `atlas.android` wraps documented Android CLI commands and
  `adb shell input tap`.
- Observation recorder: `atlas.observation` writes run traces under
  `.atlas/runs/` only.
- Redaction and normalization: `atlas.redaction` and `atlas.normalization` run
  before hashing, persistence, or proposal generation.
- Graph storage: `atlas.storage` reads per-object JSON files and computes runtime
  indexes without committing a central index.
- Matching: `atlas.matching` uses identity hash only as a fast path and falls
  back to weighted similarity over stable element signatures.
- Routing: `atlas.routing` resolves named routes and graph fallback with context
  guards and fragility penalties.
- Operations: `atlas.operations` composes layout, tap, check, route, and go
  behavior into reusable command helpers.

## Repo Layout

```text
atlas/
  __init__.py
  __main__.py
  cli.py
  config.py
  init_cmd.py
  doctor.py
  android.py
  observation.py
  operations.py
  models.py
  storage.py
  redaction.py
  normalization.py
  matching.py
  routing.py
tests/
  test_init_doctor.py
  test_graph_core.py
  test_android_runtime.py
docs/
  ATLAS_IMPLEMENTATION_PLAN.md
```

Runtime repo state after `atlas init`:

```text
.atlas/
  config.json
  graph/screens/
  graph/edges/
  routes/
  checks/
  proposals/
  runs/       # gitignored
  state/      # gitignored
```

## CLI Commands

All agent-facing commands support `--json`; global `--quiet` and `--no-color`
are accepted.

- `atlas init [--dry-run] [--yes] [--agents auto|all|codex,claude,android-studio,gemini]`
  creates Atlas directories, config, gitignore entries, and repo-local skills.
- `atlas doctor --json` checks config, graph dirs, skills, `android`, `adb`,
  and gitignore coverage.
- `atlas layout [--diff] --json` wraps `android layout` or the in-session Android
  layout delta from `android layout --diff`.
- `atlas tap --selector ... --reason ... --json`, `atlas tap --point x,y ...`,
  and `atlas tap --label N --screenshot path ...` resolve to `adb shell input
  tap X Y`, then observe compact state.
- `atlas observe start NAME --json` and `atlas observe stop --json` manage raw
  run traces under `.atlas/runs/`.
- `atlas learn --from-current-run --stage --json` stages graph proposals.
- `atlas accept PROPOSAL_ID --json` applies a proposal only when explicitly run.
- `atlas route TARGET --json` returns a route plan by name, screen, alias, or
  intent, using graph fallback when allowed.
- `atlas go TARGET --mode verified --json` executes a known route. `verified` is
  default; `safe` resolves selectors every time; `fast` may use guarded tap
  caches but validates after transition.
- `atlas check [--current|SCREEN] --json` evaluates lightweight expectations.
- `atlas drift --json`, `atlas validate --json`, `atlas repair ... --stage`, and
  `atlas map --discover --max-actions N --stage` are staged after the core loop.

Exit codes follow the brief: `0` success, `1` meaningful change, `2` expectation
failure, `3` route failed, `4` selector drift, `5` unknown screen, `6`
environment error, `7` schema/config error, and `8` context mismatch.

## Data Rules

- Every committed object includes `schema_version`.
- Committed files use deterministic key ordering and stable filenames.
- No committed central graph index in v1; indexes are computed at runtime.
- Runtime telemetry, timestamps, success counters, raw layouts, screenshots, and
  layout deltas stay under `.atlas/state/` or `.atlas/runs/`.
- Redaction runs before hashing, normalization persistence, or proposal staging.
- Verbatim text is excluded from committed graph artifacts by default.
- Screen identity is similarity-based; exact hash is only a fast path.
- Context guards apply to screens, edges, routes, and checks. A guard mismatch
  returns `context_mismatch`, not `route_broken`.

## Testing Strategy

- Init/doctor tests use temp directories and verify idempotence, dry-run behavior,
  gitignore entries, and skill installation.
- Graph tests cover canonical JSON, context guard matching, redaction before
  hashing, normalization, matching thresholds, route fallback, proposal staging,
  and absence of a committed central index.
- Android/runtime tests use fake command runners for documented command
  construction, `android screen resolve` parsing, `adb shell input tap`, layout
  diff semantics, observation recording, selector-not-found, and environment
  errors.
- CLI smoke tests should exercise JSON output and nonzero status mapping for
  context mismatch and environment failures.

## Acceptance Demos

First agent session:

```bash
atlas route article-detail --json
atlas observe start article-detail --json
atlas layout --json
atlas tap --selector "..." --reason "open article detail" --json
atlas check --current --json
atlas observe stop --json
atlas learn --from-current-run --stage --json
atlas accept <proposal-id> --json
```

Expected: `.atlas/graph` has at least one screen and one edge, `.atlas/routes`
has an article route, no raw layout JSON or volatile telemetry is committed.

Second agent session:

```bash
atlas route article-detail --json
atlas go article-detail --mode verified --json
atlas check article-detail --json
```

Expected: Atlas returns compact route/check results, reports estimated layout
calls saved, and avoids rediscovering the screen from full layout JSON.

Divergence demo:

```bash
atlas validate --json
```

Expected: Atlas classifies divergence and stages proposals without silently
updating the baseline.

Context mismatch demo:

```bash
atlas go logged-in-only-route --json
```

Expected: Atlas returns `context_mismatch` and recommends establishing the
required context or choosing a compatible route.

## Milestones

1. CLI skeleton, config, init, skill installation, and doctor.
2. Android CLI and adb adapters with mocked tests.
3. Observation capture under `.atlas/runs/`.
4. Redaction, normalization, and identity hashing.
5. Screen matching and context guard behavior.
6. Graph storage without committed index.
7. Proposal staging and explicit accept.
8. Route lookup and graph fallback.
9. Verified-mode navigation and checks.
10. Drift/validate/repair and budgeted mapping.

## Risks

- Agents bypass Atlas if skill activation is weak. Mitigation: trigger-heavy
  `SKILL.md`, AGENTS guidance, and `doctor` coverage checks.
- Compose apps may lack stable selectors. Mitigation: scored selector candidates
  and doctor recommendations, not hard failure.
- Cached coordinates are fragile. Mitigation: default verified mode, context
  guards, and postcondition checks.
- Sensitive data can leak through layout JSON. Mitigation: redaction before all
  hashing and persistence, raw artifacts gitignored by default.
- Parallel agents can conflict on graph files. Mitigation: one object per file,
  canonical JSON, deterministic filenames, no committed central index.
- Baselines can be updated too casually. Mitigation: learn/repair only stage
  proposals; accept requires explicit command.
