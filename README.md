# anycfg

[![Version](https://img.shields.io/badge/version-0.1.0--alpha.0-blue.svg)](https://github.com/jose-pr/anycfg-rs/blob/main/CHANGELOG.md)
[![Rust version](https://img.shields.io/badge/rust-1.89%2B-blue.svg)](https://github.com/jose-pr/anycfg-rs/blob/main/Cargo.toml)
[![Docs](https://img.shields.io/badge/docs-source-blue.svg)](https://github.com/jose-pr/anycfg-rs/blob/main/AGENTS.md)
[![CI](https://img.shields.io/github/actions/workflow/status/jose-pr/anycfg-rs/test.yaml)](https://github.com/jose-pr/anycfg-rs/actions/workflows/test.yaml)

**Configuration discovery, format dispatch, and ordered layering without choosing a parser or value model.**
`anycfg` supplies standard user/system configuration paths, bounded byte and stream
reading, `.cfg` format selection, and a loader. Applications register their own
codecs, define value merge semantics, and decide how results become typed data.
There are **no runtime dependencies** and no built-in document formats.

## Features

- **Pluggable formats** — a `Codec<V>` implementation or `Callbacks::new(decode, encode)` registers a format name, filename extensions, and optional native markers.
- **Application-owned values** — decoders return the application's `V`, with no intermediate JSON-like representation or built-in conversion rules.
- **User-defined merging** — implement `MergeValues<V>` or pass a merge callback. `Merge::Deep`, `Shallow` and `Replace` are policy names passed to that adapter, not hard-coded map algorithms.
- **Typed conversion** — `Loader::load_as` converts a merged `V` to any `T` with a caller callback; optional third-party serialization frameworks belong in the consumer.
- **Standard locations** — `user_config_path`, `system_config_path`, and `default_configs` provide platform defaults and system-before-user layering.
- **Deterministic detection** — an explicit format wins, then a registered extension, then `.cfg`/anonymous shebang or a registered marker, then a caller-selected fallback. No parser guessing or command execution.
- **Resource limits** — bounded per-source and combined byte input; the value adapter validates its own nesting rules.

## Installation

This repository is not yet published to a registry:

```toml
[dependencies]
anycfg = { git = "https://github.com/jose-pr/anycfg-rs" }
```

| Cargo feature | Adds | Default |
| --- | --- | --- |
| None | The core is format-neutral and has no dependencies | Yes |

## Quick start

Register a text codec and load two layers. The same mechanism works with an
application-defined structured value, parser, and merge adapter.

```rust
use anycfg::{Callbacks, CodecError, Formats, Loader, Merge, ReplaceValues, Source};

let mut formats = Formats::<String>::new();
formats.register("plain", ["txt"], Callbacks::new(
    |input: &[u8]| String::from_utf8(input.to_vec()).map_err(|_| CodecError::default()),
    |value: &String| Ok(value.as_bytes().to_vec()),
))?;
formats.set_default("plain")?;

let text = Loader::new(&formats)
    .merge(Merge::Replace)
    .load([Source::text("old"), Source::text("new")], &ReplaceValues)?;
assert_eq!(text, "new");

# Ok::<(), anycfg::Error>(())
```

For a structured format, register its decoder and encoder in exactly the same
way. The decoder chooses `V`; the merge adapter chooses how its mappings,
sequences, scalar values and opaque types behave. Convert the result through
`load_as(sources, adapter, |value| /* Result<T, E> */)`.

Read system and user configuration paths (missing files become `None`):

```rust
use anycfg::{default_configs, Source};

let paths = default_configs("myapp/config.cfg")?;
let sources: Vec<_> = paths.iter()
    .map(|path| path.as_deref().map(Source::path))
    .collect();
// Pass `sources` to Loader::load or Loader::load_or.
assert!(sources.len() == 2);
# Ok::<(), anycfg::Error>(())
```

## API overview

| Module | Purpose |
| --- | --- |
| `anycfg::Formats<V>` | Runtime registry of format codecs and marker/extension selection |
| `anycfg::Codec<V>` | Consumer-owned parse and serialize operations |
| `anycfg::Callbacks<V, D, E>` | Adapter from parse/encode closures to `Codec<V>` |
| `anycfg::Loader<V>` | Load ordered sources, apply budget limits, invoke merge and conversion |
| `anycfg::MergeValues<V>` | Consumer-owned merge and nesting validation |
| `anycfg::Source` | Input as bytes, text, reader or path plus format hints |
| Standard path functions | Discover system and per-user configuration paths |

### Path conventions

| Platform | System base | User base |
| --- | --- | --- |
| Windows | `%PROGRAMDATA%` or `C:\\ProgramData` | `%APPDATA%` or `~/AppData/Roaming` |
| Linux / Unix | `/etc` | absolute `$XDG_CONFIG_HOME` or `~/.config` |
| macOS | `/etc` | absolute `$XDG_CONFIG_HOME` or `~/.config` |

`default_configs("app.cfg")` yields `[system, user]`, retaining `None` for
missing or non-file paths. It accepts nested relative filenames and rejects
absolute paths and `..` traversal. It does not create directories. A loader
can receive the resulting optional sources directly; `load_or` returns the
consumer-provided value when all sources are absent.

### Format selection

Only registered codecs are understood. `.cfg` is a generic carrier: if it
starts with `#!<registered-name>` (including a path-like shebang) the selected
header is removed. Without a header, it uses the registry default. Other
registered filename extensions select their codec directly, without sniffing.
A native format marker can be registered explicitly with `register_marker`;
that marker is **not** stripped, because only the owning codec knows its syntax.
A forced format does not strip shebang lines.

### Value contract

No assumptions are made about the value's representation. For applications
which need deep merging, the `MergeValues<V>` implementation should walk its
own structural model and enforce `max_depth`. The default `validate` method
is intentionally a no-op; a caller that handles recursive untrusted values
must implement it. Callback mergers can choose alternative semantics. A parser
is responsible for its own input recursion and expansion limits.

### Security

Sources are never interpreted as code, commands, URLs, includes, or environment
substitutions by this crate. A registered codec is explicitly trusted to parse
and serialize its format. The core never includes input excerpts or caller
conversion errors in its diagnostic messages; codecs supply only optional
line and column numbers. A reader may consume one byte past the permitted
limit to recognize overflow.

## Development

Rust 1.89 or later:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo package --list --allow-dirty
```

The packaged `AGENTS.md` describes the exported API. CI is available from
workflow dispatch or a temporary `ci-*` tag; no release tag is created by
normal development.

### Releasing

Releases require an explicit license decision and passing Rust checks. The
manifest currently specifies `publish = false`. No release tag or registry
publication is configured.
