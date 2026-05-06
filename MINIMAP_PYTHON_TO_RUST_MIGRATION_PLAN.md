# Minimap Python → Rust Migration Plan

## Status Note

The Python reference implementation has been removed after porting the covered
CLI surface to Rust and replacing Python tests with Cargo unit/integration
tests. This document remains as historical migration rationale and as a checklist
for still-pending deeper product work such as richer normalization, drift,
validation, repair, mapping, and live-device parity tests.

## Repo description

**Give AI agents a map of your Android app.**

Minimap is shared navigation memory and soft validation for AI agents working in Android codebases. It captures how agents navigate a running Android app, stores that knowledge as a repo-committed graph, and lets future agents reuse known routes instead of rediscovering the UI from scratch.

---

## Purpose of this document

This document assumes Minimap already exists as a Python CLI and describes how to migrate it to Rust without breaking the product contract.

The migration goal is not to redesign Minimap. The goal is to preserve the Python implementation’s external behavior while improving distribution, reliability, type safety, performance, and long-term maintainability.

The most important rule:

> **The Rust rewrite must be contract-preserving. The CLI, JSON outputs, exit codes, repo artifacts, skills, and graph semantics are the product. The implementation language is an internal detail.**

---

## Suggested Codex prompt

Use this prompt when asking Codex to turn this plan into an implementation roadmap:

```text
We have a Python implementation of Minimap, a CLI that gives AI agents shared navigation memory for Android codebases. Minimap wraps Android CLI and adb primitives, records agent navigation runs, stores a repo-committed graph under .minimap/, and provides soft validation when the running app diverges from the committed graph.

We want to migrate Minimap from Python to Rust without changing external behavior.

Read MINIMAP_PYTHON_TO_RUST_MIGRATION_PLAN.md and produce a concrete implementation plan.

Constraints:
- Preserve existing CLI commands, flags, JSON output schemas, exit codes, and .minimap/ repo artifact schemas.
- Treat the Python implementation as the reference oracle until Rust reaches parity.
- Do not redesign the product during migration.
- Do not introduce new committed fields that cause git churn, such as last_validated_at or success counters.
- Preserve privacy rules: redact before fingerprinting; do not commit raw Android layout JSON; exclude verbatim text from committed artifacts by default.
- Use Rust to produce a single distributable CLI binary.
- Add fixture-based parity tests comparing Python and Rust behavior before replacing the Python CLI.
- Prefer command-by-command migration over a hard rewrite.

Deliverables:
1. Proposed Rust workspace layout.
2. Command migration order.
3. Compatibility test harness design.
4. Data model migration strategy.
5. Risk register.
6. Cutover criteria for replacing the Python CLI with Rust.
```

---

## Executive recommendation

Migrate Minimap to Rust using a **contract-first strangler approach**:

1. Freeze the current Python CLI contract.
2. Build a fixture and golden-output suite from the Python implementation.
3. Implement the Rust CLI command by command.
4. Compare Rust behavior against Python for the same inputs.
5. Ship Rust in preview behind an opt-in binary name or feature flag.
6. Cut over only after the Rust implementation passes parity tests and the core demo works end to end.

Avoid a hard rewrite.

Avoid using Rust as a thin shell that calls Python forever.

The Python implementation should become a temporary reference implementation, not a permanent dependency.

---

## Why migrate to Rust

Rust is a good fit for Minimap because Minimap is a local CLI that needs to be reliable, portable, and predictable across many developer and agent environments.

Primary benefits:

- **Single-binary distribution.** Easier installation for developers, Codex, Claude Code, Gemini, Android Studio Agent Mode, and CI-like environments.
- **Stable CLI behavior.** Minimap is part of an agent workflow, so commands, exit codes, and JSON outputs need to be boring and predictable.
- **Typed graph schemas.** Screens, edges, routes, checks, contexts, proposals, and result objects benefit from strong data modeling.
- **Safer error handling.** Android CLI and adb failures need explicit classification instead of generic stack traces.
- **Better repo artifact discipline.** Rust can enforce deterministic serialization, schema versions, migration checks, and no volatile committed fields.
- **Fast local matching.** Layout normalization, stable-element scoring, selector resolution, and route traversal are natural Rust workloads.
- **Lower runtime dependency risk.** Minimap should not fail because a local Python environment is misconfigured.

