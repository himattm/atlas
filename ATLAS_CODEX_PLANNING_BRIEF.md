# Atlas Codex Planning Brief — corrected implementation brief

Use this document as the source brief for Codex when asking it to produce a detailed implementation plan for Atlas.

The goal is not for Codex to immediately implement everything. The goal is for Codex to read this brief, inspect the repository, identify what already exists, resolve implementation risks honestly, and produce a staged implementation plan that can guide development.

This version intentionally corrects assumptions about Android CLI, skill installation, graph matching, state, privacy, and multi-agent repo storage.

---

## Paste-this prompt for Codex

```text
Read `ATLAS_CODEX_PLANNING_BRIEF.md` fully before proposing changes.

Your task is to create a very detailed implementation plan for Atlas. Do not start coding yet.

Atlas is shared navigation memory and soft validation for AI agents working in Android codebases. It captures how agents navigate a launched Android app, stores that knowledge as a repo-committed graph, and lets future agents reuse the graph to save tokens/time while detecting unexpected differences from the repo’s expected navigation state.

Produce a plan that covers architecture, repo layout, CLI command design, data schemas, Android CLI integration, agent skill packaging, graph context/state, screen matching, route resolution, privacy/redaction, merge-safe storage, graph update workflow, validation behavior, error handling, testing, milestones, and risks.

Before writing the final plan:
1. Inspect the current repo structure.
2. Identify what already exists and what needs to be built.
3. Identify unknowns, contradictions, and assumptions.
4. Ask at most one blocking clarification question at a time only if truly necessary.
5. If no clarification is required, make reasonable assumptions and state them clearly.

The output should be a living execution plan suitable for implementation. Include explicit file paths, command signatures, JSON schemas, phased milestones, acceptance criteria, and a test strategy. Do not hand-wave Android CLI integration, state/context handling, skill installation, privacy, or the multi-agent repo-sharing story.
```

---

# 1. Product definition

## Product name and namespace

The product name is **Atlas**.

Use these names unless the implementation plan explicitly calls out a packaging conflict:

```text
Product: Atlas
Default CLI namespace: atlas
Repo directory: .atlas/
Primary skill: atlas-app-navigation
Codex planning brief: ATLAS_CODEX_PLANNING_BRIEF.md
Implementation plan: docs/ATLAS_IMPLEMENTATION_PLAN.md
```

The `atlas` CLI name has public prior art, including database tooling. For the planning phase, use `atlas`. Before public distribution, evaluate whether the final binary/package should be renamed to avoid collisions.

## Correct framing

Atlas is:

> **Shared navigation memory and soft validation for AI agents working in Android codebases.**

Atlas is not only for a special class of “Android coding agents.” The agent is generic: Codex, Claude Code, Gemini CLI, Android Studio Agent Mode, or another AI coding agent. The codebase/runtime domain is Android.

Atlas captures app navigation learned during agent runs and stores it as a repo-committed graph of screens, tap recipes, routes, checks, and expectations. Future agents and teammates reuse that graph instead of rediscovering the UI from scratch.

## One-line product statement

> **Atlas turns agent navigation into committed repo memory: faster reuse when the app behaves as expected, and soft validation when it does not.**

## Longer product statement

Atlas is shared navigation memory and soft validation for AI agents working in Android codebases. It captures how agents navigate the running Android app, commits that knowledge as a graph of screens, tap recipes, routes, and checks, and lets future agents reuse the graph to save tokens and time while detecting unexpected differences from the repo’s expected navigation state.

---

# 2. Core insight

AI agents working in Android repos often need to operate the running app. Without Atlas, a typical agent loop looks like this:

```text
1. Ask Android CLI for layout JSON.
2. Read and reason over the JSON.
3. Decide which element to tap.
4. Execute a tap, usually through adb shell input tap.
5. Ask Android CLI for layout JSON again.
6. Confirm where it landed.
7. Repeat until the desired screen or state is reached.
```

That first run is expensive. It costs time, tool calls, tokens, and reasoning effort.

But once the agent has learned how to navigate from one screen to another, that knowledge should not disappear with the session.

Atlas turns:

```text
state -> layout JSON -> reasoning -> tap -> new state
```

into a reusable repo artifact:

```text
ScreenNode -- NavigationEdge/TapRecipe --> ScreenNode
```

The graph then serves two purposes:

1. **Navigation cache**: future agents skip repeated layout exploration and navigate faster.
2. **Expectation baseline**: if the current app diverges from the committed graph, Atlas calls out the difference as possible expected change, selector drift, or regression.

The key savings are not that Atlas eliminates all device observation. In verified mode Atlas may still inspect the current screen. The key savings are that Atlas returns compact match/route/check results instead of repeatedly feeding full layout JSON into the agent’s context.

---

# 3. Product goals

## Primary goals

1. **Reduce repeated navigation cost**
   - Save tokens by avoiding repeated full layout JSON reasoning.
   - Save time by reusing known routes and tap recipes.
   - Make the “second run” substantially faster than the first.

2. **Create repo-shared app navigation memory**
   - Store learned navigation in the repository.
   - Let every developer and every AI agent reuse the same app map.
   - Make the graph inspectable, reviewable, and diffable in git.

3. **Provide soft validation**
   - Use the committed graph as the expected navigation state.
   - Detect unexpected changes when a route no longer reaches the expected screen.
   - Distinguish likely expected changes from likely regressions when possible.

4. **Make learning natural**
   - Agents should use Atlas instead of raw layout/tap loops.
   - Atlas should wrap Android CLI primitives so exploration automatically becomes learnable.

5. **Package as a CLI plus standard skills**
   - Atlas must be usable by Codex, Claude Code, Gemini CLI, Android Studio Agent Mode, and future agents.
   - Skills should be installed into the correct repo-local locations for each supported agent, not a guessed single path.

---

# 4. Prior art and positioning

Atlas must explicitly position itself against nearby tools so Codex does not accidentally design a duplicate testing framework.

## Maestro

Maestro is a mobile UI automation/testing framework with declarative YAML flows. Atlas is different: Atlas’s primary artifact is shared navigation memory for AI agents, not human-authored test flows. Atlas routes may validate behavior, but the graph’s primary job is to help agents reuse navigation knowledge across sessions and teammates.

## Arbigent

Arbigent is an AI-agent-driven testing framework for Android/iOS/web. Atlas is not trying to be an autonomous QA agent. Atlas is repo-committed memory that any coding agent can use while working in an Android codebase.

## agent-device

agent-device provides mobile device control, accessibility snapshots, replayable scripts, and deterministic automation across platforms. Atlas should learn from its token-efficient snapshot and replay ideas, but the Atlas wedge is repo-committed, multi-agent navigation memory plus soft validation from the committed graph.

## Android CLI

Android CLI provides official low-level Android agent primitives. Atlas should sit above it. Android CLI helps an agent observe and act in the current session. Atlas helps agents remember and share navigation knowledge across sessions, branches, agents, and teammates.

---

# 5. Non-goals for v1

Atlas v1 is not:

