# Minimap

Give AI agents a map of your Android app.

Minimap is shared navigation memory and soft validation for AI agents working in
Android codebases. It wraps documented Android CLI and `adb` primitives, records
navigation runs, stores distilled graph artifacts under `.minimap/`, and lets later
agents reuse known routes instead of rediscovering the UI.

## Install

With Homebrew, after the tap is published:

```bash
brew install himattm/minimap/minimap
```

With Cargo, after the crates are published:

```bash
cargo install minimap-cli
```

From source:

```bash
cargo install --git https://github.com/himattm/minimap minimap-cli
```

From a checkout:

```bash
cargo build -p minimap-cli --bin minimap
```

Release binaries are published from GitHub releases for macOS, Linux, and
Windows. See [docs/RELEASING.md](docs/RELEASING.md) for maintainer release
steps.

## Basic Workflow

Initialize a repo:

```bash
minimap init --agents all
minimap doctor
```

Explore an unknown route:

```bash
minimap observe start article-detail
minimap layout
minimap tap --selector "text=Open" --reason "open article detail"
minimap layout
minimap observe stop
minimap learn --from-current-run --stage
```

Review the staged proposal, then explicitly accept it:

```bash
minimap accept <proposal-id>
```

Reuse and validate:

```bash
minimap route article-detail --current-screen home
minimap go article-detail --current-screen home
minimap check --current
minimap drift
minimap validate --all
minimap validate --all --execute --current-screen home
```

## First-Run Agent Mapping

`minimap init --agents all` installs two repo-local skills for supported agents:
`minimap-app-navigation` for normal route reuse and `minimap-first-run-mapping` for
initial discovery.

First-run mapping is token-intensive: the agent has to inspect Android layout
JSON, decide what to tap, navigate the launched app, and record routes before
Minimap can reuse the graph. Keep the first pass bounded to a few important flows.
Use the first-run skill once for an initial app map, or later only for a bounded
reason such as a new feature area, a major UI redesign, a new auth/onboarding
context, or explicit additional route coverage.

Example prompt:

```text
Use the minimap-first-run-mapping skill to perform first-run mapping for this
launched Android app. Start with the settings, profile, and article detail flows.
Warn me before any especially broad exploration. Stage learned Minimap proposals,
but do not accept or commit them until I approve.
```

The agent should use this loop for each route:

```bash
minimap map --discover <route-name> --max-actions 5 --stage
minimap layout
minimap tap --selector "<kind>=<value>" --reason "<navigation reason>"
minimap layout
minimap map --discover <route-name> --max-actions 5 --stage --finish
```

Review staged proposals before accepting:

```bash
minimap accept <proposal-id>
minimap validate --all
```

## Claude Code Plugin

Claude Code users can install the same Minimap skills from this repo's plugin
marketplace.

From Claude Code, add the marketplace:

```text
/plugin marketplace add himattm/minimap
```

Then install the plugin:

```text
/plugin install minimap@minimap
```

For local development from a checkout:

```text
/plugin marketplace add .
/plugin install minimap@minimap
```

The plugin includes:

- `minimap-app-navigation` for normal Minimap route reuse and validation.
- `minimap-first-run-mapping` for bounded, token-intensive initial app mapping.

## Product Rules

- Raw Android layout JSON is not committed by default.
- Redaction runs before hashing, normalization, or graph proposal generation.
- Runtime data stays under `.minimap/runs/` and `.minimap/state/`, which are
  gitignored by `minimap init`.
- Minimap may stage graph updates automatically, but committed graph changes only
  happen after `minimap accept`.
- `android layout --diff` remains an Android in-session diff. Minimap graph drift
  is reported by `minimap drift` and `minimap validate`.

## Live Device Smoke

With a built and launched Android app plus `android` and `adb` on `PATH`:

```bash
minimap doctor
minimap layout
minimap tap --selector "text=Settings" --reason "open settings"
minimap check --current
```

For a known route already committed under `.minimap/`:

```bash
minimap go <route-or-screen> --current-screen <screen-name>
minimap validate --all
```

Live device tests are intentionally separate from CI. CI uses fake `android` and
`adb` executables for deterministic command-contract coverage.