Rust is not chosen for novelty. It is chosen because Minimap is intended to become developer infrastructure.

---

## Why not rewrite immediately from scratch

The hard parts of Minimap are not just CLI plumbing. They are product semantics:

- What counts as the same screen?
- How are stable elements selected?
- When is a route broken versus changed?
- When should Minimap propose a graph update?
- How should selector drift be classified?
- What JSON should an agent receive next?
- What must never be committed for privacy or git-churn reasons?

A full rewrite risks accidentally changing those behaviors.

The migration should first preserve behavior, then improve internals.

---

## Migration principles

### 1. Preserve the external contract

The following are compatibility surfaces:

```text
CLI command names
CLI flags
JSON result schemas
Markdown summaries
exit codes
.minimap/ artifact layout
schema_version fields
skill installation behavior
redaction behavior
graph update proposal semantics
```

Any intentional change to one of these surfaces must be documented as a product migration, not hidden inside the Rust rewrite.

---

### 2. Keep Python as the oracle temporarily

During migration, the Python implementation should remain the reference for:

```text
input fixture → normalized layout
input run trace → proposed graph update
input graph + target → route result
input route + observed layout → drift classification
```

Rust should be compared against Python using golden fixtures.

---

### 3. No new git churn

The Rust implementation must not introduce fields that change on every run.

Do not commit:

```text
last_validated_at
success_count
failure_count
route reuse count
local timing metrics
raw layout snapshots
Android CLI session diffs
screenshots
```

Store those under:

```text
.minimap/state/     # gitignored
.minimap/runs/      # gitignored
```

---

### 4. Redaction before everything

The Rust pipeline must enforce:

```text
raw Android layout JSON
  → redact
  → normalize
  → compute stable element signatures
  → compute similarity/fingerprint
  → write committed artifacts
```

Never fingerprint sensitive text before redaction.

Never commit verbatim text by default.

---

### 5. Deterministic artifact output

Committed `.minimap/` files should be deterministic:

```text
stable field ordering where possible
stable list sorting where possible
pretty-printed JSON
no timestamps on successful no-op validation
no machine-specific absolute paths
no device-specific values unless guarded by context
```

This matters because Minimap is multi-agent shared memory. Bad artifact discipline creates noisy PRs and merge conflicts.

---

## Current Python implementation assumptions

Before writing Rust, inventory the Python implementation.

Create a command matrix:

| Command | Exists in Python | Inputs | Outputs | Exit codes | Writes files | Calls Android CLI | Calls adb |
|---|---:|---|---|---|---|---|---|
| `minimap init` | yes/no | repo path, skill targets | files changed | | yes | maybe | no |
| `minimap doctor` | yes/no | config | diagnostic JSON/Markdown | | no | yes | yes |
| `minimap layout` | yes/no | flags | normalized layout/result | | run artifacts | `android layout` | no |
| `minimap layout --diff` | yes/no | flags | in-session layout delta | | run artifacts | `android layout --diff` | no |
| `minimap tap` | yes/no | selector/point/reason | action result | | observation | maybe | yes |
| `minimap observe start` | yes/no | run name | run id | | yes | no | no |
| `minimap observe stop` | yes/no | run id | summary | | yes | no | no |
| `minimap learn --stage` | yes/no | run id | proposal | | yes | no | no |
| `minimap accept` | yes/no | proposal id | graph changes | | yes | no | no |
| `minimap route` | yes/no | target/context | route result | | no | no | no |
| `minimap go` | yes/no | target/mode | navigation result | | run artifacts | yes | yes |
| `minimap check` | yes/no | target/current | check result | | no/state | yes | no |
| `minimap validate` | yes/no | changed files/since/all | validation result | | proposals/state | yes | yes |
| `minimap drift` | yes/no | current/route | graph drift result | | proposals | yes | no |
| `minimap repair` | yes/no | edge/screen | staged repair | | yes | maybe | maybe |
| `minimap map --discover` | yes/no | budget | proposals | | yes | yes | yes |

