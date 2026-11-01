//! TOML, JSON and YAML registered over one value type, read into a typed struct.
//!
//! anycfg has no format of its own: this is what an application writes. The failure functions
//! are the part to keep. Each passes on only what its parser is known not to quote from the
//! document, because `toml`'s and `serde-saphyr`'s `Display` print the offending line.

use anycfg::{Callbacks, CodecError, Error, Formats, Loader, Merge, MergeValues, Options, Source};
use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde::{Deserialize as DeriveDeserialize, Serialize};
use serde_json::{Map, Value};

/// A registry reading `.toml`, `.json`, `.yaml` and `.yml` by extension, and `.cfg` by its first
/// line (`#!json`, `#!yaml`, `#!toml`, a document starting `%YAML`) or else as TOML.
pub fn formats() -> Result<Formats<Value>, Error> {
    let mut formats = Formats::new();
    formats.register(
        "toml",
        ["toml"],
        Callbacks::new(decode_toml, |value: &Value| {
            toml::to_string_pretty(&without_nulls(value))
                .map(String::into_bytes)
                .map_err(|_| CodecError::new().message("the value has no TOML form"))
        }),
    )?;
    formats.register(
        "json",
        ["json"],
        Callbacks::new(decode_json, |value: &Value| {
            serde_json::to_vec_pretty(value)
                .map_err(|_| CodecError::new().message("the value has no JSON form"))
        }),
    )?;
    formats.register(
        "yaml",
        ["yaml", "yml"],
        Callbacks::new(decode_yaml, |value: &Value| {
            serde_saphyr::to_string(value)
                .map(String::into_bytes)
                .map_err(|_| CodecError::new().message("the value has no YAML form"))
        }),
    )?;
    formats.register_auto(["cfg"], "toml")?;
    formats.register_marker("yaml", b"%YAML")?;
    Ok(formats)
}

fn decode_toml(bytes: &[u8]) -> Result<Value, CodecError> {
    let text = core::str::from_utf8(bytes).map_err(|error| {
        CodecError::at_offset(bytes, error.valid_up_to()).message("not valid UTF-8")
    })?;
    let table: toml::Table = toml::from_str(text).map_err(|error| toml_failure(text, &error))?;
    Ok(toml_to_value(toml::Value::Table(table)))
}

/// `message()` is the parser's text without the excerpt its `Display` adds. `duplicate key` is
/// the one failure worth naming a key for: the span then covers just that key.
fn toml_failure(text: &str, error: &toml::de::Error) -> CodecError {
    let span = error.span();
    let failure = span.clone().map_or_else(CodecError::new, |span| {
        CodecError::at_offset(text.as_bytes(), span.start)
    });
    let key = span
        .and_then(|span| text.get(span))
        .filter(|key| error.message() == "duplicate key" && is_plain_key(key));
    match key {
        Some(key) => failure.message(format!("duplicate key `{key}`")),
        None => failure.message(error.message()),
    }
}

fn is_plain_key(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Datetimes become text: deserialized straight into `Value` they would be a map under `toml`'s
/// private key. The parser has already bounded the nesting this recurses over.
fn toml_to_value(value: toml::Value) -> Value {
    match value {
        toml::Value::String(text) => Value::String(text),
        toml::Value::Integer(number) => Value::from(number),
        toml::Value::Float(number) => Value::from(number),
        toml::Value::Boolean(flag) => Value::Bool(flag),
        toml::Value::Datetime(datetime) => Value::String(datetime.to_string()),
        toml::Value::Array(items) => Value::Array(items.into_iter().map(toml_to_value).collect()),
        toml::Value::Table(table) => Value::Object(
            table
                .into_iter()
                .map(|(key, value)| (key, toml_to_value(value)))
                .collect(),
        ),
    }
}

/// TOML has no null, and through `Value` a `None` field arrives as one: left out, it reads back as
/// `None`, which is what `toml` itself does with a `None` field.
fn without_nulls(value: &Value) -> Value {
    match value {
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .filter(|(_, value)| !value.is_null())
                .map(|(key, value)| (key.clone(), without_nulls(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(without_nulls).collect()),
        other => other.clone(),
    }
}

fn decode_json(bytes: &[u8]) -> Result<Value, CodecError> {
    serde_json::from_slice::<UniqueKeys>(bytes)
        .map(|value| value.0)
        .map_err(|error| {
            // With `UniqueKeys` as the target, the text is one of serde_json's fixed syntax
            // messages or the duplicate-key message below, followed by the position.
            let text = error.to_string();
            let position = format!(" at line {} column {}", error.line(), error.column());
            CodecError::new()
                .line(error.line())
                .column(error.column())
                .message(text.strip_suffix(&position).unwrap_or(&text))
        })
}

/// A JSON value whose objects refuse a repeated key, which `Value` would silently resolve to the
/// last one.
struct UniqueKeys(Value);

impl<'de> Deserialize<'de> for UniqueKeys {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(UniqueKeysVisitor).map(Self)
    }
}

struct UniqueKeysVisitor;

impl<'de> Visitor<'de> for UniqueKeysVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("any JSON value")
    }
    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }
    fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
        Ok(Value::from(value))
    }
    fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
        Ok(Value::from(value))
    }
    fn visit_f64<E>(self, value: f64) -> Result<Value, E> {
        Ok(Value::from(value))
    }
    fn visit_str<E>(self, value: &str) -> Result<Value, E> {
        Ok(Value::from(value))
    }
    fn visit_seq<A: de::SeqAccess<'de>>(self, mut items: A) -> Result<Value, A::Error> {
        let mut out = Vec::new();
        while let Some(UniqueKeys(item)) = items.next_element()? {
            out.push(item);
        }
        Ok(Value::Array(out))
    }
    fn visit_map<A: de::MapAccess<'de>>(self, mut entries: A) -> Result<Value, A::Error> {
        let mut out = Map::new();
        while let Some(key) = entries.next_key::<String>()? {
            if out.contains_key(&key) {
                return Err(de::Error::custom(format_args!(
                    "duplicate key `{}`",
                    key.escape_default()
                )));
            }
            let UniqueKeys(value) = entries.next_value()?;
            out.insert(key, value);
        }
        Ok(Value::Object(out))
    }
}

