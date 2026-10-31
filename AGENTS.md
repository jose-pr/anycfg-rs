# `anycfg` — API header

Header-file-style reference for the `anycfg` crate: public signatures,
contracts and defaults. This is shipped beside `Cargo.toml` and `README.md`.
Repository: <https://github.com/jose-pr/anycfg-rs>.

- Rust 1.89+, edition 2024; crate name and Rust import: `anycfg`.
- Zero runtime dependencies and no built-in codecs or data model.
- Root re-exports all public types; implementation modules are private.
- `#![forbid(unsafe_code)]`; no global registry, no runtime, no logger.
- Reads environment only in `user_config_path` and `system_config_path`.

## Format registry (`anycfg::Formats<V>`)

```rust
pub trait Codec<V> {
    fn decode(&self, input: &[u8]) -> Result<V, CodecError>;
    fn encode(&self, value: &V) -> Result<Vec<u8>, CodecError>;
}
pub struct CodecError { pub line: Option<usize>, pub column: Option<usize> }
pub struct Callbacks<V, D, E> { /* private fields */ }
impl<V, D, E> Callbacks<V, D, E> {
    pub fn new(decode: D, encode: E) -> Self;
}
pub struct Formats<V> { /* private fields */ }
impl<V> Formats<V> {
    pub fn new() -> Self;
    pub fn register<C, I, S>(&mut self, name: &str, extensions: I, codec: C) -> Result<(), Error>
      where C: Codec<V> + 'static, I: IntoIterator<Item = S>, S: AsRef<str>;
    pub fn set_default(&mut self, name: &str) -> Result<(), Error>;
    pub fn register_marker(&mut self, name: &str, marker: &[u8]) -> Result<(), Error>;
    pub fn detect<'a>(&'a self, input: &'a [u8], options: Options<'_>) -> Result<Detection<'a>, Error>;
    pub fn from_slice(&self, input: &[u8], options: Options<'_>) -> Result<V, Error>;
    pub fn from_str(&self, input: &str, options: Options<'_>) -> Result<V, Error>;
    pub fn from_reader(&self, input: impl std::io::Read, options: Options<'_>) -> Result<V, Error>;
    pub fn to_vec(&self, value: &V, name: &str) -> Result<Vec<u8>, Error>;
    pub fn to_string(&self, value: &V, name: &str) -> Result<String, Error>;
    pub fn to_writer(&self, writer: impl std::io::Write, value: &V, name: &str) -> Result<(), Error>;
    pub fn to_cfg_vec(&self, value: &V, name: &str) -> Result<Vec<u8>, Error>;
}
```

`CodecError` carries only positions. Codec messages are never retained in
public parse errors. Caller registers names and extensions, optional native
prefix markers, and an explicit fallback; no format is pre-registered. A
registered extension takes priority over shebang detection; anonymous and
`.cfg` inputs recognize a leading `#!name`/`#!/path/name`, then registered
native markers, then the selected fallback. A forced format retains the
shebang. Native markers remain in the payload. One leading UTF-8 BOM is
removed; raw input otherwise reaches the selected codec unchanged.

```rust
pub struct Options<'a> {
    pub format: Option<&'a str>,
    pub name: Option<&'a str>,
    pub max_bytes: usize,
}
impl<'a> Options<'a> {
    pub fn new() -> Self;
    pub fn name(self, name: &'a str) -> Self;
    pub fn format(self, format: &'a str) -> Self;
    pub fn max_bytes(self, limit: usize) -> Self;
}
pub struct Detection<'a> { pub format: &'a str, pub payload: &'a [u8] }
```

Default per-input cap: 8 MiB. `Options::name` is a name hint, not an open
operation. `Formats::to_cfg_vec` adds a self-describing shebang for a nondefault
format; the fallback writes unmarked.

## Sources (`anycfg::Source`)

```rust
pub struct Source<'a> { /* private fields */ }
impl<'a> Source<'a> {
    pub fn bytes(bytes: &'a [u8]) -> Self;
    pub fn text(text: &'a str) -> Self;
    pub fn reader(reader: impl std::io::Read + 'a) -> Self;
    pub fn path(path: &'a std::path::Path) -> Self;
    pub fn name(self, name: &'a str) -> Self;
    pub fn format(self, format: &'a str) -> Self;
}
```

Only `Source::path` opens a file; stream readers are consumed during loading.
Per-source `format` overrides the name and any headers.

## Loader and merge adapter (`anycfg::Loader<V>`)

