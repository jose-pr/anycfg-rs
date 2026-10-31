# anycfg

[![Version](https://img.shields.io/badge/version-0.1.0--alpha.0-blue.svg)](https://github.com/jose-pr/anycfg-rs/blob/main/CHANGELOG.md)
[![Rust version](https://img.shields.io/badge/rust-1.89%2B-blue.svg)](https://github.com/jose-pr/anycfg-rs/blob/main/Cargo.toml)
[![Docs](https://img.shields.io/badge/docs-draft-blue.svg)](https://github.com/jose-pr/anycfg-rs/blob/main/AGENTS.md)
[![CI](https://img.shields.io/github/actions/workflow/status/jose-pr/anycfg-rs/test.yaml)](https://github.com/jose-pr/anycfg-rs/actions/workflows/test.yaml)

**Read and merge TOML, JSON and YAML configuration through Serde, from bytes,
streams or files.** The `.cfg` format defaults to TOML and can name its parser
with a first-line shebang. Sources are ordered, independently decoded, and
combined before deserializing into an application type. See the
[API header](https://github.com/jose-pr/anycfg-rs/blob/main/AGENTS.md).

**Status:** unpublished source draft. The project has not yet been compiled
against the Rust toolchain; the version and CI badges describe the prepared
sources, not a published crate or verified CI run.

## Features

- **Ordered sources** — load bytes, text, `std::io::Read` and local paths in one call, with later layers overriding earlier ones.
- **Three strategies** — `Replace`, `Shallow` and `Deep` (the default), with arrays and scalar values replaced rather than appended.
- **Format detection** — a `.cfg` document uses `#!json`, `#!yaml`, `#!toml` or a `%YAML` directive; without a marker it is TOML. `.json`, `.yaml`, `.yml` and `.toml` select their format by name.
- **Serde** — decode one source directly into any `Deserialize` type, or merge several sources and deserialize once into a `DeserializeOwned` type.
- **Bounded reads** — 8 MiB per source by default, 64 MiB across a loader; no implicit file discovery, commands, templates or includes.
- **Serialization** — generate plain format text, or `.cfg` text tagged for JSON and YAML.

## Installation

Until the package is released, add it from the checkout:

```toml
[dependencies]
anycfg = { path = "../anycfg-rs" }
serde = { version = "1", features = ["derive"] }
```

There are no optional Cargo features in this version. All three formats are available with the default build.

## Quick start

One source, with no filesystem dependency:

```rust
use anycfg::from_slice;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config { port: u16 }

let config: Config = from_slice(b"port = 8080")?;
assert_eq!(config.port, 8080);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Merge TOML, JSON and YAML, then deserialize once:

```rust
use anycfg::{Loader, Source};
use serde::Deserialize;
use std::io::Cursor;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config { host: String, port: u16, debug: bool }

let base = b"host = 'localhost'\nport = 8080\ndebug = false\n";
let runtime = Cursor::new(b"#!json\n{\"port\": 9090}".as_slice());
let overrides = "debug: true\n";

let config: Config = Loader::new().load([
    Source::bytes(base).name("base.cfg"),
    Source::reader(runtime).name("runtime.cfg"),
    Source::text(overrides).name("override.yaml"),
])?;
assert_eq!((config.host.as_str(), config.port, config.debug), ("localhost", 9090, true));
# Ok::<(), Box<dyn std::error::Error>>(())
```

For nested tables, `Deep` merges only mappings recursively. A later array,
scalar, or `null` replaces the earlier value. Missing keys inherit earlier
values. `Shallow` replaces entire top-level sections. `Replace` discards each
preceding document. No strategy interprets `null` as a delete sentinel.

## API overview

| Module | Purpose |
| --- | --- |
| `anycfg` | `Format`, `Options`, `Detection`, `detect`, direct Serde loads, serialization and safe errors |
| `anycfg::Loader` | Ordered multi-source merging into `DeserializeOwned` types |
| `anycfg::Source` | In-memory bytes/text, synchronous readers and filesystem paths with per-source format hints |
| `anycfg::Merge` | Replace, shallow merge and deep merge |

`Loader` currently merges through `serde_json::Value`. That supports ordinary
string-keyed configuration tables, lists and scalars, but it is **not** a
lossless intermediate for every TOML datetime, YAML-specific value or
special-number representation. Keep Guatiao's `guatiao-serde` for those specialized
values. More general value preservation and `DeserializeSeed` are separate API
work, not hidden guarantees in this loader.

## Development

Rust 1.89 or newer:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo package --list --allow-dirty
```

The package inventory must contain `AGENTS.md` and `README.md`, and no `.agents/`
or `*.local.*` files. The release is gated until a license is selected and the
build and tests have passed on supported platforms.

### Releasing

Follow [Semantic Versioning](https://semver.org/) and the
[Changelog](https://github.com/jose-pr/anycfg-rs/blob/main/CHANGELOG.md).
A release requires an explicit decision to publish, a passing CI run and a
matching version tag. No release or publication has been made.
