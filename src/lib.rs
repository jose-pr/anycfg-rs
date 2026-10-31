#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]


mod loader;

pub use loader::{Loader, Merge, Source};

use std::fmt;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use serde::Serialize;
use serde::de::{Deserialize, DeserializeOwned};

/// Formats included in version 0.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Format {
    /// TOML configuration.
    Toml,
    /// JSON configuration.
    Json,
    /// YAML configuration.
    Yaml,
}

impl Format {
    /// Canonical lowercase name of this format.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Toml => "toml",
            Self::Json => "json",
            Self::Yaml => "yaml",
        }
    }

    /// Parse a format name, alias, or leading-dot extension.
    pub fn parse(name: &str) -> Result<Self, Error> {
        match name.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "toml" | "cfg" => Ok(Self::Toml),
            "json" => Ok(Self::Json),
            "yaml" | "yml" => Ok(Self::Yaml),
            _ => Err(Error::UnsupportedFormat(name.to_owned())),
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Source hints don't need to correspond to any real filesystem file.
#[derive(Debug, Clone, Copy)]
pub struct Options<'a> {
    /// Explicit selection wins over all hints. In this mode a shebang is not stripped.
    pub format: Option<Format>,
    /// Optional logical name/path, used ONLY to examine the last extension.
    pub name: Option<&'a str>,
    /// Upper input size limit, including byte slices and buffered reads.
    pub max_bytes: usize,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self { format: None, name: None, max_bytes: 8 * 1024 * 1024 }
    }
}

impl<'a> Options<'a> {
    /// Create the default format detection options.
    #[must_use]
    pub fn new() -> Self { Self::default() }
    /// Supply a logical name whose suffix determines the format.
    #[must_use]
    pub fn name(mut self, name: &'a str) -> Self { self.name = Some(name); self }
    /// Select a format explicitly, without stripping a header.
    #[must_use]
    pub fn format(mut self, format: Format) -> Self { self.format = Some(format); self }
    /// Set the maximum input length, in bytes.
    #[must_use]
    pub fn max_bytes(mut self, max_bytes: usize) -> Self { self.max_bytes = max_bytes; self }
}

#[derive(Debug)]
/// Configuration failure that never contains raw document text.
#[non_exhaustive]
pub enum Error {
    /// Unknown parser named by a shebang or a caller.
    UnsupportedFormat(String),
    /// No input layers were provided.
    NoSources,
    /// Merge strategy requires mapping roots.
    MergeShape,
    /// Merged data does not match the requested Serde type.
    InvalidShape,
    /// Recursive merge exceeds the structural depth limit.
    TooDeep { limit: usize },
    /// Zero-based layer index and underlying safe failure.
    Source { index: usize, cause: Box<Error> },
    /// Unknown filename suffix.
    UnsupportedExtension(String),
    /// Invalid or oversized shebang directive.
    InvalidShebang,
    /// Invalid UTF-8 byte position.
    InvalidUtf8 { offset: usize },
    /// Input exceeded a byte budget.
    TooLarge { limit: usize },
    /// Syntax error in the selected parser, without its input excerpt.
    Parse { format: Format, line: Option<usize>, column: Option<usize> },
    /// The input value cannot be encoded as this format.
    Serialize { format: Format },
    /// Source or sink I/O failed.
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSources => f.write_str("no configuration sources supplied"),
            Self::MergeShape => f.write_str("merging requires object-valued document roots"),
            Self::InvalidShape => f.write_str("merged configuration does not match the target type"),
            Self::TooDeep { limit } => write!(f, "configuration merge exceeds {limit} levels"),
            Self::Source { index, cause } => write!(f, "configuration source {index}: {cause}"),
            Self::UnsupportedFormat(name) => write!(f, "unsupported configuration format {name:?} (available: toml, json, yaml)"),
            Self::UnsupportedExtension(name) => write!(f, "no configuration format for name {name:?} (supported: cfg, toml, json, yaml, yml)"),
            Self::InvalidShebang => write!(f, "malformed or overlong configuration shebang"),
            Self::InvalidUtf8 { offset } => write!(f, "configuration is not UTF-8 at byte {offset}"),
            Self::TooLarge { limit } => write!(f, "configuration exceeds {limit} bytes"),
            Self::Parse { format, line, column } => match (line, column) {
                (Some(line), Some(column)) => write!(f, "could not deserialize {format} configuration at {line}:{column}"),
                _ => write!(f, "could not deserialize {format} configuration"),
            },
            Self::Serialize { format } => write!(f, "could not serialize configuration as {format}"),
            Self::Io(err) => write!(f, "configuration I/O error: {err}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self { Self::Io(e) => Some(e), Self::Source { cause, .. } => Some(cause), _ => None }
    }
}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self { Self::Io(value) }
}

