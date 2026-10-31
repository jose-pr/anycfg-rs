use std::collections::BTreeMap;
use std::io::Read;

use anycfg::{Callbacks, CodecError, Error, Formats, Loader, Merge, MergeValues, ReplaceValues, Source};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Text(String),
    Map(BTreeMap<String, Node>),
}

fn insert(node: &mut Node, path: &str, text: String) {
    let Node::Map(map) = node else { panic!("test parser constructs map roots") };
    if let Some((head, tail)) = path.split_once('.') {
        insert(map.entry(head.to_owned()).or_insert_with(|| Node::Map(BTreeMap::new())), tail, text);
    } else {
        map.insert(path.to_owned(), Node::Text(text));
    }
}

fn registry() -> Formats<Node> {
    let mut registry = Formats::new();
    registry.register("pairs", ["pair"], Callbacks::new(
        |data: &[u8]| {
            let text = core::str::from_utf8(data).map_err(|_| CodecError::default())?;
            let mut root = Node::Map(BTreeMap::new());
            for (index, line) in text.lines().enumerate() {
                if line.is_empty() { continue; }
                let (key, value) = line.split_once('=')
                    .ok_or(CodecError { line: Some(index + 1), column: None })?;
                insert(&mut root, key, value.to_owned());
            }
            Ok(root)
        },
        |_value: &Node| Ok(Vec::new()),
    )).unwrap();
    registry.set_default("pairs").unwrap();
    registry
}

struct TreeAdapter;

impl MergeValues<Node> for TreeAdapter {
    fn validate(&self, node: &Node, limit: usize) -> Result<(), Error> {
        let mut pending = vec![(node, 0)];
        while let Some((value, depth)) = pending.pop() {
            if depth > limit { return Err(Error::TooDeep { limit }); }
            if let Node::Map(map) = value {
                pending.extend(map.values().map(|child| (child, depth + 1)));
            }
        }
        Ok(())
    }

    fn merge(&self, old: &mut Node, next: Node, strategy: Merge, max_depth: usize) -> Result<(), Error> {
        if strategy == Merge::Replace { *old = next; return Ok(()); }
        match (old, next) {
            (Node::Map(before), Node::Map(after)) => {
                for (key, value) in after {
                    if strategy == Merge::Deep {
                        if let Some(previous) = before.get_mut(&key) {
                            if matches!(previous, Node::Map(_)) && matches!(value, Node::Map(_)) {
                                self.merge(previous, value, strategy, max_depth.saturating_sub(1))?;
                            } else { *previous = value; }
                            continue;
                        }
                    }
                    before.insert(key, value);
                }
                Ok(())
            }
            _ => Err(Error::MergeShape),
        }
    }
}

#[test]
fn mixed_layers_merge_deep_without_a_library_value_type() {
    let registry = registry();
    let merged = Loader::new(&registry).load([
        Source::text("server.host=base\nserver.port=80\nname=base\n"),
        Source::text("server.port=81\nname=override\n"),
    ], &TreeAdapter).unwrap();
    let Node::Map(root) = merged else { panic!() };
    let Node::Map(server) = &root["server"] else { panic!() };
    assert_eq!(server["host"], Node::Text("base".to_owned()));
    assert_eq!(server["port"], Node::Text("81".to_owned()));
    assert_eq!(root["name"], Node::Text("override".to_owned()));
}

#[test]
fn shallow_and_replace_are_adapter_controlled() {
    let registry = registry();
    let shallow = Loader::new(&registry).merge(Merge::Shallow)
        .load([Source::text("a.x=1\n"), Source::text("a.y=2\n")], &TreeAdapter).unwrap();
    let Node::Map(map) = shallow else { panic!() };
    let Node::Map(a) = &map["a"] else { panic!() };
    assert!(!a.contains_key("x"));
    assert!(a.contains_key("y"));
    let value = Loader::new(&registry).merge(Merge::Replace)
        .load([Source::text("a.x=1\n"), Source::text("b=2\n")], &ReplaceValues).unwrap();
    let Node::Map(map) = value else { panic!() };
    assert!(!map.contains_key("a"));
}

