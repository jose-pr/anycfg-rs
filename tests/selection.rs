//! How a document selects its format: `.cfg` format lines, markers, extensions, the BOM.

mod support;

use anycfg::{Error, Formats, Options};
use serde_json::{Value, json};
use support::three_formats::formats;

const TOML_BODY: &str = "a = 1\n";
const JSON_BODY: &str = "{\"a\": 1}\n";
const YAML_BODY: &str = "a: 1\n";

fn read_cfg(registry: &Formats<Value>, document: &str) -> Result<Value, Error> {
    registry.from_str(document, Options::new().name("app.cfg"))
}

#[test]
fn a_cfg_first_line_names_its_format_and_is_not_part_of_the_document() {
    let registry = formats().unwrap();
    for (line, body) in [
        ("#!json", JSON_BODY),
        ("#!yaml", YAML_BODY),
        ("#!yml", YAML_BODY),
        ("#!toml", TOML_BODY),
        ("#!/usr/bin/env yaml", YAML_BODY),
        ("#!/usr/bin/env json", JSON_BODY),
        ("#!/opt/bin/toml", TOML_BODY),
        ("#!C:\\tools\\yaml", YAML_BODY),
        ("#! json", JSON_BODY),
        ("#!JSON", JSON_BODY),
        ("#!Yaml", YAML_BODY),
        ("#!.json", JSON_BODY),
        ("#!..yml", YAML_BODY),
        ("#!/usr/bin/env .TOML  ", TOML_BODY),
    ] {
        for newline in ["\n", "\r\n"] {
            let document = format!("{line}{newline}{body}");
            let value = read_cfg(&registry, &document).unwrap_or_else(|e| panic!("{line}: {e}"));
            assert_eq!(value, json!({"a": 1}), "{line}");
        }
    }
}

#[test]
fn a_format_line_alone_is_an_empty_document_of_that_format() {
    let registry = formats().unwrap();
    assert_eq!(read_cfg(&registry, "#!toml").unwrap(), json!({}));
    assert_eq!(read_cfg(&registry, "#!toml\n").unwrap(), json!({}));
}

#[test]
fn a_document_starting_with_the_yaml_directive_is_yaml_and_keeps_it() {
    let registry = formats().unwrap();
    let value = read_cfg(&registry, "%YAML 1.2\n---\na: 1\n").unwrap();
    assert_eq!(value, json!({"a": 1}));
}

#[test]
fn anything_else_is_the_default_format_and_nothing_is_guessed_from_the_content() {
    let registry = formats().unwrap();
    assert_eq!(read_cfg(&registry, TOML_BODY).unwrap(), json!({"a": 1}));
    assert_eq!(read_cfg(&registry, "").unwrap(), json!({}));
    for looks_like_another in [JSON_BODY, "a: 1\n", "---\na: 1\n", "# a: 1\nb: 2\n"] {
        let error = read_cfg(&registry, looks_like_another).unwrap_err();
        assert!(
            matches!(&error, Error::Parse { format, .. } if format == "toml"),
            "{looks_like_another:?}: {error:?}"
        );
    }
}

#[test]
fn a_name_no_format_answers_to_lists_the_registered_formats_without_repeating_it() {
    let registry = formats().unwrap();
    for document in [
        "#!hunter2\na = 1\n",
        "#!/usr/bin/env\na = 1\n",
        "#!ini\n",
        "#!\na = 1\n",
        "#!   \n",
        "#!/usr/bin/\n",
        "#!cfg\n",
    ] {
        let error = read_cfg(&registry, document).unwrap_err();
        let Error::UnsupportedFormat { registered } = &error else {
            panic!("{document:?}: {error:?}");
        };
        assert_eq!(registered, &["json", "toml", "yaml"], "{document:?}");
        assert!(!format!("{error} {error:?}").contains("hunter2"), "{error}");
        assert!(error.to_string().contains("json, toml, yaml"), "{error}");
    }
    let invalid = registry.from_slice(b"#!\xff\xfe\na = 1\n", Options::new().name("app.cfg"));
    assert!(matches!(invalid, Err(Error::UnsupportedFormat { .. })));
}

#[test]
fn a_parse_error_after_a_format_line_counts_the_documents_own_lines() {
    let registry = formats().unwrap();
    let error = read_cfg(&registry, "#!json\n{\n\"a\" 1}\n").unwrap_err();
    assert!(
        matches!(error, Error::Parse { line: Some(3), .. }),
        "{error:?}"
    );
    let error = read_cfg(&registry, "\u{feff}#!toml\nb = 1\na = = 2\n").unwrap_err();
    assert!(
        matches!(error, Error::Parse { line: Some(3), .. }),
        "{error:?}"
    );
}

