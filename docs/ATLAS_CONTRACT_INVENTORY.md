# Atlas CLI Contract Inventory

This document freezes the current Python CLI as the migration oracle for the
Rust rewrite. Rust must preserve these contracts unless a migration note
explicitly approves a change.

| Command | Python status | Rust migration status | Writes files | Android CLI | adb |
|---|---:|---:|---:|---:|---:|
| `atlas init` | implemented | preview in `atlas-rs` | yes | no | no |
| `atlas doctor` | implemented | preview in `atlas-rs` | no | path check only | path check only |
| `atlas layout` | implemented | pending | run artifacts when observing | yes | no |
| `atlas layout --diff` | implemented | pending | run artifacts when observing | yes | no |
| `atlas tap --selector` | implemented | pending | run artifacts when observing | yes | yes |
| `atlas tap --point` | implemented | pending | run artifacts when observing | no | yes |
| `atlas tap --label` | implemented | pending | run artifacts when observing | yes | yes |
| `atlas observe start` | implemented | pending | yes, gitignored | no | no |
| `atlas observe stop` | implemented | pending | yes, gitignored | no | no |
| `atlas learn --from-current-run --stage` | implemented as review proposal | pending | proposals | no | no |
| `atlas accept` | implemented | preview in `atlas-rs` | graph artifacts | no | no |
| `atlas route` | implemented | preview in `atlas-rs` | no | no | no |
| `atlas go` | implemented skeleton | pending | run artifacts when observing | yes | yes |
| `atlas check` | implemented | pending | no | yes for current | no |
| `atlas validate` | not implemented | pending | proposals/state | yes | maybe |
| `atlas drift` | not implemented | pending | proposals | yes | no |
| `atlas repair` | not implemented | pending | proposals | maybe | maybe |
| `atlas map --discover` | not implemented | pending | proposals/runs | yes | yes |

## Stable Exit Codes

The Python implementation currently uses:

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
codes, but Rust preview follows the Python mapping until a deliberate contract
change is approved.

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

The first fixture set should cover `init --dry-run`, `route` success,
`route` context mismatch, and `accept`.
