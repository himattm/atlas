# Minimap

Give AI agents a map of your Android app.

Minimap is shared navigation memory and soft validation for AI agents working in
Android codebases. It wraps documented [`android` CLI](https://developer.android.com/tools/agents/android-cli) and [`adb`](https://developer.android.com/tools/adb) primitives, records
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

`minimap init` creates an empty graph under `.minimap/`. That is the expected
starting state — Minimap is useful immediately, and the graph fills in as you
naturally use the app. There is no required baseline survey.

### Grow the graph one screen at a time

While you are using the app, record the route you just took:

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

Repeat for the next screen the next time you (or an agent) navigate somewhere
worth remembering. The graph grows over time without a separate setup phase.

### Reuse and validate

Once a route is in the graph, reuse and validate it:

```bash
minimap route article-detail --current-screen home
minimap go article-detail --current-screen home
minimap check --current
minimap drift
minimap validate --all
minimap validate --all --execute --current-screen home
```

## First-Run Agent Mapping (optional)

`minimap init --agents all` installs two repo-local skills for supported agents:
`minimap-app-navigation` for everyday navigation and incremental graph growth, and
`minimap-first-run-mapping` for optional bulk surveys.

Most users will not need first-run mapping. The incremental flow above grows the
graph naturally over time, one screen per session, as the app is actually used.

If you want a quick bulk pass over many flows at once — for example, seeding a
brand-new repo with coverage of the settings, profile, and article-detail flows
in a single sustained run — the `minimap-first-run-mapping` skill exists. It is
token-intensive: the agent has to inspect Android layout JSON, decide what to
tap, navigate the launched app, and record many routes in one sitting. Invoke it
only when you explicitly want that bulk pass, or for a bounded reason such as a
major UI redesign, a new auth/onboarding context, or explicit additional route
coverage across multiple flows.

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

- `minimap-app-navigation` for everyday Minimap navigation, incremental graph
  growth one screen at a time, route reuse, and validation.
- `minimap-first-run-mapping` for optional bounded bulk surveys when the user
  explicitly asks to map many flows at once.

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
