# Configuration format and layering design

`anycfg` is a standalone Serde configuration library. The core parser is
independent of any filesystem location, environment variable, runtime or
application-specific configuration discovery.

## Source pipeline

```text
Source::bytes | Source::text | Source::reader | Source::path
                               |
                         bounded input
                               |
                  optional format / name hint
                               |
          .cfg header / known suffix / default TOML
                               |
                    TOML / JSON / YAML
                               |
                    Serde value (owned)
                               |
             ordered Replace / Shallow / Deep
                               |
                  DeserializeOwned target
```

`Source::name` is not necessarily a path. Applications can supply a logical
name for data that came from a database row, network response or Guatiao
provider. An unknown extension is rejected rather than guessed. An explicit
`Source::format` wins over both suffix and header, and does not remove a `#!`
line. Input bytes are read once per source; reader and path values are
consumed during `Loader::load`, not when Source is constructed.

## Deterministic merge semantics

- Sources are applied left-to-right; the last source wins conflicts.
- `Replace` discards the preceding document and uses the latest one.
- `Shallow` combines only root object properties; a later whole section
  replaces the earlier section, including its members.
- `Deep` recursively combines object properties; later arrays, scalars and
  nulls replace earlier values. No list concatenation and no implicit
  deletion sentinel.
- Shallow/deep require object roots, else a `MergeShape` error with the
  source index; `Replace` also allows root arrays or scalars.
- Missing fields are inherited from earlier sources. Applications can
  apply `serde(default)` at the final deserialization step.
- No sources is an error, never silently `Default::default()`.
- Each source has an 8 MiB default limit; the loader has a 64 MiB
  cumulative input limit; recursive merge stops at 64 mapping levels.

The implementation currently merges through `serde_json::Value`, not a fully
lossless cross-format representation. This explicitly scopes merged data to
JSON-compatible values. In particular TOML datetime types, YAML-specific
values, number spelling and insertion order are not preserved as a contract.
A future merge representation should be capable of preserving types and order
without silently changing them. Single-document `from_slice` and `from_str`
keep their direct backend deserialization semantics, including borrowing where
supported.

## Adopting from other projects

**AI6:** Replace bootstrap format-detection internals without moving any
bootstrap-specific path resolution, secret loading, validation or config
precedence into this crate. `ai6.cfg`, `unex.cfg` and similar sources can
be passed by name; provider layers can be passed as bytes. `#[serde(deny_unknown_fields)]`
remains the application's responsibility.

**Guatiao:** Keep `guatiao-serde`'s native value and presentation model, its
allocator-aware seeds and exact-number options. `anycfg::detect` can be
used without the `Loader` intermediate. To merge Guatiao values losslessly,
add a typed intermediate or a caller-supplied merge adapter; do not convert
`guatiao::Value` into `serde_json::Value` and claim no information is lost.

**Canoa:** Native components can reuse this crate. Flutter Web does not
suddenly acquire a native Rust parser; Dart-side format detection needs
shared test vectors (including BOM, CRLF, .cfg, wrong suffix, unknown
shebangs, `%YAML`, parser failures and budget overflows). Its PWA
configuration remains read-only.

## Extensions that preserve the core boundary

Includes require an explicit resolver interface so a file, provider record
or network-supplied source can resolve an include relative to its own origin.
The resolver must return stable identity, logical name, bytes and an optional
origin. Cycle detection, depth, cumulative byte and include-count budgets
belong to the loading operation, not the TOML/JSON/YAML backend. An include
is never activated by merely reading from bytes, and command/template
execution remains off unless expressly enabled later.

A separate API accepting `DeserializeSeed` can let Guatiao deserialize into
allocator-backed values without constructing a JSON intermediate. Async I/O
can be a separate optional feature without forcing Tokio on synchronous
consumers. INI, dotenv and format plugins can also be added as opt-in
features after their dialects and security limits are specified.

## Errors and safety

Errors never quote the source's configuration value, raw line or unredacted
parser diagnostic, because configuration commonly contains secrets. Parsing,
shape, budget, I/O and invalid-format failures are separate variants.
Source errors wrap the safe cause and identify the zero-based index.
No built-in operation executes a shebang, evaluates a command, resolves
paths from an anonymous source or interpolates environment variables.

## Package and repository conventions

The crate's root `AGENTS.md` is a shipped header-file-style API reference,
not private working instructions. `.agents/`, local overrides and harness
metadata are ignored and excluded from Cargo packages. Commit messages
follow `type: description`, with focused changes and no assistant or
session attribution trailers. Publication remains disabled pending a
licensing decision and a passing Rust test/CI matrix.