```text
- A full testing framework.
- A replacement for Espresso, Maestro, UIAutomator tests, or agent-device.
- A replacement for Android CLI.
- A full autonomous app crawler without limits.
- A build/deploy system.
- A CI product.
- An iOS/web product, though naming should not make future platforms impossible.
- A cloud registry.
- A visual embedding platform.
- A silent auto-mutator of repo baselines.
```

Atlas v1 should not own:

```text
- Gradle build.
- APK generation.
- Full deployment lifecycle.
- Emulator lifecycle.
- App launch as a hard requirement.
```

Atlas v1 may assist with:

```text
- Android CLI availability checks.
- Device/emulator checks.
- Layout-readability checks.
- Configured runtime permission checks/grants.
- System dialog and overlay detection.
- Current screen identification.
- Known route navigation.
- Learning new graph edges.
- Staged graph updates.
- Validation of known routes.
```

---

# 6. Real Android CLI command surface

Atlas must use the real Android CLI surface, not invented commands.

## Android CLI commands Atlas can wrap

As of the current Android CLI docs, the relevant command surface includes:

```bash
android layout [--pretty] [--output] [--diff]
android screen capture [--output] [--annotate]
android screen resolve --screenshot=<path> --string=<string>
android describe [--project_dir=<project-directory>]
android info
android init
android skills list [--long]
android skills add [--all] [--agent=<agent-name>] [--skill=<skill-name>]
android skills find <string>
android skills remove [--agent] --skill=<skill-name>
```

Important constraints:

1. `android layout --diff` returns changes since the last in-session internal layout snapshot, not differences against Atlas’s committed graph.
2. There is no documented `android tap` command.
3. `android screen resolve` does not execute a tap. It returns a string such as `input tap 500 1000`, which must then be executed, typically via `adb shell input tap 500 1000`.

## Atlas command semantics must disambiguate diff concepts

Use different names for different concepts:

```text
Android in-session layout delta:
  atlas layout --diff --json

Atlas committed-graph divergence:
  atlas drift --json
  atlas validate --json
```

Avoid a generic `atlas diff` in v1 unless it is namespaced, for example:

```bash
atlas graph diff --base main --json
```

## Atlas tap is a composition, not a direct Android CLI wrapper

`atlas tap` must resolve and execute input through one of these paths:

### Path A — layout selector path

```text
1. Use latest `android layout` output or call `android layout`.
2. Resolve selector against normalized layout tree.
3. Compute tap coordinates from element bounds.
4. Execute `adb shell input tap X Y`.
5. Observe resulting layout or compact check.
```

### Path B — visual annotation fallback

```text
1. Run `android screen capture --annotate --output=<path>`.
2. Ask the agent or resolver to choose label #N.
3. Run `android screen resolve --screenshot=<path> --string="input tap #N"`.
4. Parse returned `input tap X Y`.
5. Execute `adb shell input tap X Y`.
6. Observe resulting layout or compact check.
```

### Path C — direct coordinate fallback

```text
1. Accept explicit normalized or absolute coordinates.
2. Convert normalized coordinates to absolute coordinates using current device profile.
3. Execute `adb shell input tap X Y`.
4. Observe resulting layout or compact check.
```

The implementation plan must specify the exact executor abstraction for adb and how errors are captured.

---

# 7. Intended layering

Atlas should sit above Android CLI.

```text
AI coding agent
    ↓
Atlas skill instructions
    ↓
Atlas CLI
    ↓
Android CLI + adb shell for input execution
    ↓
Device / emulator
```

Android CLI gives low-level observe/describe/screen primitives. Atlas adds persistence, normalization, route lookup, check evaluation, staged graph updates, and compact agent outputs.

## Important implementation principle

Atlas commands should feel like better versions of the loops agents already use.

Instead of forcing agents to do this:

```bash
android layout
# agent reads huge JSON
adb shell input tap 420 900
android layout
```

Atlas should support this:

```bash
atlas layout --json
atlas tap --selector "text=Settings" --reason "open settings" --json
atlas check --current --json
```

Or, for reuse:

```bash
atlas go settings --json
```

---

# 8. Key behavior loop

## First run: learn

```text
1. Agent needs to reach a screen.
2. Agent asks Atlas for a route.
3. Atlas does not know one.
4. Agent explores through Atlas wrappers.
5. Atlas records layout observations, actions, transitions, and checks.
6. Agent reaches target.
7. Atlas turns the run into staged graph updates.
8. Human or explicit instruction accepts the graph updates.
9. The graph is committed to the repo.
```

The first selector comes from the agent reading `atlas layout --json` output. Atlas does not magically choose every first-run action. The value is that the first-run decision becomes captured and reusable.

## Later runs: reuse

```text
1. Agent needs to reach the same screen.
2. Agent asks Atlas for a route.
3. Atlas finds a route or resolves a path through the graph.
4. Atlas navigates using known edges and tap recipes.
5. Atlas returns compact check results instead of full layout JSON.
6. Agent avoids repeated layout discovery.
```

## Divergence: soft validation

```text
1. Atlas uses the committed graph to navigate.
2. The running app does not match the expected screen/edge/check.
3. Atlas classifies the mismatch.
4. Atlas either flags possible regression or stages a graph update proposal.
5. Agent explains the difference.
6. Human or explicit instruction decides whether to accept the update.
```

## App mapping: baseline discovery

```text
1. User asks agent to map the app.
2. Atlas runs a budgeted discovery process.
3. Atlas stages screens, edges, routes, and checks.
4. Human reviews and accepts the baseline graph.
5. Future agents reuse the map.
```

---

# 9. State and context model

Real apps have different graphs under different conditions. Atlas must model this from v1.

## GraphContext

Add a `GraphContext` concept used by screens, edges, routes, and checks.

Examples of context dimensions:

```text
- auth_state: logged_out, logged_in, seeded_demo_user, unknown
- onboarding_state: first_launch, completed, unknown
- locale: en-US, fr-FR, unknown
- region: US, EU, unknown
- feature_flags: key/value map, unknown allowed
- experiment_bucket: known bucket or unknown
- app_data_state: fresh_install, existing_data, seeded_fixture, unknown
- network_state: online, offline, mocked, unknown
- device_class: phone, tablet, foldable, unknown
- orientation: portrait, landscape
- theme: light, dark, system
```

## Context guards

Context guards must exist at multiple levels:

```text
AppGraph.default_context
ScreenNode.context_guard
NavigationEdge.context_guard
Route.context_guard
Check.context_guard
```

If the current context cannot satisfy a guard, Atlas should not classify the route as broken. It should return:

```text
status: context_mismatch
recommended_action: establish required context or choose another route variant
```

This avoids spurious route-broken results when, for example, the app is logged out but the route requires a logged-in home screen.

## Context discovery

V1 does not need perfect context detection. It should support:

```text
- user-provided config values
- route-level declared context
- observed system/app state where easy
- `unknown` values that prevent overconfident validation
```

---

# 10. Graph-first data model

The product should be graph-first from v1.

