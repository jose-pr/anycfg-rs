use std::io::{Cursor, Read};
use anycfg::{detect, from_reader, from_reader_with, from_slice, from_slice_with, to_cfg_vec, to_vec, Error, Format, Options};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config { name: String, count: u32 }

fn expected() -> Config { Config { name: "ai6".into(), count: 3 } }

#[test]
fn cfg_defaults_to_toml_even_without_a_filename() {
    let bytes = b"name = 'ai6'\ncount = 3\n";
    assert_eq!(from_slice::<Config>(bytes).unwrap(), expected());
    assert_eq!(from_slice_with::<Config>(bytes, Options::new().name("ai6.cfg")).unwrap(), expected());
}

#[test]
fn shebangs_and_yaml_directive() {
    let samples: &[&[u8]] = &[
        b"#!json\n{\"name\":\"ai6\",\"count\":3}\n",
        b"#!/usr/bin/env yaml\nname: ai6\ncount: 3\n",
        b"#!YML\r\nname: ai6\ncount: 3\n",
        b"%YAML 1.2\n---\nname: ai6\ncount: 3\n",
        b"\xef\xbb\xbf#!toml\nname='ai6'\ncount=3\n",
    ];
    for &sample in samples { assert_eq!(from_slice::<Config>(sample).unwrap(), expected()); }
}

#[test]
fn filename_selects_format_not_parse_success() {
    let bytes = b"{\"name\":\"ai6\",\"count\":3}";
    assert!(from_slice_with::<Config>(bytes, Options::new().name("ai6.cfg")).is_err());
    assert_eq!(from_slice_with::<Config>(bytes, Options::new().name("ai6.json")).unwrap(), expected());
    assert!(matches!(from_slice_with::<Config>(bytes, Options::new().name("ai6.conf")), Err(Error::UnsupportedExtension(_))));
}

#[test]
fn caller_format_overrides_filename() {
    let bytes = b"name: ai6\ncount: 3\n";
    assert_eq!(from_slice_with::<Config>(bytes, Options::new().name("app.json").format(Format::Yaml)).unwrap(), expected());
}

#[test]
fn rejects_unknown_shebang_without_fallback() {
    assert!(matches!(from_slice::<Config>(b"#!ini\n[app]\nname = 'ai6'"), Err(Error::UnsupportedFormat(_))));
    assert!(matches!(from_slice::<Config>(b"#!\nname = 'ai6'"), Err(Error::InvalidShebang)));
}

#[test]
fn reader_supports_nonfilesystem_byte_streams_and_limits() {
    let bytes = b"#!json\n{\"name\":\"ai6\",\"count\":3}";
    assert_eq!(from_reader::<Config>(Cursor::new(bytes)).unwrap(), expected());
    assert!(matches!(from_reader_with::<Config>(Cursor::new(bytes), Options::new().max_bytes(5)), Err(Error::TooLarge { limit: 5 })));
    // Simulate a reader whose chunks are smaller than the shebang.
    struct Chunked<R>(R);
    impl<R: Read> Read for Chunked<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let size = buf.len().min(2);
            self.0.read(&mut buf[..size])
        }
    }
    assert_eq!(from_reader::<Config>(Chunked(Cursor::new(bytes))).unwrap(), expected());
}

#[test]
fn no_file_needed_for_borrowed_deserialization() {
    #[derive(Debug, Deserialize)]
    struct Borrowed<'a> { name: &'a str }
    let bytes = b"name = 'ai6'\n";
    assert_eq!(from_slice::<Borrowed<'_>>(bytes).unwrap().name, "ai6");
}

#[test]
fn writing_cfg_roundtrips_every_format() {
    for format in [Format::Toml, Format::Json, Format::Yaml] {
        let bytes = to_cfg_vec(&expected(), format).unwrap();
        assert_eq!(detect(&bytes, Options::new().name("app.cfg")).unwrap().format, format);
        assert_eq!(from_slice_with::<Config>(&bytes, Options::new().name("app.cfg")).unwrap(), expected());
        let raw = to_vec(&expected(), format).unwrap();
        assert!(!raw.starts_with(b"#!"));
    }
}

#[test]
fn parse_errors_do_not_echo_secret_value() {
    let err = from_slice::<Config>(b"name = 'MY_SECRET'\ncount = 'wrong'\n").unwrap_err();
    assert!(!format!("{err:?}").contains("MY_SECRET"));
    assert!(!format!("{err}").contains("MY_SECRET"));
}
