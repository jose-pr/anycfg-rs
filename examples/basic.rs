use anycfg::{Callbacks, CodecError, Formats, Loader, Merge, ReplaceValues, Source};

fn main() -> Result<(), anycfg::Error> {
    let mut formats = Formats::new();
    formats.register("plain", ["txt"], Callbacks::new(
        |input: &[u8]| String::from_utf8(input.to_vec()).map_err(|_| CodecError::default()),
        |value: &String| Ok(value.as_bytes().to_vec()),
    ))?;
    formats.set_default("plain")?;

    let result = Loader::new(&formats, "plain")
        .merge(Merge::Replace)
        .load([Source::text("base"), Source::text("user")], &ReplaceValues)?;
    println!("{result}");
    Ok(())
}