This table should become part of the migration issue tracker.

---

## Target Rust workspace

Recommended workspace:

```text
minimap/
├── Cargo.toml
├── crates/
│   ├── minimap-cli/
│   ├── minimap-core/
│   ├── minimap-android/
│   ├── minimap-repo/
│   ├── minimap-graph/
│   ├── minimap-schemas/
│   └── minimap-test-support/
├── fixtures/
│   ├── android-layouts/
│   ├── observation-runs/
│   ├── graphs/
│   ├── proposals/
│   └── golden-results/
└── tests/
    ├── cli_parity.rs
    ├── artifact_roundtrip.rs
    └── fake_android_cli.rs
```

### `minimap-cli`

Responsibilities:

```text
command parsing
flag validation
output format selection
exit code mapping
human Markdown rendering
machine JSON rendering
```

This crate should stay thin.

It should call domain services, not implement graph logic directly.

---

### `minimap-schemas`

Responsibilities:

```text
Rust structs for committed artifacts
Rust structs for run artifacts
Rust structs for JSON result outputs
schema_version constants
forward/backward compatibility helpers
deterministic serialization helpers
```

Primary types:

```text
MinimapConfig
GraphContext
ScreenNode
StableElement
NavigationEdge
TapRecipe
SelectorCandidate
Route
Check
ObservationRun
ObservedAction
GraphUpdateProposal
MinimapResult
MinimapErrorResult
```

This crate is the contract layer.

---

### `minimap-android`

Responsibilities:

```text
run android CLI commands
run adb commands
parse Android CLI layout JSON
capture Android CLI in-session layout diffs
compose minimap tap behavior
classify environment errors
```

Important: there is no real `android tap` command.

`minimap tap` must be implemented as Minimap behavior that may use:

```text
latest Android layout bounds
android screen capture --annotate
android screen resolve --string="input tap #N"
adb shell input tap X Y
adb shell input text ...
adb shell input keyevent ...
```

Do not describe `minimap tap` as a direct wrapper over `android tap`.

---

### `minimap-core`

Responsibilities:

```text
redaction
normalization
stable element extraction
screen matching
selector scoring
transition classification
soft-validation classification
proposal generation
```

This is the main product-logic crate.

---

### `minimap-graph`

Responsibilities:

```text
load committed graph from .minimap/
compute graph index at runtime
resolve named routes
fallback pathfinding over graph
route scoring
context filtering
check evaluation
```

Do not rely on a committed central `index.json`.

If an index is useful, cache it under:

```text
.minimap/state/index-cache.json
```

That file must be gitignored.

---

### `minimap-repo`

Responsibilities:

```text
find repo root
read/write .minimap/config.json
read/write screen files
read/write edge files
read/write route files
read/write check files
read/write proposal files
manage .minimap/runs/
manage .minimap/state/
install skills
patch .gitignore safely
idempotent init
```

This crate should protect committed artifacts from accidental volatile writes.

---

### `minimap-test-support`

Responsibilities:

```text
fake android CLI
fake adb
fixture repo creation
snapshot normalization
golden JSON comparison
Python/Rust parity test helpers
```

This crate makes migration safe.

---

## Compatibility contract

### CLI compatibility

Rust should support the Python CLI exactly unless a breaking change is explicitly approved.

Preserve:

```text
command names
aliases
flag names
flag defaults
stdout/stderr conventions
JSON vs Markdown output modes
exit codes
```

If the Python CLI has accidental or undesirable behavior, document it as either:

```text
preserve for compatibility
fix with migration note
remove before Rust parity baseline is frozen
```

Do not silently change it during implementation.

---

### JSON output compatibility

Every agent-facing command should have golden JSON fixtures.

Examples:

```text
minimap route article-detail --json
minimap go article-detail --json
minimap check article-detail --json
minimap learn --stage --json
minimap validate --json
minimap drift --json
minimap repair --stage --json
```

The Rust output should match Python output modulo explicitly ignored volatile fields.

Volatile fields should ideally not exist in committed files or agent-facing result JSON unless necessary.