#[test]
fn caller_can_convert_the_final_value_with_custom_callback() {
    let registry = registry();
    let port: u16 = Loader::new(&registry)
        .load_as([Source::text("port=8080\n")], &TreeAdapter, |root| {
            let Node::Map(map) = root else { return Err(()) };
            let Some(Node::Text(port)) = map.get("port") else { return Err(()) };
            port.parse().map_err(|_| ())
        }).unwrap();
    assert_eq!(port, 8080);
}

#[test]
fn absent_sources_can_be_passed_as_options() {
    let registry = registry();
    let got = Loader::new(&registry).load([
        None,
        Some(Source::text("app=one\n")),
        None,
        Some(Source::text("app=two\n")),
    ], &TreeAdapter).unwrap();
    let Node::Map(map) = got else { panic!() };
    assert_eq!(map["app"], Node::Text("two".to_owned()));
    let fallback = Loader::new(&registry).load_or([None::<Source<'_>>], &TreeAdapter, Node::Text("fallback".to_owned())).unwrap();
    assert_eq!(fallback, Node::Text("fallback".to_owned()));
}

#[test]
fn the_adapters_depth_check_runs_even_for_one_layer() {
    let registry = registry();
    let error = Loader::new(&registry).max_depth(1)
        .load([Source::text("nested.child=1\n")], &TreeAdapter).unwrap_err();
    assert!(matches!(error, Error::Source { index: 0, .. }));
}

#[derive(Clone)]
struct CountingRead(std::sync::Arc<std::sync::atomic::AtomicUsize>);
impl Read for CountingRead {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let n = buf.len().min(1);
        if n > 0 { buf[0] = b'x'; }
        Ok(n)
    }
}

#[test]
fn source_and_total_byte_limits_stop_streams() {
    let registry = registry();
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let error = Loader::new(&registry).max_total_bytes(5).max_source_bytes(10)
        .load([Source::reader(CountingRead(calls.clone()))], &TreeAdapter).unwrap_err();
    assert!(matches!(error, Error::Source { cause, .. } if matches!(*cause, Error::TooLarge { limit: 5 })));
    assert!(calls.load(std::sync::atomic::Ordering::Relaxed) <= 6);
}

#[test]
fn a_callback_can_define_merging_for_nonmap_types() {
    let mut registry = Formats::new();
    registry.register("text", ["txt"], Callbacks::new(
        |bytes: &[u8]| -> Result<String, CodecError> {
            String::from_utf8(bytes.to_vec()).map_err(|_| CodecError::default())
        },
        |value: &String| Ok(value.as_bytes().to_vec()),
    )).unwrap();
    registry.set_default("text").unwrap();
    let merged = Loader::new(&registry).load([
        Source::text("a"), Source::text("b"),
    ], &|old: &mut String, next: String, _strategy, _depth| {
        old.push_str(&next); Ok(())
    }).unwrap();
    assert_eq!(merged, "ab");
}

#[test]
fn loader_default_format_applies_to_cfg_and_unnamed_byte_streams() {
    let mut formats = Formats::new();
    formats.register("text", ["txt"], Callbacks::new(
        |input: &[u8]| String::from_utf8(input.to_vec()).map_err(|_| CodecError::default()),
        |value: &String| Ok(value.as_bytes().to_vec()),
    )).unwrap();
    formats.register("bytes", ["bin"], Callbacks::new(
        |input: &[u8]| Ok::<_, CodecError>(format!("bytes:{}", input.len())),
        |value: &String| Ok(value.as_bytes().to_vec()),
    )).unwrap();
    // No registry fallback: the loader chooses the format for unnamed data.
    let loader = Loader::new(&formats).default_format("text").merge(Merge::Replace);
    let got = loader.load([
        Source::bytes(b"one"),
        Source::reader(&b"two"[..]),
        Source::bytes(b"three").name("config.cfg"),
    ], &ReplaceValues).unwrap();
    assert_eq!(got, "three");

    let forced = loader.load([
        Source::reader(&b"four"[..]).format("bytes")
    ], &ReplaceValues).unwrap();
    assert_eq!(forced, "bytes:4");
    let named = loader.load([
        Source::bytes(b"hello").name("config.bin")
    ], &ReplaceValues).unwrap();
    assert_eq!(named, "bytes:5");
}