Do not start with only linear flows. Linear flows duplicate knowledge and make later graph migration harder.

## Core objects

```text
AppGraph
├── GraphContext
├── ScreenNode
├── NavigationEdge
├── TapRecipe
├── Route
├── Check
├── ObservationRun
├── GraphUpdateProposal
└── RuntimeState / Telemetry (gitignored)
```

## ScreenNode

A ScreenNode represents a recognizable app state under an optional context guard.

It answers:

```text
- What is this screen?
- Under what app context is it valid?
- How do we recognize it again?
- Which elements are stable enough to define it?
- Which elements are volatile/noisy?
- Which checks prove we are where we think we are?
```

Example:

```json
{
  "schema_version": "atlas.screen.v1",
  "id": "screen_article_detail",
  "name": "article-detail",
  "context_guard": {
    "auth_state": "logged_in_or_unknown",
    "locale": "any",
    "orientation": "portrait"
  },
  "source": {
    "kind": "android_cli_layout",
    "first_observed_with": "android layout"
  },
  "identity_hash": "sha256:redacted-normalized-fast-path-only",
  "match_profile": {
    "required_stable_elements": [
      {
        "role": "body",
        "selector": { "kind": "accessibility_or_semantic", "value": "article_body" },
        "weight": 5
      },
      {
        "role": "top_app_bar",
        "selector": { "kind": "structural_role", "value": "top_app_bar" },
        "weight": 2
      }
    ],
    "optional_stable_elements": [
      {
        "role": "title",
        "selector": { "kind": "visible_text_class", "value": "heading" },
        "weight": 2
      }
    ],
    "volatile_elements": [
      {
        "kind": "list",
        "normalization": "collapse_repeating_children"
      }
    ],
    "match_threshold": 0.78
  },
  "checks": [
    {
      "kind": "element_exists",
      "selector": { "kind": "accessibility_or_semantic", "value": "article_body" },
      "required": true
    }
  ],
  "aliases": ["article page", "story detail"]
}
```

Do not rely on exact SHA matching as the primary screen identity. The hash is a fast-path only. Matching must use a similarity score over redacted, normalized stable elements.

## NavigationEdge

A NavigationEdge represents a learned transition from one screen to another.

It is both:

```text
- an instruction: how to move from screen A to screen B
- an expectation: what should happen after the action
```

Example:

```json
{
  "schema_version": "atlas.edge.v1",
  "id": "edge_home_feed_to_article_detail",
  "from_screen": "home-feed",
  "to_screen": "article-detail",
  "context_guard": {
    "auth_state": "logged_in_or_unknown",
    "locale": "any",
    "orientation": "portrait"
  },
  "intent": "open an article from the feed",
  "action": {
    "kind": "tap",
    "description": "tap the first article card",
    "selector_candidates": [
      {
        "kind": "accessibility_or_semantic",
        "value": "article_card",
        "score": 0.95
      },
      {
        "kind": "visible_text_fuzzy",
        "value_class": "article_title_like",
        "score": 0.72
      },
      {
        "kind": "structural_position",
        "parent_role": "list",
        "child_index": 0,
        "score": 0.64
      },
      {
        "kind": "normalized_coordinate",
        "x": 0.51,
        "y": 0.34,
        "score": 0.35
      }
    ],
    "tap_cache": {
      "validity": {
        "device_class": "phone",
        "orientation": "portrait",
        "viewport_bucket": "medium_phone_1080x2400",
        "density_bucket": "xxhdpi",
        "system_insets_signature": "gesture-nav-edge-to-edge",
        "locale": "en-US"
      },
      "normalized_point": { "x": 0.51, "y": 0.34 },
      "source_bounds_normalized": {
        "left": 0.04,
        "top": 0.27,
        "right": 0.96,
        "bottom": 0.41
      },
      "invalidate_if_context_differs": true
    }
  },
  "expectations": [
    {
      "kind": "screen_reached",
      "screen": "article-detail",
      "timeout_ms": 3000
    },
    {
      "kind": "element_exists",
      "selector": { "kind": "accessibility_or_semantic", "value": "article_body" }
    }
  ],
  "learned_from": {
    "source": "agent-navigation"
  },
  "confidence_model": {
    "selector_confidence": 0.72,
    "transition_confidence": 0.91
  }
}
```

Do not commit volatile success counts, failure counts, or last-validated timestamps inside edge files. Store those in gitignored runtime telemetry.

## TapRecipe

A TapRecipe is the reusable memory inside a NavigationEdge.

It must store both:

```text
1. How to find the target element again.
2. Where the agent tapped last time, with strict validity guards.
```

Selectors provide portability. Cached normalized tap points provide speed only when device/context validity checks pass.

## Route

A Route is a named intent and preferred path through the graph, not the entire routing model.

Routes are useful when a task has a named goal:

```text
- go to article-detail
- validate read-article
- open bookmarks
- reproduce checkout failure
```

Example:

```json
{
  "schema_version": "atlas.route.v1",
  "name": "read-article",
  "intent": "user opens and reads an article",
  "start": {
    "screen": "home-feed",
    "context_guard": {
      "auth_state": "logged_in_or_unknown",
      "orientation": "portrait"
    }
  },
  "target": {
    "screen": "article-detail"
  },
  "preferred_edge_ids": ["edge_home_feed_to_article_detail"],
  "allow_graph_fallback": true,
  "path_constraints": {
    "max_edges": 4,
    "avoid_screens": ["system-permission-dialog"],
    "require_checks": true
  },
  "checks": [
    { "kind": "flow_reaches_screen", "screen": "article-detail" },
    {
      "kind": "element_exists",
      "selector": { "kind": "accessibility_or_semantic", "value": "article_body" }
    }
  ],
  "triggers": [
    { "kind": "path", "value": "app/src/**/article/**" },
    { "kind": "intent", "value": "read-article" }
  ]
}
```

## Check

A Check is a lightweight expectation.

It can apply to:

```text
- current screen
- target screen
- edge transition
- route
- system overlay state
- app context
```

Examples:

```text
- element exists
- screen similarity >= threshold
- route reaches target
- no unexpected intermediate screen
- no blocking system permission dialog
- current context satisfies route guard
```

## ObservationRun

ObservationRun is the raw trace of a first-run exploration.

It should include:

```text
- raw or compact layout observations
- Android in-session layout deltas
- actions
- tap reasons
- resolved selectors
- coordinates
- before/after screen matches
- timestamps
- command outputs
```

ObservationRun artifacts should be gitignored by default.

## GraphUpdateProposal

A GraphUpdateProposal is a staged change to committed navigation memory.

Examples:

```text
- screen_added
- screen_changed
- edge_added
- edge_changed
- route_added
- selector_drift
- check_added
- context_guard_added
- unknown_screen
```

Atlas should stage proposals automatically, but not accept them automatically.

## RuntimeState / Telemetry

Store volatile runtime information outside committed graph files:

```text
- last_validated_at
- success_count
- failure_count
- route_reuse_count
- recent drift observations
- timing metrics
```

Recommended path:

```text
.atlas/state/       # gitignored
```

