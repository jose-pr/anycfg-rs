use std::collections::BTreeMap;

use anycfg::{Callbacks, CodecError, Error, Formats, Options};

fn formats() -> Formats<BTreeMap<String, String>> {
    let mut formats = Formats::new();
    for (name, extension, separator) in [("equals", "eq", b'='), ("colon", "co", b':')] {
        formats.register(name, [extension], Callbacks::new(
            move |data: &[u8]| {
                let input = core::str::from_utf8(data).map_err(|_| CodecError::default())?;
                let mut map = BTreeMap::new();
                for (index, line) in input.lines().enumerate() {
                    if line.is_empty() { continue; }
                    let (key, value) = line.split_once(char::from(separator))
                        .ok_or(CodecError { line: Some(index + 1), column: None })?;
                    map.insert(key.to_owned(), value.to_owned());
                }
                Ok(map)
            },
            move |map: &BTreeMap<String, String>| {
                let mut output = String::new();
                for (key, value) in map {
                    output.push_str(key);
                    output.push(char::from(separator));
                    output.push_str(value);
                    output.push('\n');
                }
                Ok(output.into_bytes())
            },
        )).unwrap();
    }
    formats.set_default("equals").unwrap();
    formats.register_marker("colon", b"%COLON\n").unwrap();
    formats
}

#[test]
fn anonymous_sources_use_the_registered_fallback_without_sniffing() {
    let registry = formats();
    let value = registry.from_slice(b"port=80\n", Options::new()).unwrap();
    assert_eq!(value["port"], "80");
    let err = registry.from_slice(b"port:80\n", Options::new()).unwrap_err();
    assert!(matches!(err, Error::Parse { .. }));
}

#[test]
fn extensions_and_shebangs_choose_only_registered_codecs() {
    let registry = formats();
    let custom = registry.from_slice(b"port:80\n", Options::new().name("app.co")).unwrap();
    assert_eq!(custom["port"], "80");
    let cfg = registry.from_slice(b"#!/usr/bin/env colon\nport:80\n", Options::new().name("app.cfg")).unwrap();
    assert_eq!(cfg["port"], "80");
    let marked = registry.from_slice(b"%COLON\nport:80\n", Options::new()).unwrap_err();
    // The marker selects the codec but stays in the payload for the codec to handle.
    assert!(matches!(marked, Error::Parse { .. }));
    assert!(matches!(registry.from_slice(b"port=80\n", Options::new().name("app.zz")), Err(Error::UnsupportedExtension)));
    assert!(matches!(registry.from_slice(b"#!not-registered\nport:80", Options::new()), Err(Error::UnsupportedFormat)));
}

#[test]
fn explicit_format_does_not_remove_a_shebang() {
    let registry = formats();
    assert!(matches!(registry.from_slice(b"#!colon\nport:80\n", Options::new().format("colon")), Err(Error::Parse { .. })));
}

#[test]
fn cfg_serialization_round_trips_with_runtime_format_selection() {
    let registry = formats();
    let map = BTreeMap::from([("port".to_owned(), "80".to_owned())]);
    let doc = registry.to_cfg_vec(&map, "colon").unwrap();
    assert!(doc.starts_with(b"#!colon\n"));
    let read = registry.from_slice(&doc, Options::new().name("config.cfg")).unwrap();
    assert_eq!(map, read);
    let default = registry.to_cfg_vec(&map, "equals").unwrap();
    assert!(!default.starts_with(b"#!"));
}

#[test]
fn registration_is_atomic_and_names_are_validated() {
    let registry = formats();
    assert_eq!(registry.to_string(&BTreeMap::new(), "equals").unwrap(), "");
    let mut registry = Formats::<Vec<u8>>::new();
    let noop = || Callbacks::new(|x: &[u8]| Ok(x.to_vec()), |x: &Vec<u8>| Ok(x.clone()));
    registry.register("alpha", ["aa"], noop()).unwrap();
    assert!(matches!(registry.register("beta", ["aa", "bb"], noop()), Err(Error::InvalidRegistration)));
    registry.register("beta", ["bb"], noop()).unwrap();
    assert!(matches!(registry.register("not valid", ["zz"], noop()), Err(Error::InvalidRegistration)));
    assert!(matches!(registry.set_default("unknown"), Err(Error::UnsupportedFormat)));
}

#[test]
fn byte_budgets_are_enforced_even_during_detection_and_stream_decoding() {
    let registry = formats();
    assert!(matches!(registry.detect(b"abcdef", Options::new().max_bytes(3)), Err(Error::TooLarge { limit: 3 })));
    assert!(matches!(registry.from_reader(&b"abcdef"[..], Options::new().max_bytes(3)), Err(Error::TooLarge { limit: 3 })));
}

#[test]
fn parse_errors_cannot_display_secrets_returned_by_user_codecs() {
    let mut registry = Formats::new();
    registry.register("conf", ["conf"], Callbacks::new(
        |_input: &[u8]| -> Result<String, CodecError> { Err(CodecError { line: Some(2), column: Some(5) }) },
        |_value: &String| Ok(Vec::new()),
    )).unwrap();
    let err = registry.from_slice(b"password=swordfish", Options::new().format("conf")).unwrap_err();
    assert!(!format!("{err:?} {err}").contains("swordfish"));
}
