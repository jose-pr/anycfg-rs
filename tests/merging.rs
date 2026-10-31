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