---

# 11. Screen matching algorithm

Exact fingerprints are insufficient. Atlas needs a robust screen matching algorithm.

## Matching pipeline

```text
1. Capture layout via Android CLI.
2. Redact sensitive values before any hashing or storage.
3. Normalize layout into stable element signatures.
4. Compute identity_hash for exact redacted-normalized fast path.
5. If hash does not match, compute similarity against candidate ScreenNodes.
6. Return best match if confidence >= threshold.
7. If no match passes threshold, return screen_unknown and optionally stage screen_added proposal.
```

## Element signature

A normalized stable element signature can include:

```text
- semantic role or class
- clickable/enabled state
- content description class or hash, if allowed by redaction policy
- resource id, if present and stable
- test tag, if exposed and stable
- structural anchor path
- parent role
- approximate normalized bounds bucket
- text class, not verbatim text by default
```

## Suggested similarity formula

The implementation plan should refine this, but v1 can use:

```text
screen_similarity =
  0.55 * weighted_stable_element_jaccard
+ 0.20 * required_check_pass_rate
+ 0.15 * structural_anchor_similarity
+ 0.10 * role_distribution_similarity
```

Default threshold:

```text
0.78 for normal screen match
0.90 for fast-mode assumption
0.65 for repair/proposal candidate
```

## Confidence fields

Do not emit arbitrary confidence values. Define separate confidences:

```text
match_confidence: screen similarity score after normalization
selector_confidence: probability-ish score from selector candidate quality and ambiguity
transition_confidence: post-action screen/check match confidence
edge_confidence: Bayesian success estimate from runtime telemetry, not committed graph
```

Suggested edge confidence from runtime telemetry:

```text
edge_confidence = (success_count + 1) / (success_count + failure_count + 2)
```

Do not commit the success/failure counts by default. They live in `.atlas/state/`.

---

# 12. Compose-first selector strategy

Atlas must assume many Compose apps do not expose broad stable IDs or test tags.

Do not design the selector chain as though `testTag` or `resource_id` will usually exist. They are excellent when present, but v1 must degrade honestly.

## Selector candidates, not one fixed chain

Use scored candidates rather than a rigid chain:

```text
High quality:
- stable accessibility/content description for action elements
- exposed test tag, if available and stable
- stable resource id, if available and stable
- explicit semantic role + stable accessible label

Medium quality:
- visible text class or fuzzy text where text is not sensitive and is expected to be stable
- structural anchor path relative to stable parent
- role + sibling index within stable container

Low quality:
- normalized coordinate within a valid device/context guard
- visual label resolved from annotated screenshot
```

Coordinates must be last-resort or fast-mode-only, and only valid under matching device/context guards.

## Product implication

Atlas may encourage teams to add stable semantics/test tags over time, but v1 cannot require complete test tag coverage. `atlas doctor` should report selector quality and recommend improvements, not fail the entire product when tags are missing.

Example doctor output:

```text
Selector quality: medium-low
Stable semantic targets found: 11
Coordinate-only tap candidates: 6
Recommendation: add stable content descriptions or test tags for icon-only actions and repeated list items.
```

---

# 13. Coordinate and device context policy

Normalized coordinates are useful but fragile. Atlas must treat tap caches as conditional accelerators, not universal truth.

## Device context fields

Tap caches should be keyed/guarded by:

```text
- device_class: phone/tablet/foldable
- orientation
- viewport dimensions bucket
- density bucket
- system inset signature: gesture nav, three-button nav, cutout/notch, edge-to-edge
- locale
- font scale bucket
- theme, if relevant
- app context guard
```

## Mode behavior

```text
safe:
  never relies on cached coordinates unless selector resolution fails and user/agent explicitly allows fallback

verified:
  may use tap cache only if device/context guard matches and a compact current-screen check passes

fast:
  may use tap cache more aggressively, but must fall back to verified/safe when postcondition check fails
```

Fast mode is an optimization, not the default reliability path.

---

# 14. Route resolution algorithm

Routes are named shortcuts. The graph is the real navigation model.

## Route lookup

When the user/agent runs:

```bash
atlas route <target> --json
```

Atlas should:

```text
1. Identify current screen, if possible.
2. Match target by route name, screen name, alias, or intent.
3. Load candidate routes whose context guards match.
4. Try each route’s preferred_edge_ids first.
5. If preferred path is unavailable and allow_graph_fallback is true, search graph for alternate path.
6. Rank paths by confidence, recency from committed graph schema, selector quality, number of edges, and context fit.
7. Return compact route plan.
```

## Graph fallback

Use BFS or Dijkstra-style search over NavigationEdges.

Suggested edge cost:

```text
edge_cost =
  base_cost(1.0)
+ selector_fragility_penalty
+ context_uncertainty_penalty
+ coordinate_only_penalty
+ low_transition_confidence_penalty
```

Return:

```text
- preferred_path_used
- graph_fallback_used
- route_confidence
- reason preferred path was skipped, if any
```

If graph fallback finds a route different from the named preferred route, Atlas should not silently rewrite the route. It can stage a proposal if the new path is validated.

---

# 15. Repository storage and merge strategy

The graph must be committed into the repo so multiple developers and agents share the same navigation memory.

## Recommended structure

```text
.atlas/
├── config.json
├── graph/
│   ├── screens/
│   │   ├── screen_home_feed.json
│   │   ├── screen_article_detail.json
│   │   └── screen_bookmarks.json
│   └── edges/
│       ├── edge_home_feed__article_detail.json
│       ├── edge_home_feed__bookmarks.json
│       └── edge_article_detail__share_sheet.json
├── routes/
│   ├── read-article.atlas.json
│   ├── bookmark-article.atlas.json
│   └── share-article.atlas.json
├── checks/
│   ├── home-feed.checks.json
│   └── article-detail.checks.json
├── proposals/
│   └── proposal-2026-05-06-001.json
├── state/                      # gitignored
└── runs/                       # gitignored
    └── 2026-05-06T15-22-04Z/
        ├── raw-layouts/
        ├── layout-deltas/
        ├── screenshots/
        ├── actions.json
        ├── observations.json
        └── learned-updates.json
```

## No committed central index in v1

Do not commit `.atlas/graph/index.json` in v1.

Reason: a central index will create constant merge conflicts when parallel agents add screens or edges on separate branches.

Instead:

```text
- compute graph index at runtime from filenames and IDs
- optionally cache generated index under .atlas/state/index-cache.json, gitignored
- enforce deterministic file naming and canonical JSON formatting
```

## Commit by default

```text
.atlas/config.json
.atlas/graph/screens/*.json
.atlas/graph/edges/*.json
.atlas/routes/*.json
.atlas/checks/*.json
skill files installed by atlas init
AGENTS.md snippets, if installed
```

## Gitignore by default

```text
.atlas/runs/
.atlas/state/
raw Android layout JSON
screenshots
annotated screenshots
Android in-session layout deltas
debug traces
runtime telemetry
```

## Product rule

> Commit distilled navigation knowledge, not raw observations.