fn decode_yaml(bytes: &[u8]) -> Result<Value, CodecError> {
    serde_saphyr::from_slice(bytes).map_err(|error| yaml_failure(&error))
}

/// `Display` prints the source lines, and several variants' text quotes a scalar. So: the position
/// always, and the text only of a variant known to hold none. Its parser's own messages are fixed
/// strings (granit-parser 2.0).
fn yaml_failure(error: &serde_saphyr::Error) -> CodecError {
    use serde_saphyr::Error as Yaml;
    let failure = match error.location() {
        Some(location) => CodecError::new()
            .line(location.line() as usize)
            .column(location.column() as usize),
        None => CodecError::new(),
    };
    match error.without_snippet() {
        Yaml::ExternalMessage { msg, .. } => failure.message(msg.as_str()),
        Yaml::DuplicateMappingKey { key: Some(key), .. } => {
            failure.message(format!("duplicate key `{}`", key.escape_default()))
        }
        Yaml::DuplicateMappingKey { key: None, .. } => failure.message("duplicate key"),
        Yaml::Eof { .. } => failure.message("the document ended early"),
        Yaml::UnknownAnchor { .. } => failure.message("an alias names no anchor"),
        Yaml::MultipleDocuments { .. } => failure.message("more than one document"),
        Yaml::Budget { .. } => failure.message("the document exceeds the parser's budget"),
        _ => failure.message("not valid YAML"),
    }
}

/// A merge policy for `Value`: later sources win, and maps merge when `Merge::Deep` is selected.
pub struct ValueMerge;

impl MergeValues<Value> for ValueMerge {
    fn validate(&self, root: &Value, limit: usize) -> Result<(), Error> {
        let mut stack = vec![(root, 0usize)];
        while let Some((node, depth)) = stack.pop() {
            if depth > limit {
                return Err(Error::TooDeep { limit });
            }
            match node {
                Value::Array(items) => stack.extend(items.iter().map(|item| (item, depth + 1))),
                Value::Object(items) => stack.extend(items.values().map(|item| (item, depth + 1))),
                _ => {}
            }
        }
        Ok(())
    }

    fn merge(
        &self,
        prior: &mut Value,
        next: Value,
        strategy: Merge,
        limit: usize,
    ) -> Result<(), Error> {
        if strategy == Merge::Replace {
            *prior = next;
            return Ok(());
        }
        let (Value::Object(target), Value::Object(source)) = (prior, next) else {
            return Err(Error::MergeShape);
        };
        for (key, value) in source {
            match target.get_mut(&key) {
                Some(old) if strategy == Merge::Deep && old.is_object() && value.is_object() => {
                    self.merge(old, value, strategy, limit.saturating_sub(1))?;
                }
                _ => {
                    target.insert(key, value);
                }
            }
        }
        Ok(())
    }
}

/// The application's settings: `deny_unknown_fields`, so a misspelled key is refused.
#[derive(Debug, DeriveDeserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// A name.
    pub name: String,
    /// A section.
    pub server: Server,
}

/// A nested section.
#[derive(Debug, DeriveDeserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Server {
    /// A host name.
    pub host: String,
    /// A port.
    pub port: u16,
}

fn main() -> Result<(), Error> {
    let formats = formats()?;

    let one: Config = formats.from_str_typed(
        "#!json\n{\"name\":\"demo\",\"server\":{\"host\":\"localhost\",\"port\":8000}}\n",
        Options::new().name("app.cfg"),
    )?;
    println!("{one:?}");

    let layered: Config = Loader::new(&formats, "toml").load_typed(
        [
            Source::text("name = 'demo'\n[server]\nhost = 'localhost'\nport = 8000\n")
                .name("base.toml"),
            Source::text("server:\n  port: 8080\n").name("override.yaml"),
        ],
        &ValueMerge,
    )?;
    println!("{layered:?}");

    let refused = formats
        .from_str_typed::<Config>(
            "name = 'demo'\n[server]\nhost = 'localhost'\nport = 'eighty'\n",
            Options::new().name("app.toml"),
        )
        .unwrap_err();
    println!("{refused}");

    let document = formats.to_vec_typed(&layered, "yaml", serde_json::value::Serializer)?;
    println!("{}", String::from_utf8_lossy(&document));
    Ok(())
}
