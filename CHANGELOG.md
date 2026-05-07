# Changelog

All notable changes to Minimap are documented here.

## 0.1.2 - 2026-05-07

### Changed

- Sharpened `minimap --help` output: every subcommand (and `observe start`/`observe stop`) now ships an instructive `about` string covering required flags, side effects, and which command mutates the committed graph.

### Documentation

- `minimap-app-navigation` and `minimap-first-run-mapping` skills now spell out that Claude Code plugins cannot install binaries and document the brew/cargo/source install paths to fall back on.
- Bumped Claude Code plugin and marketplace metadata to match the release.

## 0.1.0 - 2026-05-06

### Added

- Rust `minimap` CLI for Android route recording, reuse, drift checks, and validation.
- Repo-committed `.minimap/` graph artifacts with ignored runtime state and run data.
- Bounded first-run mapping workflow for agent-driven Android UI discovery.
- Repo-local skills for normal route navigation and token-intensive first-run mapping.
- Claude Code plugin marketplace metadata for installing Minimap skills.
- GitHub release workflow for macOS, Linux, and Windows binaries.
- crates.io publishing workflow and Homebrew tap formula template.
