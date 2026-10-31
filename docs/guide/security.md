# Security and resource limits

`anycfg` only orchestrates inputs, codec selection, and caller-defined value merging. Treat configuration content as untrusted when it comes from users or external systems.

## What the core does not execute

- Does not execute format directives as commands.
- Does not run shell commands, templates, includes or interpolation.
- Does not follow remote URLs or fetch content.
- Does not automatically resolve a source's logical name to an actual path.
- Does not implicitly register format parsers.

A `#!format` header only selects an already registered codec for automatic formats; it is never executable.

## Limits

| Limit | Default | Where it is enforced |
| --- | --- | --- |
| Per-source bytes | 8 MiB | Input buffering / bytes before decoding |
| Combined source bytes | 64 MiB | Loader before decoding subsequent sources |
| Value depth | 64 levels | Passed to the caller's value adapter |

Tune budgets with `Loader::max_source_bytes`, `max_total_bytes`, and `max_depth`. Direct registry reads accept `Options::max_bytes`.

A reader may consume one extra byte to identify an exceeded budget. This is intentional; an unbounded reader is not read in full after reaching the configured limit.

!!! warning "Value depth is consumer-enforced"

    The library cannot inspect an arbitrary `V`. The default `MergeValues::validate` is a no-op; implement it for recursive value models, and enforce parser recursion/expansion limits inside the selected codec.

## Safe errors

Codec callbacks return `CodecError` with optional line and column positions. The public `Error::Parse`, `Error::Serialize` and `Error::Conversion` variants do not retain input excerpts or raw parser messages.

Do not add potentially sensitive source content to custom logging or error types. Filesystem `Error::Io` may still reveal ordinary operating-system diagnostics; assess that separately from document contents.

## Filesystem boundaries

`default_configs` rejects empty, absolute and traversing names and only returns paths of existing files under platform-standard directories. It does not create directories or open files.

`Source::path`, on the other hand, opens a **caller-selected path** when loading; the library does not provide a general sandbox. If your application loads user-supplied paths, enforce your own allowed roots, symlink rules and access controls.

## Trusted codecs

The application owns all registered codecs and any custom merge adapter. Only register parsers you intend to trust, with appropriate limits for aliases, nesting, expression evaluation, and other format-specific features. Do not assume that a parser is safe merely because the core never executes directives.

See [Sources and streams](sources.md), [Format registration](formats.md), and [Merging layers](merging.md).