#[test]
fn one_leading_byte_order_mark_is_ignored_in_every_selection_path() {
    let registry = formats().unwrap();
    let bom = "\u{feff}";
    let cases: [(String, Options<'_>); 5] = [
        (format!("{bom}{TOML_BODY}"), Options::new().name("app.toml")),
        (format!("{bom}{TOML_BODY}"), Options::new().name("app.cfg")),
        (
            format!("{bom}#!json\n{JSON_BODY}"),
            Options::new().name("app.cfg"),
        ),
        (
            format!("{bom}%YAML 1.2\n---\n{YAML_BODY}"),
            Options::new().name("app.cfg"),
        ),
        (format!("{bom}{JSON_BODY}"), Options::new().format("json")),
    ];
    for (document, options) in cases {
        let value = registry
            .from_str(&document, options)
            .unwrap_or_else(|e| panic!("{document:?}: {e}"));
        assert_eq!(value, json!({"a": 1}), "{document:?}");
    }
}

#[test]
fn only_one_byte_order_mark_is_ignored() {
    let registry = formats().unwrap();
    let document = "\u{feff}\u{feff}{\"a\": 1}";
    let selected = registry
        .detect(document.as_bytes(), Options::new().format("json"))
        .unwrap();
    assert_eq!(selected.payload, "\u{feff}{\"a\": 1}".as_bytes());
    assert!(matches!(
        registry.from_str(document, Options::new().format("json")),
        Err(Error::Parse { .. })
    ));
}

#[test]
fn a_fixed_extension_and_an_explicit_format_do_not_read_the_first_line() {
    let registry = formats().unwrap();
    let toml = registry
        .from_str("#!json\na = 1\n", Options::new().name("app.toml"))
        .unwrap();
    assert_eq!(toml, json!({"a": 1}));
    let forced = registry
        .from_str(
            "#!json\na: 1\n",
            Options::new().name("app.cfg").format("yaml"),
        )
        .unwrap();
    assert_eq!(forced, json!({"a": 1}));
    let selected = registry
        .detect(b"#!json\n", Options::new().format("toml"))
        .unwrap();
    assert_eq!(selected.payload, b"#!json\n");
    assert_eq!(selected.lines_removed, 0);
}

#[test]
fn a_format_answers_to_its_extensions_wherever_a_name_is_taken() {
    let registry = formats().unwrap();
    let selected = registry
        .detect(b"a: 1\n", Options::new().format(".YML"))
        .unwrap();
    assert_eq!(selected.format, "yaml");
    let selected = registry
        .detect(b"a: 1\n", Options::new().default_format("yml"))
        .unwrap();
    assert_eq!(selected.format, "yaml");
}

#[test]
fn extensions_choose_their_format_ignoring_case_and_an_unknown_one_lists_them() {
    let registry = formats().unwrap();
    for (name, body) in [
        ("a.toml", TOML_BODY),
        ("a.JSON", JSON_BODY),
        ("a.yaml", YAML_BODY),
        ("dir.d/a.yml", YAML_BODY),
    ] {
        let value = registry.from_str(body, Options::new().name(name)).unwrap();
        assert_eq!(value, json!({"a": 1}), "{name}");
    }
    let error = registry
        .from_str(TOML_BODY, Options::new().name("a.ini"))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "configuration extension `.ini` is not registered \
         (registered: cfg, json, toml, yaml, yml)"
    );
}

#[test]
fn a_name_with_no_extension_is_selected_like_an_unnamed_input() {
    let registry = formats().unwrap();
    for name in ["ai6", ".ai6", "dir.d/ai6", "ai6."] {
        assert!(
            matches!(
                registry.from_str(TOML_BODY, Options::new().name(name)),
                Err(Error::NoDefaultFormat)
            ),
            "{name}"
        );
        let value = registry
            .from_str(TOML_BODY, Options::new().name(name).default_format("toml"))
            .unwrap();
        assert_eq!(value, json!({"a": 1}), "{name}");
    }
}

#[test]
fn a_long_format_line_has_no_limit_of_its_own() {
    let registry = formats().unwrap();
    let document = format!("#!{}/json\n{JSON_BODY}", "x/".repeat(10_000));
    assert_eq!(read_cfg(&registry, &document).unwrap(), json!({"a": 1}));
}

#[test]
fn the_byte_limit_has_no_floor_and_is_checked_before_the_format_line() {
    let registry = formats().unwrap();
    let selected = registry
        .detect(b"", Options::new().format("toml").max_bytes(0))
        .unwrap();
    assert_eq!(selected.payload, b"");
    assert!(matches!(
        registry.detect(b"#!x", Options::new().name("a.cfg").max_bytes(2)),
        Err(Error::TooLarge { limit: 2 })
    ));
}
