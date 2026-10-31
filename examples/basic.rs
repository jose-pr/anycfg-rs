use anycfg::{from_slice, from_reader_with, to_cfg_string, Format, Options};
use serde::{Serialize, Deserialize};
use std::io::Cursor;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct App { listen: String }

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = b"#!json\n{\"listen\":\"127.0.0.1:7081\"}";
    let app: App = from_slice(bytes)?;
    let from_network: App = from_reader_with(Cursor::new(bytes), Options::new().name("ai6.cfg"))?;
    assert_eq!(app.listen, from_network.listen);
    println!("{}", to_cfg_string(&app, Format::Yaml)?);
    Ok(())
}