---

### Exit code compatibility

Suggested stable mapping:

```text
0  success, no relevant changes
1  success, meaningful UI/navigation changes detected
2  check/invariant failure
3  route failed
4  selector drift
5  unknown screen
6  Android CLI/device/environment error
7  Minimap config/schema error
8  privacy/redaction violation
9  unsupported schema version
```

Rust should implement exit codes centrally, not ad hoc per command.

---

### Artifact compatibility

Rust must be able to read existing Python-generated `.minimap/` directories.

At minimum:

```text
.minimap/config.json
.minimap/graph/screens/*.json
.minimap/graph/edges/*.json
.minimap/routes/*.json
.minimap/checks/*.json
.minimap/proposals/*.json
```

Rust should not require users to delete and recreate their Minimap graph.

---

## Schema migration policy

Every committed artifact must include a `schema_version`.

Rust behavior:

```text
same major version: read normally
older minor version: read and normalize in memory
newer supported version: read if compatible
newer unsupported version: fail closed with clear error
unknown version: fail closed
```

Add:

```bash
minimap migrate --check
minimap migrate --write
```

`minimap migrate --check` should report what would change.

`minimap migrate --write` should produce deterministic diffs.

Do not auto-migrate committed graph files during unrelated commands like `minimap check` or `minimap go`.

---

## Data model corrections to preserve in Rust

### GraphContext is required

Screens, edges, routes, and checks need context guards.

Example fields:

```text
auth_state
onboarding_state
locale
region
feature_flags
experiment_bucket
app_data_state
network_state
device_class
orientation
theme
font_scale
system_insets_profile
```

Rust should treat graph context as part of route resolution.

A route that does not match the current context should be filtered out before being classified as broken.

---

### Screen matching uses similarity, not exact hashes

Exact hashes are a fast path only.

Screen identity should be based on:

```text
stable element signatures
selector presence
role/class structure
important postcondition elements
screen-specific checks
weighted similarity score
context compatibility
```

Suggested approach:

```text
1. Redact layout.
2. Normalize layout.
3. Extract stable element signatures.
4. Compare current signature set to candidate ScreenNodes.
5. Compute weighted similarity.
6. Pick the best candidate above threshold.
7. Return ambiguous if multiple candidates are close.
```

Do not rely on SHA equality as the main matching mechanism.

---

### Routes are preferred paths, not the graph itself

A route may store a preferred path:

```json
{
  "preferred_edge_ids": ["edge_home_to_article", "edge_article_to_share"]
}
```

But route resolution should be able to fall back to graph traversal:

```text
current screen → target screen
```

Use context-compatible edges only.

For v1, simple BFS with edge scoring is sufficient.

Possible edge score inputs:

```text
selector strength
historical reliability from local state
context specificity
last successful local validation
number of fallbacks required
```

Do not store volatile score counters in committed artifacts.

---

### Text is excluded by default

Committed graph artifacts should not store verbatim text unless explicitly allowed.

Instead store:

```text
role
class
resource id if stable
test tag if available
content description only if non-sensitive and allowed
text category or redacted token
structural position
normalized bounds if needed
```

Text selectors may exist as runtime candidates, but committing them requires explicit policy.

---

### Coordinates are guarded fallbacks

Cached tap points are useful but fragile.

A tap cache must include a device/context guard:

```text
device class
viewport size class
orientation
density bucket
system inset profile
font scale
locale
theme
edge-to-edge state if detectable
```

If guards do not match, Minimap should fall back to selector resolution or safe mode.

Fast mode must never be the default reliability path.

---

## Rust command migration order

### Phase 0 — Freeze Python behavior

Deliverables:

```text
- command inventory
- JSON schema snapshots
- exit-code table
- fixture corpus
- Python golden outputs
- committed artifact examples
- privacy regression examples
```

Actions:

1. Run every Python command against controlled fixtures.
2. Save stdout, stderr, exit code, and file diffs.
3. Mark volatile fields to ignore.
4. Add Python tests if they do not exist.
5. Decide whether any Python behavior should be fixed before Rust parity starts.

Exit criteria:

