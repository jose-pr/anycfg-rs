use anycfg::{Error, Format, Loader, Merge, Source};
use serde::Deserialize;
use std::io::Cursor;

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Settings {
    server: Server,
    flags: Vec<String>,
    extra: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Server {
    host: String,
    port: u16,
}

#[test]
fn later_sources_deep_merge_mixed_formats_and_replace_arrays() -> Result<(), Error> {
    let config: Settings = Loader::new().load([
        Source::text("flags = []\n[server]\nhost = '127.0.0.1'\nport = 8080\n")
            .name("base.cfg"),
        Source::reader(Cursor::new(b"#!json\n{\"server\":{\"port\":9090},\"flags\":[\"a\",\"b\"]}"))
            .name("env.cfg"),
        Source::text("server:\n  host: localhost\nextra: custom\n")
            .name("local.yaml"),
    ])?;
    assert_eq!(config.server.host, "localhost");
    assert_eq!(config.server.port, 9090);
    assert_eq!(config.flags, ["a", "b"]);
    assert_eq!(config.extra.as_deref(), Some("custom"));
    Ok(())
}

#[test]
fn shallow_merge_replaces_whole_sections() -> Result<(), Error> {
    #[derive(Deserialize)]
    struct Config { server: serde_json::Value }
    let config: Config = Loader::new().merge(Merge::Shallow).load([
        Source::text("[server]\nhost = 'one'\nport = 80"),
        Source::text("{\"server\":{\"port\":443}}").format(Format::Json),
    ])?;
    assert_eq!(config.server, serde_json::json!({"port":443}));
    Ok(())
}

#[test]
fn replace_merge_uses_the_last_complete_document() -> Result<(), Error> {
    let value: serde_json::Value = Loader::new().merge(Merge::Replace).load([
        Source::text("a = 1"),
        Source::text("{\"b\":2}").format(Format::Json),
    ])?;
    assert_eq!(value, serde_json::json!({"b":2}));
    Ok(())
}

#[test]
fn total_byte_limit_applies_across_documents() {
    let error = Loader::new().max_total_bytes(16).load::<serde_json::Value, _>([
        Source::text("a = 12345"),
        Source::text("b = 67890"),
    ]).unwrap_err();
    assert!(matches!(error, Error::Source { index: 1, .. }));
    assert!(error.to_string().contains("16 bytes"));
}

#[test]
fn an_empty_loader_reports_no_sources() {
    let sources: Vec<Source<'_>> = vec![];
    let error = Loader::new().load::<serde_json::Value, _>(sources).unwrap_err();
    assert!(matches!(error, Error::NoSources));
}

#[test]
fn a_malformed_layer_reports_index_without_quoting_secrets() {
    let error = Loader::new().load::<serde_json::Value, _>([
        Source::text("good = true"),
        Source::text("password = \"secret123\"\nbad = ["),
    ]).unwrap_err();
    assert!(matches!(error, Error::Source { index: 1, .. }));
    assert!(!format!("{error:?} {error}").contains("secret123"));
}

#[test]
fn two_non_object_roots_cannot_be_shallow_merged() {
    let error = Loader::new().merge(Merge::Shallow).load::<serde_json::Value, _>([
        Source::text("[1,2]").format(Format::Json),
        Source::text("[3,4]").format(Format::Json),
    ]).unwrap_err();
    assert!(matches!(error, Error::Source { index: 1, .. }));
    assert!(error.to_string().contains("object-valued"));
}

fn nested_json(levels: usize) -> String {
    let mut input = String::new();
    for _ in 0..levels {
        input.push_str("{\"a\":");
    }
    input.push('0');
    for _ in 0..levels {
        input.push('}');
    }
    input
}

#[test]
fn every_layer_is_depth_checked_even_when_it_is_not_recursively_merged() {
    let within_limit = nested_json(64);
    let too_deep = nested_json(65);

    for policy in [Merge::Deep, Merge::Shallow, Merge::Replace] {
        let okay: serde_json::Value = Loader::new().merge(policy).load([
            Source::text(&within_limit).format(Format::Json),
        ]).unwrap();
        assert!(okay.is_object());

        let error = Loader::new().merge(policy).load::<serde_json::Value, _>([
            Source::text(&too_deep).format(Format::Json),
        ]).unwrap_err();
        assert!(matches!(error, Error::Source { index: 0, cause }
            if matches!(*cause, Error::TooDeep { limit: 64 })));
    }
}

#[test]
fn stream_reads_stop_at_the_remaining_total_byte_budget() {
    use std::cell::Cell;
    use std::io::Read;
    use std::rc::Rc;

    struct CountingReader {
        bytes: Cursor<Vec<u8>>,
        count: Rc<Cell<usize>>,
    }
    impl Read for CountingReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let n = self.bytes.read(buffer)?;
            self.count.set(self.count.get() + n);
            Ok(n)
        }
    }

    let first = b"a = 1\n";
    let read_count = Rc::new(Cell::new(0));
    let large_stream = CountingReader {
        bytes: Cursor::new(vec![b'x'; 4096]),
        count: Rc::clone(&read_count),
    };
    let result = Loader::new().max_source_bytes(4096).max_total_bytes(12)
        .load::<serde_json::Value, _>([
            Source::bytes(first),
            Source::reader(large_stream),
        ]);
    assert!(matches!(result, Err(Error::Source { index: 1, cause })
        if matches!(*cause, Error::TooLarge { limit: 12 })));
    assert!(read_count.get() <= 12 - first.len() + 1);
}
