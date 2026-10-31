# `anycfg` — API header

Header-file-style reference for the `anycfg` crate: public signatures,
contracts and defaults. This is shipped beside `Cargo.toml` and `README.md`.
Repository: <https://github.com/jose-pr/anycfg-rs>.

- Rust 1.89+, edition 2024; crate name and Rust import: `anycfg`.
- Runtime dependency: `serde = "1"`. No built-in codecs or data model; the
  JSON/TOML/YAML crates are used only in development examples and tests.
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
    pub fn register_extension(&mut self, extension: &str, format: &str) -> Result<(), Error>;
    pub fn register_auto<I, S>(&mut self, extensions: I, default_format: &str) -> Result<(), Error>
      where I: IntoIterator<Item = S>, S: AsRef<str>;
    pub fn set_default(&mut self, name: &str) -> Result<(), Error>;
    pub fn register_marker(&mut self, name: &str, marker: &[u8]) -> Result<(), Error>;
    pub fn detect<'a>(&'a self, input: &'a [u8], options: Options<'_>) -> Result<Detection<'a>, Error>;
    pub fn from_slice(&self, input: &[u8], options: Options<'_>) -> Result<V, Error>;
    pub fn from_slice_typed<T: serde::de::DeserializeOwned>(&self, input: &[u8], options: Options<'_>) -> Result<T, Error>
      where V: for<'de> serde::Deserializer<'de>;
    pub fn from_str(&self, input: &str, options: Options<'_>) -> Result<V, Error>;
    pub fn from_str_typed<T: serde::de::DeserializeOwned>(&self, input: &str, options: Options<'_>) -> Result<T, Error>
      where V: for<'de> serde::Deserializer<'de>;
    pub fn from_reader(&self, input: impl std::io::Read, options: Options<'_>) -> Result<V, Error>;
    pub fn from_reader_typed<T: serde::de::DeserializeOwned>(&self, input: impl std::io::Read, options: Options<'_>) -> Result<T, Error>
      where V: for<'de> serde::Deserializer<'de>;
    pub fn to_vec(&self, value: &V, name: &str) -> Result<Vec<u8>, Error>;
    pub fn to_vec_typed<T, S>(&self, value: &T, name: &str, serializer: S) -> Result<Vec<u8>, Error>
      where T: serde::Serialize + ?Sized, S: serde::Serializer<Ok = V>;
    pub fn to_string(&self, value: &V, name: &str) -> Result<String, Error>;
    pub fn to_writer(&self, writer: impl std::io::Write, value: &V, name: &str) -> Result<(), Error>;
    pub fn to_auto_vec(&self, value: &V, name: &str, extension: &str) -> Result<Vec<u8>, Error>;
    pub fn to_auto_vec_typed<T, S>(&self, value: &T, name: &str, extension: &str, serializer: S) -> Result<Vec<u8>, Error>
      where T: serde::Serialize + ?Sized, S: serde::Serializer<Ok = V>;
}
```

`CodecError` carries only positions. Codec messages are never retained in
public parse errors. Callers register codecs, extensions, optional native
prefix markers, and a registry fallback. The registry pre-registers `.cfg`
as an auto-detect extension; it is not a codec. For unnamed sources and this
unpinned extension, the default comes from the required second parameter
of `Loader::new` (or `Options::default_format` for direct reads). Direct
`Formats` reads can still fall back to the registry default. Other
extensions are fixed by default; callers may opt them into auto-detection
with `register_auto`. Re-registering `.cfg` pins its default, or
`register_extension` can make it fixed. Automatic detection recognizes a leading
`#!name` or `#!/path/name` (stripping the header), and registered native markers
(which remain in the payload). Explicit formats and fixed extensions do not
inspect headers. A single UTF-8 BOM is removed.

```rust
pub struct Options<'a> {
    pub format: Option<&'a str>,
    pub name: Option<&'a str>,
    pub default_format: Option<&'a str>,
    pub max_bytes: usize,
}
impl<'a> Options<'a> {
    pub fn new() -> Self;
    pub fn name(self, name: &'a str) -> Self;
    pub fn format(self, format: &'a str) -> Self;
    pub fn default_format(self, format: &'a str) -> Self;
    pub fn max_bytes(self, limit: usize) -> Self;
}
pub struct Detection<'a> { pub format: &'a str, pub payload: &'a [u8] }
```

Default per-input cap: 8 MiB. `Options::name` is a name hint, not an open
operation. `Formats::to_auto_vec` is for a registered auto-detect extension;
when the extension has no pinned default, output always carries a header.
When pinned, output is unmarked only if the codec matches that default.

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
    pub fn new(formats: &'f Formats<V>, default_format: impl Into<String>) -> Self;
    pub fn merge(self, strategy: Merge) -> Self;
    pub fn default_format(self, format: impl Into<String>) -> Self;
    pub fn max_source_bytes(self, limit: usize) -> Self;
    pub fn max_total_bytes(self, limit: usize) -> Self;
    pub fn max_depth(self, limit: usize) -> Self;
    pub fn load<'a, I, S, M>(&self, sources: I, adapter: &M) -> Result<V, Error>
      where I: IntoIterator<Item = S>, S: Into<Option<Source<'a>>>, M: MergeValues<V>;
    pub fn load_as<'a, I, S, M, T, E, F>(&self, sources: I, adapter: &M, convert: F) -> Result<T, Error>
      where I: IntoIterator<Item = S>, S: Into<Option<Source<'a>>>, M: MergeValues<V>, F: FnOnce(V) -> Result<T, E>;
    pub fn load_typed<'a, I, S, M, T>(&self, sources: I, adapter: &M) -> Result<T, Error>
      where I: IntoIterator<Item = S>, S: Into<Option<Source<'a>>>, M: MergeValues<V>,
            V: for<'de> serde::Deserializer<'de>, T: serde::de::DeserializeOwned;
    pub fn load_or<'a, I, S, M>(&self, sources: I, adapter: &M, fallback: V) -> Result<V, Error>
      where I: IntoIterator<Item = S>, S: Into<Option<Source<'a>>>, M: MergeValues<V>;
}
```

`Merge::Deep` is the default merge strategy. The loader's required
`default_format` must name a registered codec; automatic detection uses it
when no source marker wins. The `default_format` builder can change it later.
`load_typed` converts the final merged `V`
through its Serde deserializer into any owned Serde type. `load_as` instead
accepts a custom conversion closure. Both sanitize conversion errors.
Merge strategies are names given to the adapter, not built-in manipulation
of maps or other containers. A closure
with signature `Fn(&mut V, V, Merge, usize) -> Result<(), Error>` is an
adapter with no-op validation. `ReplaceValues` is only for replacement
merging. Sources are loaded left-to-right; `None` layers are skipped. `load` returns
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

## Serde examples

The repository example at <https://github.com/jose-pr/anycfg-rs/blob/main/examples/serde_formats.rs>
registers TOML, YAML and JSON handlers with consumer-side dependencies,
merges heterogeneous sources and deserializes their result into ordinary
Serde-derived Rust structs. It is not a built-in format registry.

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

- No built-in parsers; consumers must register codecs. Serde typed methods
  require that `V` implement `serde::Deserializer` or that the caller supply
  a `serde::Serializer<Ok = V>` for typed writes.
- No canonical intermediate value type; number, datetime, tagged and binary
  fidelity are controlled by the consumer's value model and codecs.
- `MergeValues::validate` defaults to no-op; callers must enforce depth for
  their own recursive values, and bound recursion inside parsers themselves.
- `.cfg` is auto-registered for detection; it uses the loader default if no
  marker matches. Without any configured default, it reports `NoDefaultFormat`.
  Other extensions require registration. Unknown names never trigger guessing.
- No includes, command evaluation, environment substitution, URL resolution,
  global state, or asynchronous runtime. No implied filesystem access.