```text
A Rust implementation can be compared to Python with deterministic fixtures.
```

---

### Phase 1 — Rust workspace and skeleton CLI

Implement:

```text
cargo workspace
minimap-cli command tree
central result rendering
central exit code mapping
basic config loading
basic repo root detection
```

Commands initially stubbed:

```bash
minimap --version
minimap help
minimap init --dry-run
minimap doctor --json
```

Exit criteria:

```text
Rust binary builds on supported platforms.
Basic CLI shape matches Python.
```

---

### Phase 2 — Repo and schema layer

Implement:

```text
MinimapConfig parsing
ScreenNode parsing
NavigationEdge parsing
Route parsing
Check parsing
Proposal parsing
schema_version validation
read existing .minimap/ from Python
write deterministic JSON
```

Do not implement Android calls yet.

Commands:

```bash
minimap graph inspect --json
minimap migrate --check --json
```

Exit criteria:

```text
Rust can read Python-generated .minimap/ fixtures and round-trip without unintended diffs.
```

---

### Phase 3 — Init and skill installation

Implement:

```bash
minimap init
minimap init --dry-run
minimap init --force
minimap doctor
```

Rules:

```text
idempotent by default
never overwrite user files without backup or explicit force
patch .gitignore safely
write skills to configured paths
report all file changes in --json
```

Skill paths should be configurable.

Default multi-write strategy should cover known repo-local paths:

```text
.agents/skills/minimap-app-navigation/
.codex/skills/minimap-app-navigation/
.claude/skills/minimap-app-navigation/
.skills/minimap-app-navigation/
.agent/skills/minimap-app-navigation/
```

Use one canonical skill body with small adapters only if necessary.

Exit criteria:

```text
Running minimap init twice produces no unwanted diff.
Skill files are installed where configured.
```

---

### Phase 4 — Android CLI and adb wrappers

Implement `minimap-android`.

Commands:

```bash
minimap layout --json
minimap layout --diff --json
minimap screen capture --annotate
minimap screen resolve ...
minimap tap ...
```

Important implementation detail:

```text
minimap tap is Minimap behavior, not a direct android CLI wrapper.
```

Supported tap paths:

```text
1. Resolve selector from latest layout bounds, then adb shell input tap X Y.
2. Resolve visual label using android screen capture --annotate + android screen resolve, then adb shell input tap X Y.
3. Use guarded normalized coordinate fallback, then adb shell input tap X Y.
```

Exit criteria:

```text
Rust can collect layouts and perform taps through fake Android CLI/adb tests and at least one live-device smoke test.
```

---

### Phase 5 — Redaction and normalization

Implement pipeline:

```text
RawAndroidLayout
  → RedactedLayout
  → NormalizedLayout
  → StableElementSet
```

Tests:

```text
PII does not survive into committed artifacts
redaction happens before fingerprinting
stable fixture normalizes identically across runs
volatile list content is collapsed
text is excluded by default
```

Exit criteria:

```text
Rust normalized output matches Python golden fixtures or intentional differences are documented.
```

---

### Phase 6 — Screen matching and checks

Implement:

```bash
minimap check --current --json
minimap check <target> --json
```

Core logic:

```text
load graph
filter by GraphContext
normalize current layout
score candidate screens
return matched / ambiguous / unknown
run checks
```

Exit criteria:

```text
Rust check results match Python golden fixtures.
Ambiguous and unknown screen cases are covered.
```

---

### Phase 7 — Route resolution and navigation

Implement:

```bash
minimap route <target> --json
minimap go <target> --mode safe --json
minimap go <target> --mode verified --json
minimap go <target> --mode fast --json
```

Route resolution:

```text
1. Try preferred route path if context-compatible.
2. If preferred path fails or is missing, search graph for alternate context-compatible path.
3. Score candidate paths.
4. Return route plan or not_found.
```

Navigation modes:

```text
safe: layout + selector resolution before each tap, verify after each transition
verified: compact current-screen check, use selector/cache, verify transition
fast: use guarded tap cache, validate after transition, fallback if mismatch
```

Exit criteria:

```text
Known target navigation works against fixture graph and fake Android CLI.
The core demo can reach a known screen with fewer layout JSON tokens exposed to the agent.
```

---

### Phase 8 — Observation recording

Implement:

```bash
minimap observe start <name>
minimap observe stop
minimap layout --json      # records when observation is active
minimap tap ...            # records when observation is active
```

Observation run format should remain gitignored.

Record:

```text
redacted/normalized layouts
action reason
selector candidate used
tap point
pre/post screen match if known
Android CLI command metadata
adb action metadata
```

Exit criteria:

```text
A first-run agent exploration produces a complete ObservationRun fixture.
```

---

### Phase 9 — Learning and proposals

Implement:

```bash
minimap learn --from-current-run --stage --json
minimap accept <proposal-id> --json
```

Proposal kinds:

```text
screen_added
screen_changed
edge_added
edge_changed
route_added
route_changed
check_added
selector_drift
context_guard_added
```

Rules:

```text
learn may stage proposals automatically
accept requires explicit command
accept writes committed graph files
accept must produce deterministic diffs
```

Exit criteria:

```text
Rust can convert a run trace into the same graph proposal as Python.
Accepting a proposal produces stable .minimap/ files.
```

---

### Phase 10 — Drift and validation

Implement:

```bash
minimap drift --json
minimap validate --json
minimap validate --all --json
minimap validate --changed-files changed-files.txt --json
```

Do not confuse:

```text
minimap layout --diff  = Android CLI in-session layout delta
minimap drift          = current app versus committed Minimap graph
minimap validate       = route/check validation against committed Minimap graph
```

V1 impact analysis:

```text
Use route triggers based on path globs and explicit route names.
If no matching triggers, recommend --all rather than pretending impact analysis is complete.
```

Exit criteria:

```text
Rust detects broken route, selector drift, unknown screen, and expected changed screen cases in fixtures.
```

---

### Phase 11 — Repair and mapping

Implement after core parity:

```bash
minimap repair <edge-or-route> --stage --json
minimap map --discover --max-actions 50 --stage --json
```

Rules:

```text
repair proposes, does not silently accept
map is always budgeted
map is always staged
map can be interrupted safely
```

Exit criteria:

```text
Rust can stage a selector repair and a small discovery proposal.
```

---

### Phase 12 — Shadow mode

Ship Rust alongside Python without replacing it.

Options:

```bash
minimap-rs ...
MINIMAP_ENGINE=rust minimap ...
minimap --engine rust ...
```

For commands that do not mutate state, run both implementations and compare:

```text
route
check with fixture input
learn on recorded run
validate in dry-run mode
```

For mutating commands, run Rust in dry-run/shadow mode and compare expected file diffs.

Exit criteria:

```text
Rust and Python agree on golden fixtures and selected real repos.
```

---

### Phase 13 — Cutover

Cut over when:

```text
all golden parity tests pass
all artifact round-trip tests pass
all privacy regression tests pass
core first-run/second-run demo works in Rust
Rust init is idempotent
Rust can read existing Python .minimap/ repos
Rust produces no timestamp-only diffs on no-op validation
Rust handles Android CLI/adb failure modes cleanly
skills still trigger agents correctly
```

Cutover steps:

1. Release Rust binary as preview.
2. Ask early repos to run `minimap migrate --check`.
3. Ask early repos to run Rust in shadow mode.
4. Fix mismatches.
5. Make Rust the default binary.
6. Keep Python fallback for one release window.
7. Remove Python runtime dependency after confidence is high.

---

## Testing strategy

### Unit tests

Cover:

```text
schema parsing
schema version rejection
redaction
normalization
stable element extraction
screen similarity
selector scoring
route pathfinding
context filtering
proposal generation
exit code mapping
```

---

### Golden tests

For every important command, capture:

```text
stdin / fixture input
stdout JSON
stderr
exit code
file diffs
```

Compare Rust against Python.

Normalize volatile values before comparison.

---

### Artifact round-trip tests

For `.minimap/` fixture repos:

```text
load with Rust
write to temp repo
compare committed files
ensure no unwanted diffs
```

---

### Privacy regression tests

