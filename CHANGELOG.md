# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Read TOML, JSON and YAML from byte slices, strings, readers and files, using a
  deterministic `.cfg` shebang convention with TOML fallback.
- Deserialize ordered configuration sources through `Loader` with deep,
  shallow and replace merge strategies. Later sources override earlier ones.
- Enforce per-source, combined-source and merge-depth limits.
- Serialize native documents and self-describing `.cfg` documents.

### Fixed

- Reject documents deeper than 64 nested mapping/array levels even when a
  source is used alone or with shallow/replacement merging.
- Stop streaming reads at the remaining combined-byte budget plus one
  overflow-detection byte, instead of reading the full source allowance first.
- Enforce the byte limit on standalone format detection.

[Unreleased]: https://github.com/jose-pr/anycfg-rs/commits/main/