Raw layout JSON is valuable for debugging, but it is too noisy and potentially sensitive to become the default shared artifact.

## Canonical JSON

All committed Atlas JSON should use:

```text
- deterministic key ordering
- stable filenames derived from IDs
- no volatile timestamps/counters
- one object per file where possible
- schema_version at top level
```

---

# 16. Privacy and redaction-first storage

Regex redaction alone is not enough.

## Mandatory rules

```text
1. Redaction runs before fingerprinting, hashing, normalization persistence, or proposal generation.
2. Verbatim text is excluded from committed artifacts by default.
3. Text may be represented as classes, roles, length buckets, or redacted hashes only when needed.
4. Committing raw layout JSON is opt-in and should require an explicit config flag plus command flag.
5. Raw observations remain in .atlas/runs/ and are gitignored by default.
```

## Text policy

Default committed text policy:

```json
{
  "text_policy": {
    "commit_verbatim_text": false,
    "commit_text_hashes": false,
    "allowed_text_roles": [],
    "allowed_static_text_patterns": []
  }
}
```

Teams can opt into specific stable strings when safe, for example static navigation labels.

## Redaction config example

```json
{
  "redaction": {
    "run_before_hashing": true,
    "default_text_action": "exclude",
    "sensitive_resource_id_patterns": ["password", "secret", "auth", "token"],
    "sensitive_text_patterns": ["email", "phone", "credit_card", "jwt", "token"],
    "allowlist_static_text": ["Settings", "Bookmarks", "Home"]
  }
}
```

---

# 17. CLI design

Codex should design the CLI around the product loop.

Every agent-facing command should support:

```bash
--json
--quiet
--no-color
```

Human-facing commands may support:

```bash
--markdown
```

## Setup

```bash
atlas init [--dry-run] [--yes] [--agents codex,claude,android-studio,gemini] [--skill-paths ...]
```

Creates or updates:

```text
.atlas/
.atlas/config.json
.atlas/graph/
.atlas/routes/
.atlas/checks/
.gitignore entries for .atlas/runs/ and .atlas/state/
repo-local skill directories selected by --agents/--skill-paths
optional AGENTS.md / CLAUDE.md / GEMINI.md guidance snippets
```

`atlas init` must be idempotent. It must not overwrite existing instructions without showing a diff or using `--yes`. It must support `--dry-run`.

```bash
atlas doctor --json
```

Checks:

```text
- android CLI exists
- adb exists when tap execution is needed
- device/emulator available
- app package configured
- active app layout readable
- .atlas graph exists
- skill files exist in configured locations
- run/state artifacts are gitignored
- configured permissions are granted, if any
- selector quality summary
- system overlay detection works, if possible
```

## Observation

```bash
atlas observe start <name> --json
atlas observe stop --json
```

Starts/stops a recorded navigation run.

```bash
atlas layout --json
atlas layout --diff --json
```

`atlas layout` wraps `android layout` and adds:

```text
- redaction before hashing
- normalization
- current screen matching
- graph lookup
- observation capture
- compact agent summary
```

`atlas layout --diff` wraps `android layout --diff`. This is an Android in-session layout delta, not a graph-vs-app drift report.

```bash
atlas tap --selector "<selector>" --reason "<why>" --json
atlas tap --point 0.51,0.34 --reason "<why>" --json
atlas tap --label 5 --screenshot .atlas/runs/current/ui.png --reason "<why>" --json
```

Resolves and executes a tap via layout selector, visual annotation, or coordinate fallback. Records the action and observes the result.

The implementation plan must decide whether `atlas tap` always observes after action or supports `--no-observe-after`.

## Learning

```bash
atlas learn --from-current-run --stage --json
```

Converts current run into staged graph updates.

```bash
atlas accept <proposal-id> --json
```

Applies staged proposal to the committed graph.

Important rule:

```text
Agents may stage graph updates automatically.
Agents must not accept or commit baseline updates without explicit human approval.
```

## Route lookup and reuse

```bash
atlas route <target> --json
```

Returns a route plan, if any, using preferred route plus graph fallback.

```bash
atlas go <target> --json
atlas navigate <target> --json
```

Executes the route plan.

Modes:

```bash
atlas go article-detail --mode safe
atlas go article-detail --mode verified
atlas go article-detail --mode fast
```

Default mode: `verified`.

### Safe mode

```text
- layout before every tap
- resolve selector every time
- layout/check after every tap
- verify destination
- avoid coordinate tap cache unless explicitly allowed
```

### Verified mode

```text
- compact current-screen check
- use known selector or valid tap cache
- verify transition with compact result
- return compact match/check output to agent
- fallback to safe mode if needed
```

### Fast mode

```text
- use cached normalized tap points if device/context guards match
- validate after transition
- fallback to verified/safe if validation fails
```

## Checks

```bash
atlas check --current --json
atlas check article-detail --json
```

Checks are quick validations used during navigation. In agent mode, they should return compact results rather than full layout JSON.

Example output:

```json
{
  "schema_version": "atlas.result.v1",
  "status": "passed",
  "matched_screen": "article-detail",
  "match_confidence": 0.84,
  "checks": [
    {
      "kind": "element_exists",
      "selector": { "kind": "accessibility_or_semantic", "value": "article_body" },
      "status": "passed"
    }
  ],
  "layout_json_returned_to_agent": false
}
```

## Drift and validation

```bash
atlas drift --json
```

Compares the current app state against the committed graph expectation for the current screen/context.

```bash
atlas validate --json
atlas validate --since HEAD~1 --json
atlas validate --changed-files changed-files.txt --json
atlas validate --all --json
```

Validation runs relevant routes/checks and reports divergence from committed navigation memory.

Impact analysis for `--since` and `--changed-files` is heuristic in v1:

```text
- route triggers from path globs
- shared UI/component path globs from config
- manifest/navigation file heuristics
- if shared UI/nav/theme files changed, recommend or run --all
```

Do not overclaim precise cross-flow impact detection in v1.

## Repair

```bash
atlas repair <edge-or-route> --stage --json
```

Proposes selector or route updates after drift. Repair should stage proposals, not silently mutate graph files.

## Mapping

```bash
atlas map --discover --max-actions 50 --stage --json
```

Budgeted app discovery mode.

Rules:

```text
- never default
- always budgeted
- always stages proposals
- does not mutate committed graph silently
```

## Permissions helper

```bash
atlas permissions check --json
atlas permissions grant --json
```

Permissions are useful but not the core product.

---

# 18. JSON result contract

Every agent-facing command should support `--json`.

Results should include:

```text
- schema_version
- status
- summary
- match_confidence / selector_confidence / transition_confidence where applicable
- graph_objects_touched
- compact output by default
- raw_artifact_paths, if any
- recommended_next_command or recommended_action
- whether human approval is required
- metrics such as layout_calls_total and layout_json_returned_to_agent
```

Example:

```json
{
  "schema_version": "atlas.result.v1",
  "status": "found",
  "target": "article-detail",
  "route": "read-article",
  "preferred_path_used": true,
  "graph_fallback_used": false,
  "steps": [
    {
      "from": "home-feed",
      "action": "tap first article card",
      "edge": "edge_home_feed_to_article_detail",
      "selector": { "kind": "accessibility_or_semantic", "value": "article_card" }
    }
  ],
  "recommended_next_command": "atlas go article-detail --mode verified --json",
  "estimated_layout_calls_saved": 3,
  "expected_agent_token_savings_source": "compact_route_plan_instead_of_full_layout_json"
}
```

## Exit codes

Proposed exit codes:

```text
0  success, no relevant changes
1  success, meaningful UI/navigation changes detected
2  check/expectation failure
3  route failed
4  selector drift
5  unknown screen
6  Android CLI/adb/device/environment error
7  Atlas config/schema error
8  context mismatch
```

---

# 19. Divergence and soft validation

Atlas should not simply say “diff changed.” It should classify divergence.

Statuses:

```text
passed
passed_with_changes
changed_requires_review
selector_drift
route_broken
screen_unknown
context_mismatch
system_overlay_blocked
environment_error
```

Classifications:

```text
expected_change
unexpected_change
ambiguous_change
selector_drift
new_screen
missing_screen
new_intermediate_screen
route_broken
unknown_screen
context_mismatch
system_overlay
webview_boundary
```

Examples:

## Route still works, UI changed

```text
Observed:
- article-detail route still reaches target
- new Share button exists

Likely classification:
- expected_change or passed_with_changes

Suggested action:
- no graph update unless Share button should become required check
```

## Route broken

```text
Expected:
- home-feed -> article-detail

Observed:
- home-feed -> blank screen

Likely classification:
- route_broken

Suggested action:
- treat as possible regression
```

## New intermediate screen

```text
Expected:
- home-feed -> article-detail

Observed:
- home-feed -> paywall -> article-detail

Likely classification:
- new_intermediate_screen
- ambiguous until human confirms intentionality

Suggested action:
- stage graph update proposal if intentional
```

## Context mismatch

```text
Expected:
- logged-in home-feed

Observed:
- logged-out sign-in screen

Likely classification:
- context_mismatch

Suggested action:
- establish required auth state or use logged-out route variant
```

## System overlay

```text
Expected:
- app screen after tap

Observed:
- Android runtime permission dialog

Likely classification:
- system_overlay_blocked

Suggested action:
- grant permission, record optional system overlay handler, or update route setup preconditions
```

---

# 20. Self-healing model

Self-healing must mean staged repair, not silent mutation.

Pipeline:

```text
observe mismatch
    ↓
classify divergence
    ↓
propose graph update
    ↓
agent explains expected vs unexpected
    ↓
human or explicit instruction accepts
    ↓
graph updates
    ↓
git diff shows changed navigation expectation
```

Agents may:

```text
- run validation
- record observations
- stage proposals
- explain proposed changes
```

Agents must not automatically:

```text
- accept proposals
- update baselines
- commit .atlas changes
- hide route-breaking divergence
```

---

# 21. Agent skill packaging

The main adoption hurdle is getting agents to use Atlas instead of raw Android CLI layout/tap loops.

## Strategy decision

Atlas should use **multi-write repo-local skill installation** in v1.

Do not rely on the Android skills catalog for Atlas v1. Android CLI can install skills from its catalog, but Atlas should not assume it can contribute to that catalog or that all agents will use it.

`atlas init` should generate identical or adapter-specific `SKILL.md` files into the repo-local locations selected by config/flags.

## Default skill name

Prefer:

```text
atlas-app-navigation
```

over `atlas-android-ui` so the skill name does not permanently lock the product to Android, while its description and body can clearly say that v1 is for Android codebases.

## Repo-local skill paths

Support these paths in v1 where applicable:

```text
Codex current docs:
  .agents/skills/atlas-app-navigation/SKILL.md

Codex team-config/changelog-compatible path:
  .codex/skills/atlas-app-navigation/SKILL.md

Android Studio Agent Mode:
  .skills/atlas-app-navigation/SKILL.md
  .agent/skills/atlas-app-navigation/SKILL.md

Claude Code:
  .claude/skills/atlas-app-navigation/SKILL.md
```

`atlas init --agents auto` should detect existing agent directories and write to those. `atlas init --agents all` may write to all known repo-local paths. `atlas doctor` should report which agents are covered.

## Skill description must be explicit

The skill description must include trigger words like:

```text
Android
Android codebase
layout JSON
android layout
android layout --diff
adb shell input tap
tap
UI
screen
navigate
route
flow
validate
launched app
emulator
Compose
```

## Recommended SKILL.md body

```markdown
---
name: atlas-app-navigation
description: Use when working in an Android codebase and needing to navigate the launched app, inspect Android layout JSON, use android layout, use android layout --diff, tap UI elements, validate screens, learn routes, reuse known navigation, or update the repo’s Atlas graph. Before calling android layout or raw adb tap commands directly, check Atlas first.
metadata:
  author: atlas
  version: "1.0"
---

# Atlas App Navigation Skill

Atlas is this repo's shared navigation memory and soft validation layer for AI agents working in this Android codebase.

Use Atlas whenever you need to navigate, inspect, tap, check, validate, or learn the launched Android app UI.

## Core rule

Before using raw Android layout or adb tap commands:

1. Ask Atlas whether the repo already knows the route.
2. If Atlas knows the route, use Atlas to navigate.
3. If Atlas does not know the route, explore through Atlas wrappers so the run can be learned and shared.
4. Stage learned graph updates, but do not accept or commit them without explicit user approval.

## Navigate to a known screen

```bash
atlas route <target> --json
```

If a route exists:

```bash
atlas go <target> --mode verified --json
atlas check <target> --json
```

## Explore an unknown screen

```bash
atlas observe start <task-name> --json
atlas layout --json
```

The agent reads the compact layout summary, chooses a selector, and calls:

```bash
atlas tap --selector "<best selector>" --reason "<why this tap is needed>" --json
atlas layout --diff --json
atlas check --current --json
```

When the target is reached:

```bash
atlas observe stop --json
atlas learn --from-current-run --stage --json
```

Summarize the proposed graph updates. Do not accept or commit them unless the user approves.

## Validate after code changes

After Android UI changes and after the app is built and launched:

```bash
atlas validate --json
```

Report in this order:

1. Context mismatch or system overlay blockers
2. Broken known routes
3. Failed checks
4. New or changed screens
5. Selector drift
6. Proposed graph updates

Do not run `atlas accept`, `atlas update-baseline`, or commit `.atlas/` changes without explicit user approval.

## Fallback

Only use raw Android CLI layout or adb tap commands if Atlas cannot perform the needed action. If you bypass Atlas, explain why and prefer adding the observed navigation back into Atlas afterward.
```

---

# 22. AGENTS.md guidance

AGENTS.md should stay short. It should not duplicate the whole skill.

Recommended snippet:

```markdown
## Android UI navigation with Atlas

This repo uses Atlas for shared navigation memory and soft validation for AI agents working in this Android codebase.

When working with the launched Android app UI:

1. Use the `atlas-app-navigation` skill.
2. Run `atlas route <target> --json` before exploring manually.
3. Use `atlas go <target> --json` for known screens.
4. If no route exists, use `atlas observe`, `atlas layout`, `atlas tap`, and `atlas learn --stage`.
5. Do not call raw `android layout` or adb tap commands first.
6. Do not accept Atlas graph updates or commit `.atlas/` changes without explicit user approval.
```

---

# 23. Normalization

Atlas should normalize Android layout output into stable graph facts.

Default stripping rules:

```text
- timestamps
- transient focus states
- animation/spinner states
- dynamic counters
- generated IDs matching known dynamic patterns
- text input values
- volatile feed/list item content
- raw bounds, unless converted to guarded normalized bounds
```

Preserve after redaction:

```text
- resource id if stable
- test tag if exposed and stable
- content description class/hash/allowlisted value according to text policy
- role/class
- clickable/enabled state
- parent/child structure
- normalized bounds bucket
- sibling position as fallback
```

Do not preserve verbatim text by default.

---

# 24. Config file

Example `.atlas/config.json`:

```json
{
  "schema_version": "atlas.config.v1",
  "android": {
    "app_package": "com.example.app",
    "assume_app_launched": true,
    "permissions": ["android.permission.POST_NOTIFICATIONS"]
  },
  "context": {
    "auth_state": "unknown",
    "onboarding_state": "unknown",
    "locale": "en-US",
    "orientation": "portrait",
    "feature_flags": {}
  },
  "storage": {
    "commit_raw_layouts": false,
    "runs_dir": ".atlas/runs",
    "state_dir": ".atlas/state",
    "commit_runtime_telemetry": false,
    "generate_index_cache": true,
    "commit_index_cache": false
  },
  "navigation": {
    "default_mode": "verified",
    "safe_mode_fallback": true,
    "screen_match_confidence_min": 0.78,
    "repair_candidate_confidence_min": 0.65,
    "transition_timeout_ms": 3000
  },
  "normalization": {
    "store_normalized_bounds": true,
    "collapse_repeating_lists": true,
    "strip_dynamic_text_inputs": true
  },
  "redaction": {
    "run_before_hashing": true,
    "default_text_action": "exclude",
    "commit_verbatim_text": false,
    "allowlist_static_text": ["Home", "Settings", "Bookmarks"]
  },
  "skills": {
    "skill_name": "atlas-app-navigation",
    "install_strategy": "multi-write-detected",
    "install_paths": [
      ".agents/skills",
      ".codex/skills",
      ".skills",
      ".agent/skills",
      ".claude/skills"
    ]
  }
}
```

---

# 25. Schema versioning and migration

Every committed Atlas object must include `schema_version`.

Rules:

```text
- If Atlas reads a newer major schema version, fail closed with a clear error.
- If Atlas reads an older supported minor schema, it may auto-upgrade in memory.
- Persistent migrations require `atlas migrate` and should show a diff.
- Agents must not run destructive migrations without explicit approval.
```

Command:

```bash
atlas migrate --from atlas.screen.v1 --to atlas.screen.v2 --dry-run --json
atlas migrate --apply --json
```

---

# 26. System dialogs, overlays, and WebView boundaries

## System overlays

Atlas must detect common Android system overlays before classifying app routes as broken:

```text
- runtime permission dialogs
- notification permission prompts
- system app chooser
- biometric prompt
- keyboard overlay when it blocks taps
```

V1 behavior:

```text
- classify as system_overlay_blocked
- offer permission helper or optional overlay handler proposal
- do not mutate app graph silently
```

## WebView boundaries

Many apps embed WebViews. Atlas v1 should not promise full WebView introspection.

V1 behavior:

```text
- detect likely WebView boundary when layout visibility drops or class indicates WebView
- represent it as a special screen/region type
- allow coordinate or visual-annotation fallbacks with low confidence
- flag WebView-heavy routes as fragile
```

---

# 27. Planning deliverable Codex should produce

Codex should create a detailed plan document, ideally:

```text
docs/ATLAS_IMPLEMENTATION_PLAN.md
```

The plan should include:

## A. Current repo assessment

```text
- existing language/framework/package layout
- existing CLI structure, if any
- existing Android CLI integration, if any
- existing skills/plugins, if any
- test infrastructure
- build/test commands
- constraints
```

## B. Architecture

```text
- command layer
- Android CLI adapter layer
- adb executor layer
- observation recorder
- redaction layer
- normalizer
- screen matcher
- graph storage layer
- route engine
- check engine
- context engine
- proposal engine
- skill installer
- JSON result formatter
```

## C. File and module plan

For every new module/file:

```text
- path
- purpose
- public functions/types
- dependencies
- tests
```

## D. CLI command spec

For every command:

```text
- command signature
- inputs
- outputs
- JSON schema
- exit codes
- failure modes
- examples
```

## E. Data schemas

Detailed schemas for:

```text
- config
- graph context
- screen
- edge
- route
- check
- observation run
- proposal
- runtime telemetry
- result
```

## F. Agent skill package

```text
- exact skill paths per agent
- SKILL.md contents
- AGENTS.md snippet
- install/update behavior
- activation strategy
```

## G. Phased implementation milestones

Suggested milestones:

```text
1. CLI skeleton + config + doctor
2. Android CLI/adb adapter commands
3. Observation run capture
4. Redaction + normalization
5. Screen matching
6. Minimal graph storage without committed index
7. Learn/stage/accept proposals
8. Route lookup with graph fallback
9. Go/navigation in verified mode
10. Check/drift/validate
11. Skill installation and AGENTS.md snippet
12. Mapping/discovery mode
```

Each milestone should include:

```text
- scope
- files changed
- test cases
- manual demo
- acceptance criteria
```

## H. Test strategy

Include:

```text
- unit tests for schema parsing/validation
- unit tests for redaction-before-hashing
- unit tests for normalization
- unit tests for screen matching similarity
- unit tests for selector resolution
- unit tests for route graph fallback
- unit tests for context guard mismatch
- unit tests for proposal generation
- golden-file tests for JSON outputs
- mocked Android CLI integration tests
- mocked adb executor tests
- merge-safety tests for parallel screen/edge additions
- end-to-end demo path on a sample Android app
```

## I. Risks and mitigations

Cover at least:

```text
- agents bypassing Atlas
- bad skill activation
- raw layout noise
- graph merge conflicts
- selector drift
- cached coordinate fragility
- device/profile variance
- Compose apps without stable tags
- sensitive data in layout JSON
- false positive divergence
- baseline updates accepted too casually
- system overlays
- WebViews
- app state/context mismatches
- Android CLI preview changes
- public CLI name collision
```

## J. Open questions

Codex should include open questions, but only ask one blocking question at a time during interactive work.

---

# 28. Required acceptance criteria for the implementation plan

The implementation plan is not done unless it explains how to demonstrate this exact loop.

## First agent session