Fixtures should include sensitive-looking data:

```text
email addresses
phone numbers
JWT-like strings
password fields
chat messages
search queries
contact names
payment-like strings
```

Assertions:

```text
not present in committed graph
not included in fingerprints before redaction
not printed in JSON output unless explicitly allowed
not written to proposals unless explicitly allowed
```

---

### Fake Android CLI tests

Implement a fake `android` binary and fake `adb` binary for tests.

The fake should support:

```text
android layout
android layout --diff
android screen capture --annotate
android screen resolve
adb shell input tap
adb shell input text
adb shell input keyevent
```

This avoids requiring an emulator for most CI tests.

---

### Live device smoke tests

Keep these minimal:

```text
minimap doctor
minimap layout --json
minimap tap with coordinates on known test app
minimap check known screen
minimap go known route
```

Live tests are important but should not be the whole test suite.

---

## Performance and token-savings strategy

Rust should preserve the product’s true savings mechanism.

Minimap saves time/tokens mainly by:

```text
1. Avoiding repeated agent reasoning over full Android layout JSON.
2. Returning compact route/check results instead of full layout trees.
3. Reusing committed tap recipes and screen expectations.
4. Falling back to full layout inspection only when confidence is too low.
```

In verified mode, `minimap check --current` should return compact match output, not dump the full layout JSON to the agent.

Example:

```json
{
  "status": "matched",
  "screen": "article-detail",
  "confidence": "high",
  "checks": [
    { "kind": "element_exists", "name": "article_body", "status": "passed" }
  ]
}
```

Do not send large layout JSON to the agent unless the command explicitly asks for it.

---

## Error model

Rust should classify errors into product states.

Examples:

```text
environment_error: android CLI missing
environment_error: adb missing
environment_error: no device connected
environment_error: app not launched
layout_error: android layout failed
screen_unknown: no matching ScreenNode
screen_ambiguous: multiple candidates above threshold
selector_drift: expected selector missing but similar candidate found
route_broken: expected target not reached
schema_error: unsupported artifact schema
privacy_error: redaction invariant failed
```

Every error should include:

```text
status
human summary
machine-readable code
recommended next command when possible
exit code
```

---

## Skill migration

The Rust migration must preserve how agents discover and use Minimap.

Recommended skill name:

```text
minimap-app-navigation
```

This is more future-proof than `minimap-android-ui`, while the skill content can still be Android-specific in v1.

Default repo-local install paths:

```text
.agents/skills/minimap-app-navigation/SKILL.md
.codex/skills/minimap-app-navigation/SKILL.md
.claude/skills/minimap-app-navigation/SKILL.md
.skills/minimap-app-navigation/SKILL.md
.agent/skills/minimap-app-navigation/SKILL.md
```

`minimap init` should allow:

```bash
minimap init --skills codex,claude,android-studio
minimap init --skills all
minimap init --skills none
minimap init --dry-run
```

Rules:

```text
idempotent
no silent overwrites
show planned file changes in dry run
preserve user edits where possible
```

The skill should instruct agents:

```text
- use Minimap before raw Android layout/tap exploration
- run minimap route before manual exploration
- run minimap go for known targets
- use minimap observe/layout/tap when discovering unknown paths
- stage graph updates but do not accept them without explicit approval
- use minimap check/validate for soft validation
```

---

## Distribution plan

Rust enables simpler distribution.

Recommended distribution targets:

```text
GitHub release binaries
Homebrew tap
cargo install for developers comfortable with Rust
optional install script
```

Do not require Python at runtime after cutover.

During transition:

```text
minimap-python may remain available as fallback
minimap-rs may be preview binary
```

After cutover:

```text
minimap is Rust
Python code remains only for historical comparison or internal experiments
```

---

## Risk register

### Risk: Rust accidentally changes product semantics

Mitigation:

```text
Python golden fixtures
command parity tests
artifact round-trip tests
shadow mode
contract-first migration
```

---

### Risk: Screen matching differs from Python

Mitigation:

```text
fixture corpus covering dynamic lists, modals, empty states, loading states, locale changes, and Compose screens without tags
explicit scoring formula
threshold tests
ambiguous result tests
```

