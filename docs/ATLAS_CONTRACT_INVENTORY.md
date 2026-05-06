# Atlas CLI Contract Inventory

This document records the contract that was ported from the Python reference
implementation before the Python package was removed. Rust must preserve these
contracts unless a migration note explicitly approves a change.

| Command | Former Python status | Rust status | Writes files | Android CLI | adb |
|---|---:|---:|---:|---:|---:|
| `atlas init` | implemented | implemented | yes | no | no |
| `atlas doctor` | implemented | implemented | no | path check only | path check only |
| `atlas layout` | implemented | implemented | run artifacts when observing | yes | no |
| `atlas layout --diff` | implemented | implemented | run artifacts when observing | yes | no |
| `atlas tap --selector` | implemented | implemented | run artifacts when observing | yes | yes |
| `atlas tap --point` | implemented | implemented | run artifacts when observing | no | yes |
| `atlas tap --label` | implemented | implemented | run artifacts when observing | yes | yes |
| `atlas observe start` | implemented | implemented | yes, gitignored | no | no |
| `atlas observe stop` | implemented | implemented | yes, gitignored | no | no |
| `atlas learn --from-current-run --stage` | implemented as review proposal | implemented | proposals | no | no |
| `atlas accept` | implemented | implemented | graph artifacts | no | no |
| `atlas route` | implemented | implemented | no | no | no |
| `atlas go` | implemented skeleton | implemented edge tap execution | run artifacts when observing | yes | yes |
| `atlas check` | implemented | implemented with current-screen matching | no | yes for current | no |
| `atlas validate` | not implemented | pending | proposals/state | yes | maybe |
| `atlas drift` | not implemented | pending | proposals | yes | no |
| `atlas repair` | not implemented | pending | proposals | maybe | maybe |
| `atlas map --discover` | not implemented | pending | proposals/runs | yes | yes |

## Stable Exit Codes

The Rust implementation preserves this mapping:

```text
0  success
1  meaningful change or doctor failure
2  check/invariant failure or generic command failure
3  route failed
4  selector drift
5  unknown screen / not found
6  Android CLI/device/environment error
7  Atlas config/schema error
8  context mismatch
```

The migration plan originally listed privacy and unsupported-schema-specific
codes, but Rust follows the existing mapping until a deliberate contract change
is approved.

## Golden Fixture Policy

Golden fixtures should capture:

```text
command
repo fixture input
stdout JSON
stderr
exit code
expected file writes
volatile fields ignored during comparison
```

The Rust CLI fixture set covers `init --dry-run`, route context mismatch,
`observe`/`learn`, `layout --diff`, `tap --selector`, `go` edge execution, and
`check --current` screen matching with fake Android/adb executables.