```bash
atlas route article-detail --json
# route not found

atlas observe start article-detail --json
atlas layout --json
# agent reads compact/full first-run layout output and chooses selector
atlas tap --selector "..." --reason "open article detail" --json
atlas check --current --json
atlas observe stop --json
atlas learn --from-current-run --stage --json
atlas accept <proposal-id> --json
```

Expected result:

```text
.atlas/graph contains at least one screen and one edge.
.atlas/routes contains a route to article-detail.
The graph is committed to the repo.
No raw layout JSON is committed.
No volatile timestamps/counters are committed.
```

## Second agent session

```bash
atlas route article-detail --json
atlas go article-detail --mode verified --json
atlas check article-detail --json
```

Expected result:

```text
Atlas reaches article-detail with fewer layout calls and less layout JSON returned to the agent than the first run.
Atlas reports estimated layout calls saved and whether full layout JSON was avoided.
The agent does not rediscover the screen from scratch.
```

## Divergence demo

Change the app so navigation differs.

```bash
atlas validate --json
```

Expected result:

```text
Atlas detects divergence from the committed graph.
Atlas classifies the divergence.
Atlas stages a graph update if the change appears learnable.
Atlas does not silently update the baseline.
```

## Context mismatch demo

Run a logged-in-only route from a logged-out app state.

Expected result:

```text
Atlas returns context_mismatch rather than route_broken.
Atlas recommends establishing the required context or using a logged-out route variant.
```

---

# 29. Metrics

Codex should include instrumentation for:

```text
- layout_calls_total
- layout_calls_saved_estimate
- full_layout_json_returned_to_agent_count
- compact_check_returned_count
- time_to_target_screen_ms
- route_success_rate
- route_reuse_count
- graph_update_proposal_count
- graph_update_acceptance_count
- selector_drift_count
- route_broken_count
- context_mismatch_count
- system_overlay_blocked_count
- false_positive_divergence_count, if measurable
```

Headline metric:

> How much faster and cheaper is the second navigation run than the first?

Supporting metric:

> How often does Atlas avoid returning full layout JSON to the agent after the graph is learned?

---

# 30. Hard product rules

The implementation plan must preserve these rules:

1. Atlas is for AI agents working in Android codebases.
2. The shared graph is committed at the repo level.
3. The graph is both navigation cache and soft validation baseline.
4. Atlas wraps Android CLI and composes with adb for input execution; it does not replace Android CLI.
5. Atlas v1 assumes the app is already built/launched.
6. Raw layout JSON is not committed by default.
7. Redaction runs before hashing/fingerprinting/storage.
8. Verbatim text is excluded from committed graph artifacts by default.
9. Agents may stage graph updates automatically.
10. Agents must not accept baseline updates without explicit human approval.
11. Routes are named preferred paths; graph traversal is the fallback.
12. Screen identity is similarity-based; exact hash is only a fast path.
13. Runtime telemetry/timestamps are gitignored by default.
14. No committed central index in v1.
15. Agent adoption depends on skill activation and command convenience.
16. The second run must be measurably faster/cheaper than the first.

---

# 31. Suggested first implementation order

1. Implement `.atlas/config.json` loading and validation.
2. Implement `atlas init --dry-run` with idempotent file generation.
3. Implement `atlas doctor`.
4. Implement Android CLI adapter with mocked tests.
5. Implement adb executor abstraction with mocked tests.
6. Implement `atlas layout --json` and `atlas layout --diff --json`.
7. Implement observation run storage under `.atlas/runs/`.
8. Implement redaction-before-hashing.
9. Implement normalization of layout output.
10. Implement screen matching similarity.
11. Implement `atlas tap` layout-selector path.
12. Implement `atlas tap` coordinate path.
13. Implement visual annotation fallback path.
14. Implement graph storage for ScreenNode and NavigationEdge without committed index.
15. Implement `atlas learn --stage` proposal generation.
16. Implement `atlas accept`.
17. Implement `atlas route` with preferred path and graph fallback.
18. Implement `atlas go` in verified mode.
19. Implement `atlas check`.
20. Implement `atlas drift` and `atlas validate`.
21. Implement multi-write skill installation.
22. Implement `atlas repair`.
23. Implement `atlas map --discover --max-actions` only after core reuse works.

---

# 32. What Codex should not do in the planning phase

Codex should not:

```text
- start coding immediately
- invent undocumented Android CLI commands
- treat android layout --diff as persistent graph diff
- replace this product with a testing framework
- design only linear flows
- rely on SHA fingerprints as primary screen identity
- commit raw Android layout dumps by default
- commit verbatim text by default
- commit runtime timestamps/counters by default
- make baseline updates automatic
- ignore app state/context
- ignore multi-agent repo sharing and merge conflicts
- treat Atlas as an Android-only agent rather than a tool for AI agents working in Android codebases
- omit skill activation strategy
- omit JSON contracts
- omit tests
```

---

# 33. Reference notes for Codex usage

These notes are for planning how Atlas should integrate with Codex and other agents:

- Codex reads repository `AGENTS.md` files before work and layers project-specific guidance with global guidance.
- Codex skills are directories with `SKILL.md` files. The `SKILL.md` must include `name` and `description`.
- Codex initially sees skill metadata and loads the full `SKILL.md` only when it selects a skill, so the skill description must be precise and trigger-heavy.
- Android skills follow the agent skills open standard. Android Studio Agent Mode supports project-root `.skills/` and `.agent/skills/`.
- Android CLI can install Android skills from its catalog, but Atlas v1 should write its own repo-local skill files directly.
- For complex implementation work, a living plan document can guide multi-hour execution. This brief should be converted into a concrete implementation plan before coding.

---

# 34. Source links for implementation verification

Codex should verify these before implementation because Android CLI and agent skill conventions are moving targets:

```text
Android CLI overview:
https://developer.android.com/tools/agents/android-cli

Android skills overview:
https://developer.android.com/tools/agents/android-skills

Android Studio Agent Mode skills:
https://developer.android.com/studio/gemini/skills

Codex AGENTS.md:
https://developers.openai.com/codex/guides/agents-md

Codex skills:
https://developers.openai.com/codex/skills

Codex customization / skills:
https://developers.openai.com/codex/concepts/customization

Maestro:
https://github.com/mobile-dev-inc/Maestro

Arbigent:
https://github.com/takahirom/arbigent

agent-device:
https://github.com/callstackincubator/agent-device
```

---

# 35. Final planning prompt variant

Use this shorter prompt once the brief is committed:

```text
Use `ATLAS_CODEX_PLANNING_BRIEF.md` as the source of truth. Create `docs/ATLAS_IMPLEMENTATION_PLAN.md` as a detailed living implementation plan. Inspect the repo first. Do not implement yet. The plan must cover CLI, Android CLI/adb adapter, graph context, screen matching, route resolution, graph schemas, repo storage, skill installation, AGENTS.md guidance, staged self-healing proposals, soft validation, privacy/redaction, test strategy, milestones, and acceptance criteria. Preserve the product framing: Atlas is shared navigation memory and soft validation for AI agents working in Android codebases.
```
