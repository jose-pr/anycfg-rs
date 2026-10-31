#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations)]

mod formats;
mod loader;
mod paths;

pub use formats::{Callbacks, Codec, CodecError, Detection, Formats, Options};
pub use loader::{Loader, Merge, MergeValues, ReplaceValues, Source};
pub use paths::{default_configs, system_config_path, user_config_path};

use core::fmt;
use std::io;

/// Default maximum size of one document (8 MiB).
pub const DEFAULT_MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
/// Default maximum size of all documents combined (64 MiB).
pub const DEFAULT_MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
/// Default maximum depth passed to a caller's value adapter.
pub const DEFAULT_MAX_DEPTH: usize = 64;

/// Errors returned by format selection, file lookup and source loading.
///
/// Parser and conversion diagnostics are intentionally sanitized. In particular,
/// raw configuration values and parser excerpts never appear in errors.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// No formats have been registered as the fallback for `.cfg` files.
    NoDefaultFormat,
    /// A format was requested but no matching codec was registered.
    UnsupportedFormat,
    /// A filename extension has no codec registered for it.
    UnsupportedExtension,
    /// A format name or extension is invalid or already in use.
    InvalidRegistration,
    /// A shebang is malformed or exceeds the scanner limit.
    InvalidShebang,
    /// Input bytes do not encode valid UTF-8 where UTF-8 is required.
    InvalidUtf8 {
        /// Byte offset of the first invalid UTF-8 sequence.
        offset: usize,
    },
    /// A document exceeded the configured byte limit.
    TooLarge {
        /// Maximum permitted document or input size, in bytes.
        limit: usize,
    },
    /// A value adapter rejected the document's depth.
    TooDeep {
        /// Maximum permitted nesting depth provided to the value adapter.
        limit: usize,
    },
    /// No present sources were supplied.
    NoSources,
    /// A selected merge policy could not merge the values.
    MergeShape,
    /// The caller's value conversion failed.
    Conversion,
    /// A registered decoder rejected the document.
    Parse {
        /// Registered codec name that rejected the document.
        format: String,
        /// Optional one-based source line reported by the codec.
        line: Option<usize>,
        /// Optional one-based source column reported by the codec.
        column: Option<usize>,
    },
    /// A registered encoder rejected the value.
    Serialize {
        /// Registered codec name that could not encode the value.
        format: String,
    },
    /// A source failed to read, decode, validate or merge.
    Source {
        /// Zero-based index of the failing source in the input iterator.
        index: usize,
        /// Error raised while reading, decoding, validating, or merging this source.
        cause: Box<Error>,
    },
    /// The user's home directory could not be determined.
    HomeUnavailable,
    /// A default configuration filename was missing or invalid.
    InvalidConfigName,
    /// Reading from or writing to a source failed.
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDefaultFormat => f.write_str("no default configuration format registered"),
            Self::UnsupportedFormat => f.write_str("configuration format not registered"),
            Self::UnsupportedExtension => f.write_str("configuration extension not registered"),
            Self::InvalidRegistration => f.write_str("invalid or duplicate format registration"),
            Self::InvalidShebang => f.write_str("invalid configuration shebang"),
            Self::InvalidUtf8 { offset } => write!(f, "configuration has invalid UTF-8 at byte {offset}"),
            Self::TooLarge { limit } => write!(f, "configuration exceeds {limit} bytes"),
            Self::TooDeep { limit } => write!(f, "configuration exceeds {limit} value levels"),
            Self::NoSources => f.write_str("no configuration sources supplied"),
            Self::MergeShape => f.write_str("configuration values cannot be merged"),
            Self::Conversion => f.write_str("configuration value conversion failed"),
            Self::Parse { format, line, column } => match (line, column) {
                (Some(line), Some(column)) => write!(f, "could not parse {format} configuration at {line}:{column}"),
                _ => write!(f, "could not parse {format} configuration"),
            },
            Self::Serialize { format } => write!(f, "could not serialize {format} configuration"),
            Self::Source { index, cause } => write!(f, "configuration source {index}: {cause}"),
            Self::HomeUnavailable => f.write_str("could not locate the user home directory"),
            Self::InvalidConfigName => f.write_str("configuration filename must be nonempty and relative"),
            Self::Io(err) => write!(f, "configuration I/O error: {err}"),
        }
    }
}

impl core::error::Error for Error {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Source { cause, .. } => Some(cause),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}
