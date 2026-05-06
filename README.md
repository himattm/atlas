# Atlas

Give AI agents a map of your Android app.

Atlas is shared navigation memory and soft validation for AI agents working in
Android codebases. It wraps documented Android CLI and `adb` primitives, records
navigation runs, stores distilled graph artifacts under `.atlas/`, and lets later
agents reuse known routes instead of rediscovering the UI.

## Install

From source:

```bash
cargo install --git https://github.com/himattm/atlas atlas-cli
```

From a checkout:

```bash
cargo build -p atlas-cli --bin atlas
```

Release binaries are published from GitHub releases for macOS and Linux.

## Basic Workflow

Initialize a repo:

```bash
atlas init --agents all
atlas doctor
```

Explore an unknown route:

```bash
atlas observe start article-detail
atlas layout
atlas tap --selector "text=Open" --reason "open article detail"
atlas layout
atlas observe stop
atlas learn --from-current-run --stage
```

Review the staged proposal, then explicitly accept it:

```bash
atlas accept <proposal-id>
```

Reuse and validate:

```bash
atlas route article-detail --current-screen home
atlas go article-detail --current-screen home
atlas check --current
atlas drift
atlas validate --all
```

## First-Run Agent Mapping

`atlas init --agents all` installs two repo-local skills for supported agents:
`atlas-app-navigation` for normal route reuse and `atlas-first-run-mapping` for
initial discovery.

First-run mapping is token-intensive: the agent has to inspect Android layout
JSON, decide what to tap, navigate the launched app, and record routes before
Atlas can reuse the graph. Keep the first pass bounded to a few important flows.
Use the first-run skill once for an initial app map, or later only for a bounded
reason such as a new feature area, a major UI redesign, a new auth/onboarding
context, or explicit additional route coverage.

Example prompt:

```text
Use the atlas-first-run-mapping skill to perform first-run mapping for this
launched Android app. Start with the settings, profile, and article detail flows.
Warn me before any especially broad exploration. Stage learned Atlas proposals,
but do not accept or commit them until I approve.
```

The agent should use this loop for each route:

```bash
atlas observe start <route-name>
atlas layout
atlas tap --selector "<kind>=<value>" --reason "<navigation reason>"
atlas layout
atlas observe stop
atlas learn --from-current-run --stage
```

Review staged proposals before accepting:

```bash
atlas accept <proposal-id>
atlas validate --all
```

## Product Rules

- Raw Android layout JSON is not committed by default.
- Redaction runs before hashing, normalization, or graph proposal generation.
- Runtime data stays under `.atlas/runs/` and `.atlas/state/`, which are
  gitignored by `atlas init`.
- Atlas may stage graph updates automatically, but committed graph changes only
  happen after `atlas accept`.
- `android layout --diff` remains an Android in-session diff. Atlas graph drift
  is reported by `atlas drift` and `atlas validate`.

## Live Device Smoke

With a built and launched Android app plus `android` and `adb` on `PATH`:

```bash
atlas doctor
atlas layout
atlas tap --selector "text=Settings" --reason "open settings"
atlas check --current
```

For a known route already committed under `.atlas/`:

```bash
atlas go <route-or-screen> --current-screen <screen-name>
atlas validate --all
```

Live device tests are intentionally separate from CI. CI uses fake `android` and
`adb` executables for deterministic command-contract coverage.
