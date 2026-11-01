use std::collections::BTreeMap;

use anycfg::{Callbacks, CodecError, Error, Formats, Options};

fn formats() -> Formats<BTreeMap<String, String>> {
    let mut formats = Formats::new();
    for (name, extension, separator) in [("equals", "eq", b'='), ("colon", "co", b':')] {
        formats
            .register(
                name,
                [extension],
                Callbacks::new(
                    move |data: &[u8]| {
                        let input =
                            core::str::from_utf8(data).map_err(|_| CodecError::default())?;
                        let mut map = BTreeMap::new();
                        for (index, line) in input.lines().enumerate() {
                            if line.is_empty() {
                                continue;
                            }
                            let (key, value) = line
                                .split_once(char::from(separator))
                                .ok_or_else(|| CodecError::new().line(index + 1))?;
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
                ),
            )
            .unwrap();
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
    let err = registry
        .from_slice(b"port:80\n", Options::new())
        .unwrap_err();
    assert!(matches!(err, Error::Parse { .. }));
}

#[test]
fn extensions_and_format_lines_choose_only_registered_codecs() {
    let registry = formats();
    let custom = registry
        .from_slice(b"port:80\n", Options::new().name("app.co"))
        .unwrap();
    assert_eq!(custom["port"], "80");
    let cfg = registry
        .from_slice(
            b"#!/usr/bin/env colon\nport:80\n",
            Options::new().name("app.cfg"),
        )
        .unwrap();
    assert_eq!(cfg["port"], "80");
    let marked = registry
        .from_slice(b"%COLON\nport:80\n", Options::new())
        .unwrap_err();
    // The marker selects the codec but stays in the payload for the codec to handle.
    assert!(matches!(marked, Error::Parse { .. }));
    assert!(matches!(
        registry.from_slice(b"port=80\n", Options::new().name("app.zz")),
        Err(Error::UnsupportedExtension { .. })
    ));
    assert!(matches!(
        registry.from_slice(b"#!not-registered\nport:80", Options::new()),
        Err(Error::UnsupportedFormat { .. })
    ));
}

#[test]
fn explicit_format_does_not_remove_a_format_line() {
    let registry = formats();
    assert!(matches!(
        registry.from_slice(b"#!colon\nport:80\n", Options::new().format("colon")),
        Err(Error::Parse { .. })
    ));
}

#[test]
fn automatic_serialization_round_trips_with_runtime_format_selection() {
    let registry = formats();
    let map = BTreeMap::from([("port".to_owned(), "80".to_owned())]);
    let doc = registry.to_auto_vec(&map, "colon", "cfg").unwrap();
    assert!(doc.starts_with(b"#!colon\n"));
    let read = registry
        .from_slice(&doc, Options::new().name("config.cfg"))
        .unwrap();
    assert_eq!(map, read);
    let default = registry.to_auto_vec(&map, "equals", "cfg").unwrap();
    assert!(default.starts_with(b"#!equals\n"));
    assert_eq!(
        registry
            .from_slice(&default, Options::new().name("config.cfg"))
            .unwrap(),
        map
    );
}

#[test]
fn registration_is_atomic_and_names_are_validated() {
    let registry = formats();
    assert_eq!(registry.to_string(&BTreeMap::new(), "equals").unwrap(), "");
    let mut registry = Formats::<Vec<u8>>::new();
    let noop = || Callbacks::new(|x: &[u8]| Ok(x.to_vec()), |x: &Vec<u8>| Ok(x.clone()));
    registry.register("alpha", ["aa"], noop()).unwrap();
    assert!(matches!(
        registry.register("beta", ["aa", "bb"], noop()),
        Err(Error::InvalidRegistration)
    ));
    registry.register("beta", ["bb"], noop()).unwrap();
    assert!(matches!(
        registry.register("not valid", ["zz"], noop()),
        Err(Error::InvalidRegistration)
    ));
    assert!(matches!(
        registry.set_default("unknown"),
        Err(Error::UnsupportedFormat { .. })
    ));
}

#[test]
fn byte_budgets_are_enforced_even_during_detection_and_stream_decoding() {
    let registry = formats();
    assert!(matches!(
        registry.detect(b"abcdef", Options::new().max_bytes(3)),
        Err(Error::TooLarge { limit: 3 })
    ));
    assert!(matches!(
        registry.from_reader(&b"abcdef"[..], Options::new().max_bytes(3)),
        Err(Error::TooLarge { limit: 3 })
    ));
}

#[test]
fn parse_errors_cannot_display_secrets_returned_by_user_codecs() {
    let mut registry = Formats::new();
    registry
        .register(
            "conf",
            ["conf"],
            Callbacks::new(
                |_input: &[u8]| -> Result<String, CodecError> {
                    Err(CodecError::new().line(2).column(5))
                },
                |_value: &String| Ok(Vec::new()),
            ),
        )
        .unwrap();
    let err = registry
        .from_slice(b"password=swordfish", Options::new().format("conf"))
        .unwrap_err();
    assert!(!format!("{err:?} {err}").contains("swordfish"));
}

#[test]
fn cfg_is_automatically_registered_with_a_loader_supplied_default() {
    let mut registry = formats();
    let cfg = registry
        .from_slice(
            b"port:80\n",
            Options::new().name("app.cfg").default_format("colon"),
        )
        .unwrap();
    assert_eq!(cfg["port"], "80");
    let header = registry
        .from_slice(
            b"#!equals\nport=81\n",
            Options::new().name("app.cfg").default_format("colon"),
        )
        .unwrap();
    assert_eq!(header["port"], "81");

    registry.register_auto(["cfg", ".conf"], "colon").unwrap();
    let pinned = registry
        .from_slice(
            b"port:82\n",
            Options::new().name("app.cfg").default_format("equals"),
        )
        .unwrap();
    assert_eq!(pinned["port"], "82");
    let document = registry.to_auto_vec(&pinned, "colon", "cfg").unwrap();
    assert!(!document.starts_with(b"#!"));

    registry.register_extension("cfg", "equals").unwrap();
    let fixed = registry
        .from_slice(
            b"port=83\n",
            Options::new().name("app.cfg").default_format("colon"),
        )
        .unwrap();
    assert_eq!(fixed["port"], "83");
    assert!(matches!(
        registry.from_slice(b"#!colon\nport:83\n", Options::new().name("app.cfg")),
        Err(Error::Parse { .. })
    ));
    assert!(matches!(
        registry.to_auto_vec(&fixed, "equals", "cfg"),
        Err(Error::UnsupportedExtension { .. })
    ));
}

#[test]
fn any_extension_can_opt_into_auto_format_selection() {
    let mut registry = formats();
    registry.register_auto(["conf"], "colon").unwrap();
    let detected = registry
        .from_slice(b"#!equals\nport=200\n", Options::new().name("app.conf"))
        .unwrap();
    assert_eq!(detected["port"], "200");
    let fallback = registry
        .from_slice(
            b"port:201\n",
            Options::new().name("app.conf").default_format("equals"),
        )
        .unwrap();
    assert_eq!(fallback["port"], "201");
    assert!(matches!(
        registry.register_auto(["co"], "equals"),
        Err(Error::InvalidRegistration)
    ));
    assert!(matches!(
        registry.register_extension("eq", "colon"),
        Err(Error::InvalidRegistration)
    ));
    assert!(matches!(
        registry.from_slice(b"port=202\n", Options::new().name("app.unknown")),
        Err(Error::UnsupportedExtension { .. })
    ));
}

#[test]
fn unnamed_streams_use_an_explicit_default_or_registered_header() {
    let registry = formats();
    let value = registry
        .from_reader(&b"port:300\n"[..], Options::new().default_format("colon"))
        .unwrap();
    assert_eq!(value["port"], "300");
    let header = registry
        .from_slice(
            b"#!equals\nport=301\n",
            Options::new().default_format("colon"),
        )
        .unwrap();
    assert_eq!(header["port"], "301");
}
