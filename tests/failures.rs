//! A failure names what is wrong and shows no value from the document.

mod support;

use anycfg::{Callbacks, CodecError, Error, Formats, Loader, Merge, Options, Source};
use serde::Deserialize;
use serde::de::value::MapDeserializer;
use serde::de::{Deserializer, Visitor};
use serde_json::Value;
use support::everything_shown;
use support::three_formats::{Config, ValueMerge, formats};

const SECRET: &str = "hunter2";

fn assert_hidden(error: &Error, document: &str) {
    let shown = everything_shown(error);
    assert!(
        !shown.contains(SECRET),
        "{document:?} leaked the secret into:\n{shown}"
    );
}

/// Each of these is a syntax error in `format`, with the secret on the offending line.
fn malformed(format: &str) -> Vec<String> {
    let secret = SECRET;
    match format {
        "toml" => vec![
            format!("password = {secret}\n"),
            format!("password = \"{secret}\n"),
            format!("{secret}\n"),
            format!("password = 12{secret}\n"),
            format!("password = [1, {secret}\n"),
            format!("password = {{ x = {secret} }}\n"),
            format!("password = '{secret}' {secret}\n"),
            format!("password = \"\\q{secret}\"\n"),
            format!("password = 1979-13-45{secret}\n"),
            format!("password = \"{secret}\"\npassword = \"{secret}\"\n"),
            format!("a = '{secret}'\nb = 1 2\n"),
        ],
        "json" => vec![
            format!("{{\"password\": {secret}}}"),
            format!("{{\"password\": \"{secret}"),
            format!("{{\"password\" \"{secret}\"}}"),
            format!("{{\"password\": \"{secret}\",}}"),
            format!("{{\"password\": \"{secret}\"}} {secret}"),
            format!("{{\"password\": \"\\q{secret}\"}}"),
            format!("[1e999999{secret}]"),
            format!("{{\"password\": \"{secret}\", \"password\": \"{secret}\"}}"),
        ],
        "yaml" => vec![
            format!("password: [{secret}\n"),
            format!("password: \"{secret}\nb: 1\n"),
            format!("password: *{secret}\n"),
            format!("password: 1\nb: {{ {secret}\n"),
            format!("password: !!int {secret}\n"),
            format!("- {secret}\nb: 1\n"),
            format!("{secret}\n  : x: y\n"),
            format!("? [{secret}]\n: 1\n"),
            format!("password: {secret}\npassword: {secret}\n"),
            format!("%YAML 9.9 {secret}\n---\na: 1\n"),
            format!("password: {secret}\n---\nb: 1\n"),
        ],
        other => panic!("no documents for {other}"),
    }
}

#[test]
fn a_malformed_document_names_its_position_and_hides_its_values() {
    let registry = formats().unwrap();
    for format in ["toml", "json", "yaml"] {
        for document in malformed(format) {
            let options = Options::new().format(format);
            let error = registry.from_str(&document, options).unwrap_err();
            assert_hidden(&error, &document);
            let Error::Parse {
                format: named,
                line,
                column,
                message,
            } = &error
            else {
                panic!("{document:?}: {error:?}");
            };
            assert_eq!(named, format, "{document:?}");
            assert!(
                line.is_some() && column.is_some(),
                "{document:?}: {error:?}"
            );
            assert!(message.is_some(), "{document:?}: {error:?}");
        }
    }
}

