# Releasing Minimap

Minimap uses Apache-2.0 and ships from tagged GitHub releases. The GitHub
release is the source of truth for binary assets. crates.io and Homebrew should
follow the same version tag.

## Release Channels

- GitHub Releases: macOS, Linux, and Windows archives built by `.github/workflows/release.yml`.
- crates.io: Rust users install with `cargo install minimap-cli`.
- Homebrew tap: macOS and Linux users install with `brew install himattm/minimap/minimap`.

Scoop, winget, Nix, AUR, Debian, and RPM packages can be added later once there
is demand.

## Prerequisites

- A clean `main` branch.
- A semver version in `Cargo.toml` and all internal crate dependency versions.
- A crates.io token saved as the repository secret `CARGO_REGISTRY_TOKEN`.
- A Homebrew tap repository named `himattm/homebrew-minimap`.

## Local Verification

Run these checks before tagging:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build -p minimap-cli --bin minimap
cargo package --workspace
```

For a smoke test from the built binary:

```bash
target/debug/minimap --help
target/debug/minimap init --dry-run --agents all
```

## crates.io

Publishing to crates.io is manual because published versions are permanent and
cannot be overwritten.

The `Publish crates` GitHub Actions workflow publishes packages in dependency
order. Run it with `dry_run=true` first, then rerun with `dry_run=false`.

Manual equivalent:

```bash
cargo publish -p minimap-schemas --dry-run
cargo publish -p minimap-core --dry-run
cargo publish -p minimap-android --dry-run
cargo publish -p minimap-repo --dry-run
cargo publish -p minimap-graph --dry-run
cargo publish -p minimap-cli --dry-run
```

Then publish for real in the same order, waiting for each crate to appear in the
registry index before publishing dependents:

```bash
cargo publish -p minimap-schemas
cargo publish -p minimap-core
cargo publish -p minimap-android
cargo publish -p minimap-repo
cargo publish -p minimap-graph
cargo publish -p minimap-cli
```

## GitHub Release

Create and push a signed or annotated tag from the commit that was published to
crates.io:

```bash
git tag -a v0.1.0 -m "Minimap v0.1.0"
git push origin v0.1.0
```

The release workflow creates archives and `.sha256` files for each supported
target.

## Homebrew Tap

Create the tap once:

```bash
brew tap-new himattm/homebrew-minimap
gh repo create himattm/homebrew-minimap --public --source "$(brew --repository himattm/homebrew-minimap)" --push
```

For each release:

1. Copy `packaging/homebrew/Formula/minimap.rb.template` to the tap as
   `Formula/minimap.rb`.
2. Replace `__VERSION__` with the release version without the leading `v`.
3. Replace `__SOURCE_SHA256__` with the SHA-256 of the GitHub source archive.

Get the source archive checksum:

```bash
curl -L https://github.com/himattm/minimap/archive/refs/tags/v0.1.0.tar.gz | shasum -a 256
```

Test the formula locally from the tap:

```bash
brew install --build-from-source himattm/minimap/minimap
brew test himattm/minimap/minimap
brew audit --strict --online himattm/minimap/minimap
```

Then commit and push the formula in `himattm/homebrew-minimap`.

Users install with:

```bash
brew install himattm/minimap/minimap
```

## Version Bump Checklist

For the next release after `0.1.0`:

1. Update `[workspace.package].version` in `Cargo.toml`.
2. Update internal dependency versions in each `crates/minimap-*/Cargo.toml`.
3. Update release notes in `CHANGELOG.md`.
4. Run local verification.
5. Publish crates.
6. Push the release tag.
7. Update the Homebrew tap.
