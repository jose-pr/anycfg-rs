//! Example: application-registered format codecs with Serde typed output.

use anycfg::{Callbacks, CodecError, Error, Formats, Loader, Merge, MergeValues, Options, Source};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Config {
    name: String,
    server: Server,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Server {
    host: String,
    port: u16,
}

fn formats() -> Result<Formats<Value>, Error> {
    let mut registry = Formats::new();
    registry.register("toml", ["toml"], Callbacks::new(
        |bytes: &[u8]| {
            let text = std::str::from_utf8(bytes).map_err(|_| CodecError::default())?;
            toml::from_str::<Value>(text).map_err(|_| CodecError::default())
        },
        |value: &Value| {
            toml::to_string_pretty(value)
                .map(|text| text.into_bytes())
                .map_err(|_| CodecError::default())
        },
    ))?;
    registry.register("yaml", ["yaml", "yml"], Callbacks::new(
        |bytes: &[u8]| serde_saphyr::from_slice::<Value>(bytes).map_err(|_| CodecError::default()),
        |value: &Value| {
            serde_saphyr::to_string(value)
                .map(|text| text.into_bytes())
                .map_err(|_| CodecError::default())
        },
    ))?;
    registry.register("json", ["json"], Callbacks::new(
        |bytes: &[u8]| serde_json::from_slice::<Value>(bytes).map_err(|_| CodecError::default()),
        |value: &Value| serde_json::to_vec_pretty(value).map_err(|_| CodecError::default()),
    ))?;
    registry.set_default("toml")?;
    registry.register_marker("yaml", b"%YAML ")?;
    Ok(registry)
}

/// Example merging policy for `serde_json::Value`; applications may use any
/// other Serde-compatible representation or their own merge adapter.
struct ValueMerge;

impl MergeValues<Value> for ValueMerge {
    fn validate(&self, root: &Value, limit: usize) -> Result<(), Error> {
        let mut stack = vec![(root, 0usize)];
        while let Some((node, depth)) = stack.pop() {
            if depth > limit { return Err(Error::TooDeep { limit }); }
            match node {
                Value::Array(items) => stack.extend(items.iter().map(|item| (item, depth + 1))),
                Value::Object(items) => stack.extend(items.values().map(|item| (item, depth + 1))),
                _ => {}
            }
        }
        Ok(())
    }

    fn merge(&self, prior: &mut Value, next: Value, strategy: Merge, limit: usize) -> Result<(), Error> {
        if strategy == Merge::Replace { *prior = next; return Ok(()); }
        let (Value::Object(target), Value::Object(source)) = (prior, next) else {
            return Err(Error::MergeShape);
        };
        for (key, value) in source {
            if strategy == Merge::Deep {
                if let Some(old) = target.get_mut(&key) {
                    if old.is_object() && value.is_object() {
                        self.merge(old, value, strategy, limit.saturating_sub(1))?;
                    } else {
                        *old = value;
                    }
                    continue;
                }
            }
            target.insert(key, value);
        }
        Ok(())
    }
}

pub fn run() -> Result<(), Error> {
    let formats = formats()?;
    let typed: Config = Loader::new(&formats, "toml").load_typed([
        Source::text("name = 'demo'\n[server]\nhost = 'localhost'\nport = 8000\n").format("toml"),
        Source::text("server:\n  port: 8080\n").name("override.yaml"),
        Source::text("#!json\n{\"server\":{\"host\":\"example.org\"}}").name("local.cfg"),
    ], &ValueMerge)?;
    assert_eq!(typed.server.host, "example.org");
    assert_eq!(typed.server.port, 8080);

    let from_toml: Config = formats.from_slice_typed(
        b"name = 'demo'\n[server]\nhost = 'localhost'\nport = 8000\n",
        Options::new().name("config.toml"),
    )?;
    assert_eq!(from_toml.server.port, 8000);

    let from_yaml: Config = formats.from_reader_typed(
        std::io::Cursor::new(b"name: demo\nserver:\n  host: localhost\n  port: 8000\n"),
        Options::new().format("yaml"),
    )?;
    assert_eq!(from_yaml, from_toml);

    let from_json: Vec<u16> = formats.from_slice_typed(
        b"[10,20,30]", Options::new().format("json"),
    )?;
    assert_eq!(from_json, vec![10, 20, 30]);

    // A single loader default covers unnamed streams and unmarked .cfg files.
    let streamed: Config = Loader::new(&formats, "json")
        .merge(Merge::Replace)
        .load_typed([Source::reader(std::io::Cursor::new(
            br#"{"name":"demo","server":{"host":"localhost","port":8000}}"#,
        ))], &ValueMerge)?;
    assert_eq!(streamed, from_toml);

    let bad: Error = formats.from_slice_typed::<Config>(
        br#"{"name":"swordfish","server":{"host":"example.org","port":"invalid"}}"#,
        Options::new().format("json"),
    ).unwrap_err();
    assert!(matches!(bad, Error::Conversion));
    assert!(!format!("{bad:?} {bad}").contains("swordfish"));

    // The same typed value can be serialized into any registered output format.
    let json = formats.to_vec_typed(&typed, "json", serde_json::value::Serializer)?;
    let decoded: Config = formats.from_slice_typed(&json, Options::new().format("json"))?;
    assert_eq!(typed, decoded);

    let toml = formats.to_vec_typed(&typed, "toml", serde_json::value::Serializer)?;
    let from_toml_output: Config = formats.from_slice_typed(&toml, Options::new().format("toml"))?;
    assert_eq!(typed, from_toml_output);

    let yaml = formats.to_auto_vec_typed(&typed, "yaml", "cfg", serde_json::value::Serializer)?;
    assert!(yaml.starts_with(b"#!yaml\n"));
    let round_trip: Config = formats.from_slice_typed(&yaml, Options::new().name("app.cfg"))?;
    assert_eq!(typed, round_trip);
    Ok(())
}

#[cfg(not(test))]
fn main() -> Result<(), Error> {
    run()
}