/// The same `Config`, in each format, with `port` and then `name` holding the wrong type.
fn wrong_types(format: &str) -> Vec<String> {
    let secret = SECRET;
    match format {
        "toml" => vec![
            format!("name = 'n'\n[server]\nhost = 'h'\nport = '{secret}'\n"),
            format!("name = ['{secret}']\n[server]\nhost = 'h'\nport = 1\n"),
            format!("name = 'n'\nserver = '{secret}'\n"),
        ],
        "json" => vec![
            format!(r#"{{"name":"n","server":{{"host":"h","port":"{secret}"}}}}"#),
            format!(r#"{{"name":["{secret}"],"server":{{"host":"h","port":1}}}}"#),
            format!(r#"{{"name":"n","server":"{secret}"}}"#),
        ],
        "yaml" => vec![
            format!("name: demo\nserver:\n  host: h\n  port: {secret}\n"),
            format!("name: [{secret}]\nserver:\n  host: h\n  port: 1\n"),
            format!("name: demo\nserver: {secret}\n"),
        ],
        other => panic!("no documents for {other}"),
    }
}

#[test]
fn a_wrong_type_names_the_keys_path_and_hides_the_value() {
    let registry = formats().unwrap();
    for format in ["toml", "json", "yaml"] {
        let documents = wrong_types(format);
        let paths = ["server.port", "name", "server"];
        for (document, path) in documents.iter().zip(paths) {
            let options = Options::new().format(format);
            let error = registry
                .from_str_typed::<Config>(document, options)
                .unwrap_err();
            assert_hidden(&error, document);
            let Error::Conversion {
                path: named,
                message,
            } = &error
            else {
                panic!("{document:?}: {error:?}");
            };
            assert_eq!(named.as_deref(), Some(path), "{document:?}");
            assert!(
                message
                    .as_deref()
                    .is_some_and(|text| text.contains("expected")),
                "{document:?}: {error:?}"
            );
        }
    }
    let error = registry
        .from_str_typed::<Config>(&wrong_types("toml")[0], Options::new().name("config.toml"))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid configuration at `server.port`: expected u16, found a string"
    );
}

#[test]
fn an_unknown_field_is_named_with_where_it_was_found() {
    let registry = formats().unwrap();
    for (format, document) in [
        (
            "toml",
            "name = 'n'\n[server]\nhost = 'h'\nport = 1\ncolour = 'hunter2'\n",
        ),
        (
            "json",
            r#"{"name":"n","server":{"host":"h","port":1,"colour":"hunter2"}}"#,
        ),
        (
            "yaml",
            "name: demo\nserver:\n  host: h\n  port: 1\n  colour: hunter2\n",
        ),
    ] {
        let error = registry
            .from_str_typed::<Config>(document, Options::new().format(format))
            .unwrap_err();
        assert_hidden(&error, document);
        assert_eq!(
            error.to_string(),
            "invalid configuration at `server.colour`: unknown field `colour`, \
             expected one of `host`, `port`",
            "{format}"
        );
    }
}

#[test]
fn a_missing_field_is_named_with_where_it_belongs() {
    let registry = formats().unwrap();
    let error = registry
        .from_str_typed::<Config>(
            "name = 'hunter2'\n[server]\nhost = 'h'\n",
            Options::new().format("toml"),
        )
        .unwrap_err();
    assert_hidden(&error, "missing");
    assert_eq!(
        error.to_string(),
        "invalid configuration at `server`: missing field `port`"
    );
    let error = registry
        .from_str_typed::<Config>(
            "[server]\nhost = 'h'\nport = 1\n",
            Options::new().format("toml"),
        )
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid configuration: missing field `name`"
    );
}

#[test]
fn a_repeated_key_is_named_by_the_codec_in_each_format() {
    let registry = formats().unwrap();
    for (format, document, position) in [
        ("toml", "name = 'hunter2'\nname = 'hunter2'\n", (2, 1)),
        (
            "json",
            "{\"name\":\"hunter2\",\n\"name\":\"hunter2\"}",
            (2, 6),
        ),
        ("yaml", "name: hunter2\nname: hunter2\n", (2, 1)),
    ] {
        let error = registry
            .from_str_typed::<Config>(document, Options::new().format(format))
            .unwrap_err();
        assert_hidden(&error, document);
        let Error::Parse {
            line,
            column,
            message,
            ..
        } = &error
        else {
            panic!("{format}: {error:?}");
        };
        assert_eq!(
            (*line, *column),
            (Some(position.0), Some(position.1)),
            "{format}"
        );
        assert_eq!(message.as_deref(), Some("duplicate key `name`"), "{format}");
    }
}

/// A value model that keeps a repeated key: serde's own duplicate check then sees both.
struct Pairs(Vec<(String, String)>);

impl<'de> Deserializer<'de> for Pairs {
    type Error = serde::de::value::Error;

    fn deserialize_any<W: Visitor<'de>>(self, visitor: W) -> Result<W::Value, Self::Error> {
        visitor.visit_map(MapDeserializer::new(self.0.into_iter()))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option
        unit unit_struct newtype_struct seq tuple tuple_struct map struct enum identifier
        ignored_any
    }
}

#[test]
fn a_duplicate_field_reaching_the_target_type_is_named() {
    #[derive(Debug, Deserialize)]
    #[allow(dead_code, reason = "the read is what is under test")]
    struct Target {
        port: String,
    }
    let mut registry = Formats::new();
    registry
        .register(
            "pairs",
            ["pairs"],
            Callbacks::new(
                |_: &[u8]| {
                    Ok(Pairs(vec![
                        ("port".to_owned(), SECRET.to_owned()),
                        ("port".to_owned(), SECRET.to_owned()),
                    ]))
                },
                |_: &Pairs| Ok(Vec::new()),
            ),
        )
        .unwrap();
    let error = registry
        .from_slice_typed::<Target>(b"", Options::new().format("pairs"))
        .unwrap_err();
    assert_hidden(&error, "pairs");
    assert_eq!(
        error.to_string(),
        "invalid configuration: duplicate field `port`"
    );
}

