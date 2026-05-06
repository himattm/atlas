# Atlas Implementation Plan

Atlas is shared navigation memory and soft validation for AI agents working in
Android codebases. The v1 implementation is now a Rust CLI workspace that wraps
documented Android CLI commands and `adb`, stores distilled navigation knowledge
under `.atlas/`, and returns compact JSON results for agents.

## Current Repo Assessment

- The repo is a Rust workspace with a single distributable `atlas` binary.
- The original Python reference implementation has been removed after porting
  the covered command surface to Rust and replacing Python tests with Cargo
  unit/integration tests.
- `.atlas/runs/` and `.atlas/state/` are gitignored by `atlas init`. Distilled
  graph files are intended to be committed after explicit proposal acceptance.
- CI builds and validates the Rust workspace on Linux and macOS. Tagged
  releases build GitHub release binaries for Linux and macOS targets.

## Architecture

- Command layer: `crates/atlas-cli` parses agent-facing commands and emits
  compact JSON.
- Schema layer: `crates/atlas-schemas` owns committed artifact and result
  contracts.
- Repo layer: `crates/atlas-repo` manages `.atlas/`, deterministic JSON,
  proposals, skills, gitignore entries, and observation runs.
- Android layer: `crates/atlas-android` wraps documented Android CLI commands
  and `adb shell input tap`.
- Core layer: `crates/atlas-core` handles redaction-before-hashing,
  normalization, identity hashes, and screen similarity matching.
- Graph layer: `crates/atlas-graph` resolves named routes and graph fallback
  with context guards.

## Repo Layout

```text
crates/
  atlas-cli/
  atlas-schemas/
  atlas-repo/
  atlas-android/
  atlas-core/
  atlas-graph/
  atlas-test-support/
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

All agent-facing commands emit JSON. The global `--json`, `--quiet`, and
`--no-color` flags are accepted for compatibility with agent command templates.

- `atlas init [--dry-run] [--agents auto|all|codex,claude,android-studio,gemini]`
  creates Atlas directories, config, gitignore entries, and repo-local skills:
  `atlas-app-navigation` for everyday route reuse and
  `atlas-first-run-mapping` for bounded, token-intensive initial discovery.
- `atlas doctor --json` checks config, graph dirs, skills, `android`, `adb`,
  and gitignore coverage.
- `atlas layout [--diff] --json` wraps `android layout` or the in-session Android
  layout delta from `android layout --diff`.
- `atlas tap --selector ... --reason ... --json`, `atlas tap --point x,y ...`,
  and `atlas tap --label N --screenshot path ...` resolve to `adb shell input
  tap X Y`, then observe compact state.
- `atlas observe start NAME --json` and `atlas observe stop --json` manage raw
  run traces under `.atlas/runs/`.
- `atlas learn --from-current-run --stage --json` stages graph proposals,
  preserving each observed tap transition as an edge when the run has enough
  layout/action data.
- `atlas accept PROPOSAL_ID --json` applies a proposal only when explicitly run.
- `atlas route TARGET --json` returns a route plan by name, screen, alias, or
  intent, using graph fallback when allowed.
- `atlas go TARGET --mode verified --json` executes a known route. `verified` is
  default; `safe` resolves selectors every time; `fast` may use guarded tap
  caches but validates after transition.
- `atlas check [--current|SCREEN] --json` evaluates lightweight expectations.
- `atlas drift --json`, `atlas validate --json`, and `atlas repair ... --stage`
  classify divergence and stage review proposals without mutating committed
  graph objects. `atlas validate` is dry by default; `--execute` opts into route
  execution.
- `atlas map --discover NAME --max-actions N --stage [--finish]` coordinates a
  bounded assistant-led discovery run. Atlas manages observation and proposal
  staging while the agent chooses selectors from layout output.

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

- Rust unit tests cover schema/context behavior, canonical JSON, init
  idempotence, graph fallback, and Android command composition.
- Rust CLI integration tests use temp repos and fake `android`/`adb` executables
  for `init`, `route`, `observe`, `learn`, `layout --diff`, and `tap --selector`.
- CLI integration tests now cover drift, validation proposal staging,
  validation route execution, learned multi-step screen/edge/route proposals
  after stopped runs, bounded map discovery, and route postcondition
  verification after `go` executes edge taps.
- Future parity tests should add live-device smoke coverage and richer
  redaction/normalization fixtures.

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
10. Drift/validate/repair.
11. Budgeted mapping.

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
