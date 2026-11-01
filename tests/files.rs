//! Reading a file: the format from its name or first line, and failures that name the path.

mod support;

use std::fs;
use std::path::Path;

use anycfg::{Error, Loader, Options, Source};
use support::everything_shown;
use support::three_formats::{Config, Server, ValueMerge, formats};

fn expected() -> Config {
    Config {
        name: "demo".to_owned(),
        server: Server {
            host: "h".to_owned(),
            port: 1,
        },
    }
}

fn written(directory: &Path, name: &str, content: &[u8]) -> std::path::PathBuf {
    let path = directory.join(name);
    fs::write(&path, content).unwrap();
    path
}

#[test]
fn a_typed_read_takes_the_format_from_the_files_name_or_first_line() {
    let registry = formats().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let toml = b"name = 'demo'\n[server]\nhost = 'h'\nport = 1\n".as_slice();
    let json = br#"{"name":"demo","server":{"host":"h","port":1}}"#.as_slice();
    let yaml = b"name: demo\nserver:\n  host: h\n  port: 1\n".as_slice();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("a.toml", toml.to_vec()),
        ("a.json", json.to_vec()),
        ("a.yaml", yaml.to_vec()),
        ("a.yml", yaml.to_vec()),
        ("plain.cfg", toml.to_vec()),
        ("toml.cfg", [b"#!toml\n".as_slice(), toml].concat()),
        ("json.cfg", [b"#!json\n".as_slice(), json].concat()),
        (
            "yaml.cfg",
            [b"#!/usr/bin/env YAML\n".as_slice(), yaml].concat(),
        ),
        ("yml.cfg", [b"#!.yml\n".as_slice(), yaml].concat()),
        (
            "directive.cfg",
            [b"%YAML 1.2\n---\n".as_slice(), yaml].concat(),
        ),
        (
            "bom.cfg",
            [b"\xef\xbb\xbf#!json\n".as_slice(), json].concat(),
        ),
    ];
    for (name, content) in cases {
        let path = written(directory.path(), name, &content);
        let config: Config = registry.load_typed(&path, Options::new()).unwrap();
        assert_eq!(config, expected(), "{name}");
    }
}

#[test]
fn a_missing_file_is_a_failure_naming_its_path() {
    let registry = formats().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("absent.toml");
    let error = registry
        .load_typed::<Config>(&path, Options::new())
        .unwrap_err();
    let Error::File { path: named, cause } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(named, &path);
    assert!(matches!(**cause, Error::Io(_)), "{cause:?}");
    assert!(error.to_string().starts_with(&path.display().to_string()));
}

#[test]
fn a_malformed_file_names_its_path_and_position_and_hides_the_line() {
    let registry = formats().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = written(
        directory.path(),
        "bad.cfg",
        b"#!toml\nok = 1\npassword = hunter2\n",
    );
    let error = registry
        .load_typed::<Config>(&path, Options::new())
        .unwrap_err();
    let shown = everything_shown(&error);
    assert!(!shown.contains("hunter2"), "{shown}");
    assert_eq!(
        error.to_string(),
        format!(
            "{}: could not parse toml configuration at line 3, column 12: \
             string values must be quoted, expected literal string",
            path.display()
        )
    );
}

#[test]
fn a_refused_key_in_a_file_names_the_path_and_the_key_and_hides_the_value() {
    let registry = formats().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = written(
        directory.path(),
        "bad.yaml",
        b"name: demo\nserver:\n  host: h\n  port: hunter2\n",
    );
    let error = registry
        .load_typed::<Config>(&path, Options::new())
        .unwrap_err();
    let shown = everything_shown(&error);
    assert!(!shown.contains("hunter2"), "{shown}");
    assert_eq!(
        error.to_string(),
        format!(
            "{}: invalid configuration at `server.port`: expected u16, found a string",
            path.display()
        )
    );
}

#[test]
fn a_file_over_the_byte_limit_is_refused_without_reading_it_all() {
    let registry = formats().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = written(directory.path(), "big.toml", &[b'#'; 64]);
    let error = registry
        .load(&path, Options::new().max_bytes(10))
        .unwrap_err();
    let Error::File { cause, .. } = &error else {
        panic!("{error:?}");
    };
    assert!(
        matches!(**cause, Error::TooLarge { limit: 10 }),
        "{cause:?}"
    );
}

#[test]
fn options_can_override_what_the_files_name_says() {
    let registry = formats().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = written(directory.path(), "settings.txt", b"a = 1\n");
    assert!(matches!(
        registry.load(&path, Options::new()),
        Err(Error::File { .. })
    ));
    let value = registry.load(&path, Options::new().format("toml")).unwrap();
    assert_eq!(value, serde_json::json!({"a": 1}));
    let value = registry.load(&path, Options::new().name("x.toml")).unwrap();
    assert_eq!(value, serde_json::json!({"a": 1}));
}

#[test]
fn a_loader_names_the_file_of_a_path_source() {
    let registry = formats().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let good = written(directory.path(), "good.toml", b"a = 1\n");
    let bad = written(directory.path(), "bad.toml", b"a = = hunter2\n");
    let error = Loader::new(&registry, "toml")
        .load([Source::path(&good), Source::path(&bad)], &ValueMerge)
        .unwrap_err();
    let shown = everything_shown(&error);
    assert!(!shown.contains("hunter2"), "{shown}");
    let Error::Source { index: 1, cause } = &error else {
        panic!("{error:?}");
    };
    assert!(
        matches!(&**cause, Error::File { path, .. } if *path == bad),
        "{cause:?}"
    );
}
