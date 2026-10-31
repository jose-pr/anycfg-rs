# Design constraints

`anycfg` is a configuration orchestration crate, not a document parser or a
universal value model. It owns the discovery order, input boundaries, format
selection and source bookkeeping. The consumer owns data interpretation.

## Responsibilities

- **Source:** borrowed bytes, UTF-8 text, a one-shot reader or a local path.
- **Format:** `Formats<V>` maps registered names and extensions to `Codec<V>`
  implementors; `Callbacks` adapts closures.
- **Detection:** explicit choice, fixed extension, or automatic extension
  selection (registered headers/markers before its default), then loader or
  registry fallback. The `.cfg` extension is auto-registered with an unpinned
  fallback that inherits the explicitly supplied default in `Loader::new`. No speculative parsing or I/O.
- **Value:** the decoder returns any caller-defined `V` directly, without
  converting through a generic JSON-compatible representation.
- **Merge:** the `MergeValues<V>` adapter receives the strategy and depth budget;
  it determines what maps, lists, scalar/opaque values and deletion mean.
- **Typed output:** Serde conversion to a target type or a custom `load_as`
  callback; no format crate is required by the library.
- **Paths:** the same user/system configuration directories as the Python
  project, using the operating system's conventions, without opening files.

## Data fidelity

The core does not convert numbers, datetimes, tags, null values, byte arrays,
or mapping order. A consumer can preserve these exactly in its model. The
codec determines whether source representations are representable in that
model, and the converter determines whether `V` can become `T`.

## Trust and budgets

No implicit command, network, includes, interpolation or executable header.
Only registered codecs can parse bytes; the parser's own recursion and
expansion bounds belong to its implementation. The loader bounds source bytes
and total bytes and passes a depth limit to the model adapter. `load_as` and
codec errors are sanitized and do not include untrusted document content.

`default_configs` checks for files, never creates them, and returns
`[Option<system>, Option<user>]`. The caller explicitly creates sources from
present paths. Filesystem errors while opening a present source are returned,
not silently interpreted as a missing configuration.