```rust
pub enum Merge { Replace, Shallow, Deep }
pub trait MergeValues<V> {
    fn validate(&self, value: &V, max_depth: usize) -> Result<(), Error> { /* no-op */ }
    fn merge(&self, previous: &mut V, next: V, strategy: Merge, max_depth: usize) -> Result<(), Error>;
}
pub struct ReplaceValues;
pub struct Loader<'f, V> { /* private fields */ }
impl<'f, V> Loader<'f, V> {
    pub fn new(formats: &'f Formats<V>) -> Self;
    pub fn merge(self, strategy: Merge) -> Self;
    pub fn max_source_bytes(self, limit: usize) -> Self;
    pub fn max_total_bytes(self, limit: usize) -> Self;
    pub fn max_depth(self, limit: usize) -> Self;
    pub fn load<'a, I, S, M>(&self, sources: I, adapter: &M) -> Result<V, Error>
      where I: IntoIterator<Item = S>, S: Into<Option<Source<'a>>>, M: MergeValues<V>;
    pub fn load_as<'a, I, S, M, T, E, F>(&self, sources: I, adapter: &M, convert: F) -> Result<T, Error>
      where I: IntoIterator<Item = S>, S: Into<Option<Source<'a>>>, M: MergeValues<V>, F: FnOnce(V) -> Result<T, E>;
    pub fn load_or<'a, I, S, M>(&self, sources: I, adapter: &M, fallback: V) -> Result<V, Error>
      where I: IntoIterator<Item = S>, S: Into<Option<Source<'a>>>, M: MergeValues<V>;
}
```

`Merge::Deep` is the default. These policies are *names* given to the
adapter, not built-in manipulation of JSON or other containers. A closure
with signature `Fn(&mut V, V, Merge, usize) -> Result<(), Error>` is an
adapter with no-op validation. `ReplaceValues` is only for replacement
merging. `load_as` sanitizes conversion errors as `Error::Conversion`.
Sources are loaded left-to-right; `None` layers are skipped. `load` returns
`NoSources` when all are absent; `load_or` returns the explicit fallback.
Default source cap: 8 MiB. Combined cap: 64 MiB. Depth passed to the
adapter: 64. Recursive value models must implement `validate` and parser
input depth/expansion limits to enforce these limits.

## Standard locations (`anycfg`)

```rust
pub fn user_config_path(name: Option<&std::path::Path>) -> Result<std::path::PathBuf, Error>;
pub fn system_config_path(name: Option<&std::path::Path>) -> std::path::PathBuf;
pub fn default_configs(name: impl AsRef<std::path::Path>) -> Result<[Option<std::path::PathBuf>; 2], Error>;
```

Windows: `%APPDATA%` (fallback `~/AppData/Roaming`) and `%PROGRAMDATA%`
(fallback `C:\\ProgramData`). Unix/macOS: absolute `$XDG_CONFIG_HOME` or
`~/.config`, and `/etc` for system configuration. `default_configs` returns
system then user paths, replacing absent/non-file paths with `None`.
An absolute, empty or traversal filename is rejected for default discovery.
It does not create directories or read documents. Its `None` values can be
mapped to optional `Source::path` instances.

## Errors

`Error` is `#[non_exhaustive]`: `NoDefaultFormat`, `UnsupportedFormat`,
`UnsupportedExtension`, `InvalidRegistration`, `InvalidShebang`,
`InvalidUtf8 { offset }`, `TooLarge { limit }`, `TooDeep { limit }`,
`NoSources`, `MergeShape`, `Conversion`, `Parse { format, line, column }`,
`Serialize { format }`, `Source { index, cause }`, `HomeUnavailable`,
`InvalidConfigName`, `Io`.

## Environment variables

- Windows: `APPDATA` and `PROGRAMDATA` for path discovery.
- Unix/macOS: `XDG_CONFIG_HOME` (absolute only); home is platform-resolved.
- The library does not read application-specific settings, secrets or keys.

## Gotchas

- No built-in parsers or converters; consumers must register codecs.
- No canonical intermediate value type; number, datetime, tagged and binary
  fidelity are controlled by the consumer's value model and codecs.
- `MergeValues::validate` defaults to no-op; callers must enforce depth for
  their own recursive values, and bound recursion inside parsers themselves.
- `.cfg` is a filename convention; without a registered fallback or header it
  reports `NoDefaultFormat`. Unknown names never trigger parser guessing.
- No includes, command evaluation, environment substitution, URL resolution,
  global state, or asynchronous runtime. No implied filesystem access.
