# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- Replace the fixed format enum and hard-coded parser dependencies with a
  registered `Codec<V>` interface, callback adapters and configurable format
  names, suffixes, native markers and fallback selection.
- Decode directly into the consumer's value model, with no intermediate
  format-specific or JSON-compatible representation.
- Delegate `Replace`, `Shallow`, `Deep` merging and structural depth
  validation to a `MergeValues<V>` implementation or merge callback.
- Convert the final merged value to an application type through `load_as`.

### Added

- Require an explicit default format in `Loader::new(&formats, default_format)`;
  registered file extensions and content markers can still take precedence.
- Automatic format selection registered per extension; `.cfg` is enabled by
  default and inherits the loader's default format when no header matches.
- `Formats::register_auto` to pin an extension-specific default and
  `Formats::register_extension` to configure fixed formats.
- `Loader::default_format` and `Options::default_format` for unnamed streams,
  byte slices, and automatically detected extensions without a pinned default.
- General-purpose `to_auto_vec`/`to_auto_vec_typed` writers for registered
  auto-detect extensions.
- `user_config_path`, `system_config_path` and `default_configs` using standard
  platform directories. `default_configs` yields system then user paths, with
  missing files represented by `None`.
- Bounded parsing and reading from byte slices, strings, streams and paths.
- Optional source layers and `load_or` for an explicit missing-file fallback.

[Unreleased]: https://github.com/jose-pr/anycfg-rs/commits/main/
