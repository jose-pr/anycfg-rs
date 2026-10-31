# `anycfg` — API header

Header-file-style reference for the `anycfg` crate: the public types,
functions, arguments, defaults and constraints, so the crate can be used without
reading its source. This file lives beside `Cargo.toml` and `README.md` and
ships in the Cargo package. Source: <https://github.com/jose-pr/anycfg-rs>.

- Rust 1.89+, edition 2024. Default build includes TOML, JSON and YAML.
- No optional features. The crate imports as `anycfg`.
- Everything listed here is re-exported from the crate root; `loader` is a private implementation module.
- No unsafe code. No environment variables read. No global registry or runtime.

## Formats and direct reads (`anycfg`)

```rust
pub enum Format { Toml, Json, Yaml }
impl Format {
    pub const fn name(self) -> &'static str;
    pub fn parse(name: &str) -> Result<Self, Error>;
}

pub struct Options<'a> { pub format: Option<Format>, pub name: Option<&'a str>, pub max_bytes: usize }
impl<'a> Options<'a> {
    pub fn new() -> Self;
    pub fn name(self, name: &'a str) -> Self;
    pub fn format(self, format: Format) -> Self;
    pub fn max_bytes(self, max_bytes: usize) -> Self;
}

pub fn detect<'de>(bytes: &'de [u8], options: Options<'_>) -> Result<Detection<'de>, Error>;
pub struct Detection<'de> { pub format: Format, pub payload: &'de [u8] }

pub fn from_slice<'de, T: serde::Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, Error>;
pub fn from_slice_with<'de, T: serde::Deserialize<'de>>(bytes: &'de [u8], options: Options<'_>) -> Result<T, Error>;
pub fn from_str<'de, T: serde::Deserialize<'de>>(text: &'de str) -> Result<T, Error>;
pub fn from_str_with<'de, T: serde::Deserialize<'de>>(text: &'de str, options: Options<'_>) -> Result<T, Error>;
pub fn from_reader<T: serde::de::DeserializeOwned>(reader: impl std::io::Read) -> Result<T, Error>;
pub fn from_reader_with<T: serde::de::DeserializeOwned>(reader: impl std::io::Read, options: Options<'_>) -> Result<T, Error>;
pub fn from_path<T: serde::de::DeserializeOwned>(path: impl AsRef<std::path::Path>) -> Result<T, Error>;
```

`Options::default()` is `format = None`, `name = None`, `max_bytes = 8 MiB`.
Format selection: explicit `format` wins; recognized non-`.cfg` suffix wins
otherwise; a `.cfg` or anonymous source uses a recognized `#!` first line or
`%YAML` marker; otherwise TOML. A recognized shebang is stripped only when
selected by the header. Invalid extensions and unknown shebang formats fail.
`name` is a logical hint; it is never opened. `detect` also rejects inputs
exceeding `max_bytes` even when no deserialization is requested. `from_path`
is the separate filesystem convenience. Direct slice/string methods support borrowing where
supported by a backend. Reader/path methods buffer and return owned data.

## Sources (`anycfg::Source`)

```rust
pub struct Source<'a> { /* private fields */ }
impl<'a> Source<'a> {
    pub fn bytes(bytes: &'a [u8]) -> Self;
    pub fn text(text: &'a str) -> Self;
    pub fn reader(reader: impl std::io::Read + 'a) -> Self;
    pub fn path(path: &'a std::path::Path) -> Self;
    pub fn name(self, name: &'a str) -> Self;
    pub fn format(self, format: Format) -> Self;
}
```

A source can be moved into a `Loader::load` call only once; the reader is
consumed. Each source independently selects its format. `Source::path` uses
its own filename as its default name; `Source::name` overrides it. No source
has implicit filesystem access unless built with `Source::path`.

## Merge loader (`anycfg::Loader`)

```rust
pub enum Merge { Replace, Shallow, Deep }
pub struct Loader { /* private fields */ }
impl Loader {
    pub fn new() -> Self;
    pub fn merge(self, merge: Merge) -> Self;
    pub fn max_source_bytes(self, limit: usize) -> Self;
    pub fn max_total_bytes(self, limit: usize) -> Self;
    pub fn load<'a, T, I>(&self, sources: I) -> Result<T, Error>
    where T: serde::de::DeserializeOwned,
          I: IntoIterator<Item = Source<'a>>;
}
```

Default strategy: `Merge::Deep`; default budgets: 8 MiB per source, 64 MiB
combined; every layer is validated to 64 nested object/array levels.
Sources apply left
to right; later values win. `Replace` replaces the root. `Shallow` merges
only root object properties. `Deep` merges object properties recursively,
replacing arrays, scalars and null. Shallow/deep merging requires both root
documents to be objects. No sources is an error, not an empty object.

Every layer is decoded into a `serde_json::Value` and depth checked before
merging, including a source used alone or with `Replace`. Reader inputs are
limited by both their per-source cap and the remaining combined budget (with
at most one extra byte read to detect overflow). The values merge there,
and the final value deserializes into `T` once. Therefore, `load()` does not
promise preservation of TOML datetime types, unusual YAML types, number
spelling, or insertion order. A single-source direct `from_slice` does not
pass through the merging value model. The crate does not implement includes,
commands, interpolation, URI resolution, environment lookup, or async I/O.

## Serialization (`anycfg`)

```rust
pub fn to_string<T: serde::Serialize>(value: &T, format: Format) -> Result<String, Error>;
pub fn to_vec<T: serde::Serialize>(value: &T, format: Format) -> Result<Vec<u8>, Error>;
pub fn to_cfg_string<T: serde::Serialize>(value: &T, format: Format) -> Result<String, Error>;
pub fn to_cfg_vec<T: serde::Serialize>(value: &T, format: Format) -> Result<Vec<u8>, Error>;
pub fn to_writer<T: serde::Serialize>(writer: impl std::io::Write, value: &T, format: Format) -> Result<(), Error>;
pub fn to_cfg_writer<T: serde::Serialize>(writer: impl std::io::Write, value: &T, format: Format) -> Result<(), Error>;
```

`to_cfg_*` emits TOML without a shebang and adds `#!json` or `#!yaml` to
other formats; plain `to_*` emits format-native content. Writers serialize
completely before writing to their sink. The crate does not promise atomic
replacements when callers pass open files.

## Errors

`Error` is non-exhaustive: `UnsupportedFormat`, `UnsupportedExtension`,
`InvalidShebang`, `InvalidUtf8`, `TooLarge`, `TooDeep`, `NoSources`,
`MergeShape`, `InvalidShape`, `Parse`, `Serialize`, `Io`, and `Source`.
`Source { index, cause }` identifies the zero-based failing source.
The `Parse`, `Serialize` and `InvalidShape` variants deliberately omit
backend parser messages that might quote secret values. The API makes no
promise to expose offending configuration lines.

## Gotchas

- A `.cfg` document starting `{` is still TOML, not JSON, without `#!json`.
- An explicit format does not strip a shebang; it is ordinary document text.
- A reader is synchronous and buffered. Buffering stops at its input cap.
- TOML and JSON have different null and datetime domains; mixed-format deep
  merging uses the JSON-compatible subset, not a lossless cross-format AST.
- A `Source` name is only an extension hint; it is not an origin for includes.