/// A detected format and the payload after BOM / recognized shebang removal.
#[derive(Debug, Clone, Copy)]
pub struct Detection<'de> {
    /// The parser selected for this document.
    pub format: Format,
    /// The bytes after any removed BOM or selected header.
    pub payload: &'de [u8],
}

const HEADER_LIMIT: usize = 256;
const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";

fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes)
}

fn header(bytes: &[u8]) -> Result<(Option<Format>, &[u8]), Error> {
    if bytes.starts_with(b"#!") {
        let end = bytes.iter().position(|b| *b == b'\n').unwrap_or(bytes.len());
        if end > HEADER_LIMIT { return Err(Error::InvalidShebang); }
        let first_line = std::str::from_utf8(&bytes[2..end]).map_err(|_| Error::InvalidShebang)?;
        let last = first_line.split_ascii_whitespace().last().ok_or(Error::InvalidShebang)?;
        let last = last.replace('\\', "/");
        let interpreter = last.rsplit('/').next().ok_or(Error::InvalidShebang)?;
        let format = Format::parse(interpreter)?;
        let tail = if end < bytes.len() { &bytes[end + 1..] } else { &bytes[end..] };
        return Ok((Some(format), tail));
    }
    // Native YAML directive, not a generic parse-success guess.
    if bytes.starts_with(b"%YAML ") || bytes.starts_with(b"%YAML\t") {
        return Ok((Some(Format::Yaml), bytes));
    }
    Ok((None, bytes))
}

fn suffix_format(name: &str) -> Result<Option<Format>, Error> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let ext = base.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    if ext.eq_ignore_ascii_case("cfg") { return Ok(None); }
    match ext.to_ascii_lowercase().as_str() {
        "toml" => Ok(Some(Format::Toml)),
        "json" => Ok(Some(Format::Json)),
        "yaml" | "yml" => Ok(Some(Format::Yaml)),
        _ => Err(Error::UnsupportedExtension(name.to_owned())),
    }
}

/// Detect format without trying multiple parsers, executing a shebang, or accessing a path.
/// Precedence: explicit `Options::format` > non-.cfg suffix > .cfg header > TOML.
///
/// # Errors
///
/// Returns an unsupported-format or invalid-directive error.
/// With no name, a byte stream follows `.cfg` rules.
pub fn detect<'de>(bytes: &'de [u8], options: Options<'_>) -> Result<Detection<'de>, Error> {
    if bytes.len() > options.max_bytes {
        return Err(Error::TooLarge { limit: options.max_bytes });
    }
    let bytes = strip_bom(bytes);
    if let Some(format) = options.format {
        return Ok(Detection { format, payload: bytes });
    }
    if let Some(name) = options.name {
        if let Some(format) = suffix_format(name)? {
            return Ok(Detection { format, payload: bytes });
        }
    }
    let (format, payload) = header(bytes)?;
    Ok(Detection { format: format.unwrap_or(Format::Toml), payload })
}

/// Directly deserialize from bytes, preserving borrowed fields in `T` where supported.
/// With no name or format, the stream follows `.cfg` rules and defaults to TOML.
///
/// # Errors
///
/// Returns an error for malformed UTF-8, invalid format, or invalid Serde shape.
pub fn from_slice<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, Error> {
    from_slice_with(bytes, Options::default())
}

/// Deserialize a byte slice with explicit detection and size settings.
///
/// # Errors
///
/// Returns a safe configuration error on invalid input or failed serialization.
pub fn from_slice_with<'de, T: Deserialize<'de>>(bytes: &'de [u8], options: Options<'_>) -> Result<T, Error> {
    if bytes.len() > options.max_bytes {
        return Err(Error::TooLarge { limit: options.max_bytes });
    }
    let selected = detect(bytes, options)?;
    let text = std::str::from_utf8(selected.payload)
        .map_err(|e| Error::InvalidUtf8 { offset: e.valid_up_to() })?;
    let parse_err = || Error::Parse { format: selected.format, line: None, column: None };
    match selected.format {
        Format::Toml => toml::from_str(text).map_err(|_| parse_err()),
        Format::Json => serde_json::from_slice(selected.payload).map_err(|e| Error::Parse {
            format: Format::Json, line: Some(e.line()), column: Some(e.column())
        }),
        Format::Yaml => serde_saphyr::from_slice(selected.payload).map_err(|_| parse_err()),
    }
}

