//! Typed reads: order, round trips, enums and options, depth, and that any input is only ever a
//! value or an error.

mod support;

use core::fmt;
use std::collections::BTreeMap;

use anycfg::{DEFAULT_MAX_DEPTH, Error, Formats, Loader, Options, ReplaceValues, Source};
use serde::de::{Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use support::three_formats::{Config, Server, ValueMerge, formats};

const FORMATS: [&str; 3] = ["toml", "json", "yaml"];

/// The keys of a map in the order a deserializer handed them over.
struct Keys(Vec<String>);

impl<'de> Deserialize<'de> for Keys {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KeysVisitor;
        impl<'de> Visitor<'de> for KeysVisitor {
            type Value = Keys;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Keys, A::Error> {
                let mut keys = Vec::new();
                while let Some((key, _)) = map.next_entry::<String, serde::de::IgnoredAny>()? {
                    keys.push(key);
                }
                Ok(Keys(keys))
            }
        }
        deserializer.deserialize_map(KeysVisitor)
    }
}

#[test]
fn a_typed_read_sees_the_documents_order_in_every_format() {
    let registry = formats().unwrap();
    for (format, document) in [
        ("toml", "zebra = 1\nalpha = 2\nmiddle = 3\n"),
        ("json", r#"{"zebra":1,"alpha":2,"middle":3}"#),
        ("yaml", "zebra: 1\nalpha: 2\nmiddle: 3\n"),
    ] {
        let keys: Keys = registry
            .from_str_typed(document, Options::new().format(format))
            .unwrap();
        assert_eq!(keys.0, ["zebra", "alpha", "middle"], "{format}");
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Everything {
    text: String,
    flag: bool,
    negative: i64,
    float: f64,
    list: Vec<u8>,
    map: BTreeMap<String, u16>,
    level: Level,
    kind: Kind,
    tagged: Tagged,
    newtype: Newtype,
    tuple: (u8, String),
    present: Option<u8>,
    #[serde(default)]
    defaulted: u8,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Level {
    Low,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Pair { left: u8, right: u8 },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Tagged {
    Named { name: String },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Newtype(u8);

fn everything() -> Everything {
    Everything {
        text: "hello".to_owned(),
        flag: true,
        negative: -3,
        float: 1.5,
        list: vec![1, 2],
        map: BTreeMap::from([("a".to_owned(), 7)]),
        level: Level::Low,
        kind: Kind::Pair { left: 1, right: 2 },
        tagged: Tagged::Named {
            name: "n".to_owned(),
        },
        newtype: Newtype(9),
        tuple: (4, "t".to_owned()),
        present: Some(5),
        defaulted: 0,
    }
}

#[test]
fn what_is_written_reads_back_in_every_format_through_each_serde_shape() {
    let registry = formats().unwrap();
    for format in FORMATS {
        let document = registry
            .to_vec_typed(&everything(), format, serde_json::value::Serializer)
            .unwrap();
        let read: Everything = registry
            .from_slice_typed(&document, Options::new().format(format))
            .unwrap_or_else(|error| {
                panic!("{format}: {error}\n{}", String::from_utf8_lossy(&document))
            });
        assert_eq!(read, everything(), "{format}");
    }
}

#[test]
fn a_none_field_reads_back_as_none_in_every_format() {
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Target {
        absent: Option<u8>,
        kept: u8,
    }
    let registry = formats().unwrap();
    let target = Target {
        absent: None,
        kept: 1,
    };
    for format in FORMATS {
        let document = registry
            .to_vec_typed(&target, format, serde_json::value::Serializer)
            .unwrap_or_else(|error| panic!("{format}: {error}"));
        let read: Target = registry
            .from_slice_typed(&document, Options::new().format(format))
            .unwrap();
        assert_eq!(read, target, "{format}");
    }
}

#[test]
fn a_cfg_file_written_in_any_format_reads_back_from_its_first_line() {
    let registry = formats().unwrap();
    let config = Config {
        name: "demo".to_owned(),
        server: Server {
            host: "h".to_owned(),
            port: 1,
        },
    };
    for format in FORMATS {
        let document = registry
            .to_auto_vec_typed(&config, format, "cfg", serde_json::value::Serializer)
            .unwrap();
        // TOML is `.cfg`'s pinned default here, so only the others carry a line.
        assert_eq!(document.starts_with(b"#!"), format != "toml", "{format}");
        let read: Config = registry
            .from_slice_typed(&document, Options::new().name("app.cfg"))
            .unwrap();
        assert_eq!(read, config, "{format}");
    }
}

#[test]
fn a_toml_datetime_arrives_as_text() {
    #[derive(Debug, Deserialize)]
    struct Target {
        at: String,
    }
    let registry = formats().unwrap();
    let target: Target = registry
        .from_str_typed("at = 1979-05-27T07:32:00Z\n", Options::new().format("toml"))
        .unwrap();
    assert_eq!(target.at, "1979-05-27T07:32:00Z");
}

fn nested_json(depth: usize) -> String {
    format!("{}{}", "[".repeat(depth), "]".repeat(depth))
}

#[test]
fn depth_is_a_setting_with_a_default_and_no_floor() {
    let registry = formats().unwrap();
    let json = Options::new().format("json");

    // `depth` brackets hold `depth - 1` levels beneath the root, which is at 0.
    let at_limit = nested_json(DEFAULT_MAX_DEPTH + 1);
    assert!(registry.from_str_typed::<Value>(&at_limit, json).is_ok());
    let beyond = nested_json(DEFAULT_MAX_DEPTH + 2);
    let error = registry.from_str_typed::<Value>(&beyond, json).unwrap_err();
    assert!(
        matches!(
            error,
            Error::TooDeep {
                limit: DEFAULT_MAX_DEPTH
            }
        ),
        "{error:?}"
    );

    let shallow = json.max_depth(1);
    assert!(registry.from_str_typed::<Value>("[[]]", shallow).is_ok());
    let error = registry
        .from_str_typed::<Value>("[[[]]]", shallow)
        .unwrap_err();
    assert!(matches!(error, Error::TooDeep { limit: 1 }), "{error:?}");

    let floorless = json.max_depth(0);
    assert!(registry.from_str_typed::<u8>("1", floorless).is_ok());
    assert!(registry.from_str_typed::<Value>("[]", floorless).is_ok());
    let error = registry
        .from_str_typed::<Value>("[1]", floorless)
        .unwrap_err();
    assert!(matches!(error, Error::TooDeep { limit: 0 }), "{error:?}");

    let raised = json.max_depth(100);
    let deep = nested_json(90);
    assert!(registry.from_str_typed::<Value>(&deep, raised).is_ok());
}

#[test]
fn a_loaders_limits_are_its_callers_settings_with_no_floor() {
    let registry = formats().unwrap();
    let loader = Loader::new(&registry, "toml").max_depth(0);
    let error = loader
        .load_typed::<_, _, _, Value>([Source::text("a = 1\n")], &ReplaceValues)
        .unwrap_err();
    assert!(matches!(error, Error::TooDeep { limit: 0 }), "{error:?}");

    let error = Loader::new(&registry, "toml")
        .max_source_bytes(0)
        .load([Source::text("a = 1\n")], &ReplaceValues)
        .unwrap_err();
    assert!(
        matches!(&error, Error::Source { cause, .. }
            if matches!(**cause, Error::TooLarge { limit: 0 })),
        "{error:?}"
    );
    let empty = Loader::new(&registry, "toml")
        .max_source_bytes(0)
        .max_total_bytes(0)
        .load([Source::text("")], &ReplaceValues)
        .unwrap();
    assert_eq!(empty, serde_json::json!({}));
}

#[test]
fn a_registry_can_be_shared_between_threads() {
    fn shareable<T: Send + Sync>(_: &T) {}
    let registry = formats().unwrap();
    shareable(&registry);
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| {
            registry
                .from_str("a = 1\n", Options::new().format("toml"))
                .unwrap()
        });
        assert_eq!(reader.join().unwrap(), serde_json::json!({"a": 1}));
    });
}

/// A small deterministic generator: a failure here must reproduce, and no test needs a crate
/// for what is only a source of awkward bytes.
struct Bytes(u64);

impl Bytes {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn document(&mut self) -> Vec<u8> {
        const PIECES: [&[u8]; 28] = [
            b"#!",
            b"json",
            b"toml",
            b"yaml",
            b"%YAML",
            b"\n",
            b"\r\n",
            b" ",
            b"=",
            b":",
            b"{",
            b"}",
            b"[",
            b"]",
            b"\"",
            b"'",
            b"key",
            b"-",
            b"1",
            b"1.5",
            b"null",
            b"\xef\xbb\xbf",
            b"\xff",
            b"\\",
            b"*a",
            b"&a",
            b"!!int",
            b"\t",
        ];
        let length = self.next() % 24;
        let mut out = Vec::new();
        for _ in 0..length {
            let piece = usize::try_from(self.next() % 28).unwrap();
            out.extend_from_slice(PIECES[piece]);
        }
        out
    }
}

fn only_a_value_or_an_error(registry: &Formats<Value>, document: &[u8], options: Options<'_>) {
    if let Err(error) = registry.from_slice_typed::<Config>(document, options) {
        assert!(
            matches!(
                error,
                Error::Parse { .. }
                    | Error::Conversion { .. }
                    | Error::UnsupportedFormat { .. }
                    | Error::TooDeep { .. }
            ),
            "{:?}: {error:?}",
            String::from_utf8_lossy(document)
        );
        assert!(!error.to_string().is_empty());
    }
}

#[test]
fn arbitrary_bytes_are_only_ever_a_value_or_a_named_error() {
    let registry = formats().unwrap();
    let mut bytes = Bytes(0x9E37_79B9_7F4A_7C15);
    for _ in 0..4000 {
        let document = bytes.document();
        only_a_value_or_an_error(&registry, &document, Options::new().name("a.cfg"));
        for format in FORMATS {
            only_a_value_or_an_error(&registry, &document, Options::new().format(format));
        }
    }
}

#[test]
fn a_merged_value_reads_into_the_target_type() {
    let registry = formats().unwrap();
    let config: Config = Loader::new(&registry, "toml")
        .load_typed(
            [
                Source::text("name = 'demo'\n[server]\nhost = 'h'\nport = 1\n").name("a.toml"),
                Source::text("server:\n  port: 2\n").name("b.yaml"),
                Source::text("#!json\n{\"server\":{\"host\":\"i\"}}").name("c.cfg"),
            ],
            &ValueMerge,
        )
        .unwrap();
    assert_eq!(config.server.host, "i");
    assert_eq!(config.server.port, 2);
}
