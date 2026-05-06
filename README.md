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