#[test]
fn an_unknown_variant_does_not_repeat_the_value() {
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "lowercase")]
    enum Level {
        Info,
        Warn,
    }
    #[derive(Debug, Deserialize)]
    #[allow(dead_code, reason = "the read is what is under test")]
    struct Target {
        level: Level,
    }
    let registry = formats().unwrap();
    let error = registry
        .from_str_typed::<Target>("level = 'hunter2'\n", Options::new().format("toml"))
        .unwrap_err();
    assert_hidden(&error, "variant");
    assert_eq!(
        error.to_string(),
        "invalid configuration at `level`: unknown variant, expected one of `info`, `warn`"
    );
    let level: Target = registry
        .from_str_typed("level = 'warn'\n", Options::new().format("toml"))
        .unwrap();
    assert!(matches!(level.level, Level::Warn));
    assert!(matches!(
        registry
            .from_str_typed::<Target>("level = 'info'\n", Options::new().format("toml"))
            .unwrap()
            .level,
        Level::Info
    ));
}

#[test]
fn a_value_out_of_range_shows_only_its_kind() {
    #[derive(Debug, Deserialize)]
    #[allow(dead_code, reason = "the read is what is under test")]
    struct Target {
        port: u16,
    }
    let registry = formats().unwrap();
    let error = registry
        .from_str_typed::<Target>("port = 7654321\n", Options::new().format("toml"))
        .unwrap_err();
    assert!(!everything_shown(&error).contains("7654321"), "{error:?}");
    assert_eq!(
        error.to_string(),
        "invalid configuration at `port`: invalid value, expected u16, found an integer"
    );
}

#[test]
fn a_custom_serde_message_is_dropped_because_it_may_hold_anything() {
    struct Picky;
    impl<'de> Deserialize<'de> for Picky {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let text = String::deserialize(deserializer)?;
            Err(serde::de::Error::custom(format!("refusing {text}")))
        }
    }
    #[derive(Deserialize)]
    #[allow(dead_code, reason = "the read is what is under test")]
    struct Target {
        token: Picky,
    }
    let registry = formats().unwrap();
    let Err(error) =
        registry.from_str_typed::<Target>("token = 'hunter2'\n", Options::new().format("toml"))
    else {
        panic!("the custom error was not raised");
    };
    assert_hidden(&error, "custom");
    assert_eq!(
        error.to_string(),
        "invalid configuration at `token`: refused by its type"
    );
}

#[test]
fn a_control_character_in_a_refused_key_is_escaped() {
    let registry = formats().unwrap();
    let error = registry
        .from_str_typed::<Config>(r#"{"forged\nline":1}"#, Options::new().format("json"))
        .unwrap_err();
    let shown = error.to_string();
    assert!(!shown.contains('\n'), "{shown}");
    assert!(shown.contains("forged\\nline"), "{shown}");
}

#[test]
fn a_layered_typed_load_hides_values_too() {
    let registry = formats().unwrap();
    let error = Loader::new(&registry, "toml")
        .merge(Merge::Deep)
        .load_typed::<_, _, _, Config>(
            [
                Source::text("name = 'n'\n[server]\nhost = 'h'\nport = 1\n").name("base.toml"),
                Source::text("server:\n  port: hunter2\n").name("override.yaml"),
            ],
            &ValueMerge,
        )
        .unwrap_err();
    assert_hidden(&error, "layers");
    assert_eq!(
        error.to_string(),
        "invalid configuration at `server.port`: expected u16, found a string"
    );
}

#[test]
fn a_codecs_position_and_message_reach_the_parse_error() {
    let mut registry = Formats::<String>::new();
    registry
        .register(
            "conf",
            ["conf"],
            Callbacks::new(
                |_: &[u8]| Err(CodecError::new().line(2).column(5).message("expected `=`")),
                |_: &String| Err(CodecError::new().message("cannot write this")),
            ),
        )
        .unwrap();
    let error = registry
        .from_slice(b"a\nb c", Options::new().format("conf"))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "could not parse conf configuration at line 2, column 5: expected `=`"
    );
    let error = registry.to_vec(&String::new(), "conf").unwrap_err();
    assert_eq!(
        error.to_string(),
        "could not serialize conf configuration: cannot write this"
    );
}

#[test]
fn a_position_is_counted_from_a_byte_offset_in_lines_and_characters() {
    let input = "first\nsé\ncond".as_bytes();
    let at = |offset| {
        let failure = CodecError::at_offset(input, offset);
        (failure.line.unwrap(), failure.column.unwrap())
    };
    assert_eq!(at(0), (1, 1));
    assert_eq!(at(5), (1, 6));
    assert_eq!(at(6), (2, 1));
    assert_eq!(at(9), (2, 3));
    assert_eq!(at(10), (3, 1));
    assert_eq!(at(10_000), (3, 5));
}

#[test]
fn a_conversion_callback_gives_its_own_message() {
    let registry = formats().unwrap();
    let error = Loader::new(&registry, "toml")
        .load_as(
            [Source::text("port = 'hunter2'\n")],
            &ValueMerge,
            |_: Value| -> Result<u16, CodecError> {
                Err(CodecError::new().message("port must be a number"))
            },
        )
        .unwrap_err();
    assert_hidden(&error, "callback");
    assert_eq!(
        error.to_string(),
        "invalid configuration: port must be a number"
    );
}
