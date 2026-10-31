# anycfg

[![Version](https://img.shields.io/badge/version-0.1.0--alpha.0-blue.svg)](https://github.com/jose-pr/anycfg-rs/blob/main/CHANGELOG.md)
[![Rust version](https://img.shields.io/badge/rust-1.89%2B-blue.svg)](https://github.com/jose-pr/anycfg-rs/blob/main/Cargo.toml)
[![Docs](https://img.shields.io/badge/docs-source-blue.svg)](https://github.com/jose-pr/anycfg-rs/blob/main/AGENTS.md)
[![CI](https://img.shields.io/github/actions/workflow/status/jose-pr/anycfg-rs/test.yaml)](https://github.com/jose-pr/anycfg-rs/actions/workflows/test.yaml)

**Configuration discovery, format dispatch, and ordered layering without choosing a parser or value model.**
`anycfg` supplies standard user/system configuration paths, bounded byte and stream
reading, `.cfg` format selection, and a loader. Applications register their own
codecs, define value merge semantics, and decide how results become typed data.
The only runtime dependency is **Serde**; there are no built-in document formats.

## Features

- **Pluggable formats** — a `Codec<V>` implementation or `Callbacks::new(decode, encode)` registers a format name, filename extensions, and optional native markers.
- **Application-owned values** — decoders return the application's `V`, with no intermediate JSON-like representation or built-in conversion rules.
- **User-defined merging** — implement `MergeValues<V>` or pass a merge callback. `Merge::Deep`, `Shallow` and `Replace` are policy names passed to that adapter, not hard-coded map algorithms.
- **Serde-typed values** — `Formats::from_slice_typed`, `Loader::load_typed`, and `Formats::to_vec_typed` work with `DeserializeOwned`/`Serialize` types through an application-chosen Serde value model. `load_as` remains available for custom conversion.
- **Standard locations** — `user_config_path`, `system_config_path`, and `default_configs` provide platform defaults and system-before-user layering.
- **Deterministic detection** — an explicit format wins, then a registered extension, then `.cfg`/anonymous shebang or a registered marker, then a caller-selected fallback. No parser guessing or command execution.
- **Resource limits** — bounded per-source and combined byte input; the value adapter validates its own nesting rules.

## Installation

This repository is not yet published to a registry:

```toml
[dependencies]
anycfg = { git = "https://github.com/jose-pr/anycfg-rs" }
serde = { version = "1", features = ["derive"] }

# Only add the format backends your application uses:
serde_json = "1"
toml = "0.9"
serde-saphyr = "2"
```

| Cargo feature | Adds | Default |
| --- | --- | --- |
| None | Serde is mandatory; JSON, TOML and YAML remain application-selected | Yes |

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

### Typed Serde configuration

The crate depends on `serde` but not on any particular parser. Add the format
implementations your application chooses. For instance, a JSON-only application
can register `serde_json` as a codec and deserialize into an ordinary Rust struct:

```rust
use anycfg::{Callbacks, CodecError, Formats, Options};
use serde::{Deserialize, Serialize};
use serde_json::Value;

# fn main() -> Result<(), anycfg::Error> {
#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Server { host: String, port: u16 }

let mut formats = Formats::<Value>::new();
formats.register("json", ["json"], Callbacks::new(
    |bytes: &[u8]| serde_json::from_slice::<Value>(bytes)
        .map_err(|_| CodecError::default()),
    |value: &Value| serde_json::to_vec(value)
        .map_err(|_| CodecError::default()),
))?;
formats.set_default("json")?;

let config: Server = formats.from_slice_typed(
    br#"{"host":"localhost","port":8080}"#,
    Options::new().name("app.json"),
)?;
let encoded = formats.to_vec_typed(&config, "json", serde_json::value::Serializer)?;
let decoded: Server = formats.from_slice_typed(&encoded, Options::new().format("json"))?;
assert_eq!(config, decoded);
# Ok(())
# }
```

For **TOML, YAML and JSON in a single registry**, see the runnable
[Serde formats example](https://github.com/jose-pr/anycfg-rs/blob/main/examples/serde_formats.rs).
It registers all three backends, merges TOML defaults with YAML and JSON
layers, and calls `Loader::load_typed` with a Serde-derived result type. Only the example and tests depend on `toml`, `serde-saphyr`, and
`serde_json`; the library requires none of them. Its use of
`serde_json::Value` is an example choice, not a core requirement. More
specialized values can preserve different datetime, number and tag semantics.

For a value model that is not a Serde deserializer, use
`load_as(sources, adapter, |value| /* Result<T, E> */)` to convert the value.
A Serde-aware value model must implement `serde::Deserializer` to use typed
reads. For typed writes, the caller supplies a `serde::Serializer` producing
that same value type; this avoids selecting a universal intermediate tree.

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
| Serde typed methods | Decode `V` into `DeserializeOwned` targets; serialize `Serialize` values via a caller-selected Serde serializer |
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