---

### Risk: Privacy regression

Mitigation:

```text
redaction-first typed pipeline
privacy fixtures
fail-closed privacy assertions
no verbatim text by default
```

---

### Risk: Skill install paths drift across agents

Mitigation:

```text
multi-write strategy
configurable paths
minimap doctor checks skill presence
repo adapter snippets
clear init dry run
```

---

### Risk: Existing `.minimap/` repos fail to load

Mitigation:

```text
schema compatibility tests
fixture repos from real Python output
minimap migrate --check
clear unsupported-version errors
```

---

### Risk: Rust implementation over-engineers too early

Mitigation:

```text
keep v1 synchronous
no daemon
no database
no embedded scripting
no ML dependency in core CLI
no plugin runtime until needed
```

---

### Risk: Migration distracts from product validation

Mitigation:

```text
freeze feature work during core parity
migrate highest-value commands first
keep demo-driven acceptance criteria
avoid redesigning Minimap under the banner of Rust
```

---

## Cutover acceptance criteria

Rust can replace Python when all of the following are true:

```text
[ ] Rust reads existing Python-generated .minimap/ repos.
[ ] Rust writes deterministic graph artifacts.
[ ] Rust init is idempotent and supports --dry-run.
[ ] Rust doctor classifies environment failures clearly.
[ ] Rust layout wraps Android CLI correctly.
[ ] Rust tap works through adb and does not assume android tap exists.
[ ] Rust redacts before hashing or storage.
[ ] Rust excludes verbatim text from committed artifacts by default.
[ ] Rust check identifies known, unknown, and ambiguous screens.
[ ] Rust route resolves preferred paths and graph fallback paths.
[ ] Rust go reaches known targets in verified mode.
[ ] Rust observe records state/action/state traces.
[ ] Rust learn stages graph proposals matching Python fixtures.
[ ] Rust accept applies deterministic graph updates.
[ ] Rust validate classifies route_broken, selector_drift, screen_unknown, and passed_with_changes.
[ ] Rust produces stable JSON outputs for agents.
[ ] Rust exit codes match the contract.
[ ] Rust produces no timestamp-only diffs on successful no-op validation.
[ ] Rust installs skills to configured repo-local paths.
[ ] The first-run learns / second-run reuses demo works end to end.
```

---

## Demo required before cutover

Use a small Android sample app.

First run:

```bash
minimap route article-detail --json
minimap observe start article-detail
minimap layout --json
minimap tap --selector "..." --reason "open article detail" --json
minimap check --current --json
minimap observe stop
minimap learn --from-current-run --stage --json
minimap accept <proposal-id> --json
```

Expected result:

```text
Minimap stages and accepts a screen + edge + route graph update.
```

Second run:

```bash
minimap go article-detail --mode verified --json
```

Expected result:

```text
Minimap reaches the known screen without requiring the agent to reason over the full layout JSON again.
```

Change detection:

```bash
minimap validate --all --json
```

Expected result:

```text
Minimap detects whether the running app still matches the committed graph and either passes, flags drift, or stages an expected update proposal.
```

This demo proves the product, not just the rewrite.

---

## Recommended immediate next steps

1. Add this document to the repo as:

```text
docs/MINIMAP_PYTHON_TO_RUST_MIGRATION_PLAN.md
```

2. Freeze the Python CLI contract.
3. Generate golden fixtures from the Python implementation.
4. Create the Rust workspace skeleton.
5. Implement schema loading and artifact round-tripping before Android integration.
6. Migrate read-only commands first.
7. Migrate Android-wrapper commands next.
8. Migrate mutating graph commands last.
9. Run Rust in shadow mode.
10. Cut over only after parity and demo acceptance.

---

## Final summary

Minimap should move to Rust, but the migration should be boring and contract-driven.

The Python implementation has already encoded important product decisions. Preserve those decisions first. Then use Rust to make Minimap easier to install, safer to run, more predictable for agents, and more durable as shared repo infrastructure.

The Rust rewrite succeeds when users and agents experience no product disruption except that Minimap becomes easier to install, faster to run, and harder to break.
