# Serde and typed configuration

`serde` is the only runtime dependency of `anycfg`. Applications may deserialize a registered format into a Serde-compatible value `V`, merge those values, and then deserialize the final result into any compatible Rust type.

The library does not depend on `serde_json`, `toml` or `serde-saphyr` at runtime. Those are **application-selected codecs** and are present as development dependencies for the example.

## Example dependencies

~~~toml
[dependencies]
anycfg = { git = "https://github.com/jose-pr/anycfg-rs" }
serde = { version = "1", features = ["derive"] }

# Choose your own format implementations:
serde_json = "1"
toml = "0.9"
serde-saphyr = "2"
~~~

## Deserialize directly into a struct

Choose a `V` that can act as a Serde deserializer. In this example, the application uses `serde_json::Value` and only registers JSON:

~~~rust
use anycfg::{Callbacks, CodecError, Formats, Options};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Server {
    host: String,
    port: u16,
}

fn main() -> Result<(), anycfg::Error> {
    let mut formats = Formats::<Value>::new();
    formats.register("json", ["json"], Callbacks::new(
        |bytes: &[u8]| serde_json::from_slice::<Value>(bytes)
            .map_err(|_| CodecError::default()),
        |value: &Value| serde_json::to_vec(value)
            .map_err(|_| CodecError::default()),
    ))?;

    let server: Server = formats.from_slice_typed(
        br#"{"host":"localhost","port":8080}"#,
        Options::new().name("server.json"),
    )?;

    assert_eq!(server.port, 8080);

    // The caller supplies the Serde serializer used to create the same V.
    let encoded = formats.to_vec_typed(
        &server,
        "json",
        serde_json::value::Serializer,
    )?;
    let decoded: Server = formats.from_slice_typed(
        &encoded,
        Options::new().format("json"),
    )?;
    assert_eq!(server, decoded);
    Ok(())
}
~~~

The codec selects how bytes become `V`; Serde then selects how `V` becomes `Server`. This separation works with custom representations as long as they satisfy the relevant Serde traits.

## Register TOML, YAML and JSON

Each format supplies its own byte parser and serializer, but all three return the application's chosen `Value`:

~~~rust
use anycfg::{Callbacks, CodecError, Formats};
use serde_json::Value;

# fn formats() -> Result<Formats<Value>, anycfg::Error> {
let mut formats = Formats::<Value>::new();

formats.register("toml", ["toml"], Callbacks::new(
    |bytes: &[u8]| {
        let text = std::str::from_utf8(bytes).map_err(|_| CodecError::default())?;
        toml::from_str::<Value>(text).map_err(|_| CodecError::default())
    },
    |value: &Value| toml::to_string_pretty(value)
        .map(|text| text.into_bytes())
        .map_err(|_| CodecError::default()),
))?;

formats.register("yaml", ["yaml", "yml"], Callbacks::new(
    |bytes: &[u8]| serde_saphyr::from_slice::<Value>(bytes)
        .map_err(|_| CodecError::default()),
    |value: &Value| serde_saphyr::to_string(value)
        .map(|text| text.into_bytes())
        .map_err(|_| CodecError::default()),
))?;

formats.register("json", ["json"], Callbacks::new(
    |bytes: &[u8]| serde_json::from_slice::<Value>(bytes)
        .map_err(|_| CodecError::default()),
    |value: &Value| serde_json::to_vec_pretty(value)
        .map_err(|_| CodecError::default()),
))?;

formats.register_marker("yaml", b"%YAML ")?;
# Ok(formats)
# }
~~~

Here the extensions `.toml`, `.json`, `.yaml` and `.yml` select their codecs directly. The auto-detect `.cfg` extension inherits the default supplied to `Loader::new` and may switch to another codec if its bytes have a registered marker or format header.

## Merge formats, then deserialize

Once codecs and an application-specific `MergeValues<Value>` adapter are registered, the loader can merge several different document formats:

~~~rust
# use anycfg::{Loader, Source, MergeValues, Error, Formats};
# use serde::Deserialize;
# use serde_json::Value;
# #[derive(Deserialize)]
# struct Config { name: String, server: Server }
# #[derive(Deserialize)]
# struct Server { host: String, port: u16 }
# fn example<M: MergeValues<Value>>(formats: &Formats<Value>, merge_adapter: &M) -> Result<(), Error> {
let config: Config = Loader::new(formats, "toml").load_typed(
    [
        Source::text("name = 'demo'\n[server]\nhost = 'localhost'\nport = 8000\n")
            .name("base.toml"),
        Source::text("server:\n  port: 8080\n")
            .name("override.yaml"),
        Source::text("#!json\n{\"server\":{\"host\":\"example.org\"}}")
            .name("local.cfg"),
    ],
    merge_adapter,
)?;

assert_eq!(config.server.host, "example.org");
assert_eq!(config.server.port, 8080);
# Ok(())
# }
~~~

A complete working example, including the merge adapter and output round trips, is included as [`examples/serde_formats.rs`](https://github.com/jose-pr/anycfg-rs/blob/main/examples/serde_formats.rs).

## Data fidelity

A shared value model is a choice by the application. In the examples, `serde_json::Value` is convenient for basic mappings, arrays and scalars, but it may lose format-specific information such as TOML datetimes, YAML tags or non-string map keys.

For applications requiring that information, choose a more expressive `V`, implement matching codecs and a matching `MergeValues<V>`, then use Serde conversion only where it can preserve the intended meaning. The library does not transform data through a hidden JSON-like intermediate tree.