/// Deserialize UTF-8 text using anonymous `.cfg` detection.
///
/// # Errors
///
/// Returns a safe configuration error on invalid input or failed serialization.
pub fn from_str<'de, T: Deserialize<'de>>(text: &'de str) -> Result<T, Error> {
    from_slice(text.as_bytes())
}

/// Deserialize UTF-8 text with format hints.
///
/// # Errors
///
/// Returns a safe configuration error on invalid input or failed serialization.
pub fn from_str_with<'de, T: Deserialize<'de>>(text: &'de str, options: Options<'_>) -> Result<T, Error> {
    from_slice_with(text.as_bytes(), options)
}

/// Buffered, bounded stream decoding, for network streams and other `Read` sources.
/// Unlike `from_slice`, stream decoding returns owned data.
///
/// # Errors
///
/// Returns an error when the read budget is exceeded or decoding fails.
pub fn from_reader<T: DeserializeOwned>(reader: impl Read) -> Result<T, Error> {
    from_reader_with(reader, Options::default())
}

/// Deserialize a bounded byte stream with format hints.
///
/// # Errors
///
/// Returns a safe configuration error on invalid input or failed serialization.
pub fn from_reader_with<T: DeserializeOwned>(reader: impl Read, options: Options<'_>) -> Result<T, Error> {
    let mut bytes = Vec::new();
    let byte_limit = u64::try_from(options.max_bytes).unwrap_or(u64::MAX - 1);
    reader.take(byte_limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() > options.max_bytes {
        return Err(Error::TooLarge { limit: options.max_bytes });
    }
    from_slice_with(&bytes, options)
}

/// Optional filesystem convenience wrapper; bytes/streams are the core API.
/// Read and deserialize a path using its extension.
///
/// # Errors
///
/// Returns a safe configuration error on invalid input or failed serialization.
pub fn from_path<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T, Error> {
    let path = path.as_ref();
    let name = path.to_string_lossy();
    from_reader_with(File::open(path)?, Options::default().name(&name))
}

/// Pure format serialization: does not add shebangs.
///
/// # Errors
///
/// Returns an error if the value is unrepresentable in the format.
pub fn to_string<T: Serialize>(value: &T, format: Format) -> Result<String, Error> {
    let mut text = match format {
        Format::Toml => toml::to_string_pretty(value).map_err(|_| Error::Serialize { format })?,
        Format::Json => serde_json::to_string_pretty(value).map_err(|_| Error::Serialize { format })?,
        Format::Yaml => serde_saphyr::to_string(value).map_err(|_| Error::Serialize { format })?,
    };
    if !text.ends_with('\n') { text.push('\n'); }
    Ok(text)
}

/// Serialize a value as UTF-8 bytes in a named format.
///
/// # Errors
///
/// Returns a safe configuration error on invalid input or failed serialization.
pub fn to_vec<T: Serialize>(value: &T, format: Format) -> Result<Vec<u8>, Error> {
    Ok(to_string(value, format)?.into_bytes())
}

/// Generate a self-describing `.cfg` document: TOML by default; JSON/YAML with a marker.
///
/// # Errors
///
/// Returns an error if the value is unrepresentable in the format.
pub fn to_cfg_string<T: Serialize>(value: &T, format: Format) -> Result<String, Error> {
    let text = to_string(value, format)?;
    if format == Format::Toml { Ok(text) } else { Ok(format!("#!{}\n{text}", format.name())) }
}

/// Serialize a self-describing `.cfg` document as UTF-8 bytes.
///
/// # Errors
///
/// Returns a safe configuration error on invalid input or failed serialization.
pub fn to_cfg_vec<T: Serialize>(value: &T, format: Format) -> Result<Vec<u8>, Error> {
    Ok(to_cfg_string(value, format)?.into_bytes())
}

/// Writes complete serialized data to a `Write` sink (no implicit shebang).
///
/// # Errors
///
/// Returns an error for invalid serialization or failed I/O.
pub fn to_writer<T: Serialize>(mut writer: impl Write, value: &T, format: Format) -> Result<(), Error> {
    let encoded = to_vec(value, format)?;
    writer.write_all(&encoded)?;
    Ok(())
}

/// Writes a `.cfg` document (TOML or tagged JSON/YAML) to any byte writer.
///
/// # Errors
///
/// Returns an error for invalid serialization or failed I/O.
pub fn to_cfg_writer<T: Serialize>(mut writer: impl Write, value: &T, format: Format) -> Result<(), Error> {
    let encoded = to_cfg_vec(value, format)?;
    writer.write_all(&encoded)?;
    Ok(())
}
