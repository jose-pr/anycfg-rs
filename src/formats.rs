//! The registry of a caller's formats, and how a document selects one.

use alloc::collections::BTreeMap;
use core::fmt;
use std::io::{Read, Write};

use serde::de::DeserializeOwned;
use serde::{Deserializer, Serialize, Serializer};

use crate::codec::Codec;
use crate::convert;
use crate::error::Error;

/// Default maximum size of one document (8 MiB).
pub const DEFAULT_MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
/// Default maximum nesting of one document: a typed read, and what a merge adapter is given.
pub const DEFAULT_MAX_DEPTH: usize = 64;

/// Per-input detection hints; no filesystem access occurs during detection.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Options<'a> {
    /// Explicit registered format; when present, a format line is not removed.
    pub format: Option<&'a str>,
    /// Logical source name; only its final extension is examined.
    pub name: Option<&'a str>,
    /// Default for unnamed inputs when automatic selection finds no format header.
    pub default_format: Option<&'a str>,
    /// Maximum accepted source length in bytes.
    pub max_bytes: usize,
    /// Maximum nesting a typed read accepts.
    pub max_depth: usize,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            format: None,
            name: None,
            default_format: None,
            max_bytes: DEFAULT_MAX_SOURCE_BYTES,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }
}

impl<'a> Options<'a> {
    /// Create options with [`DEFAULT_MAX_SOURCE_BYTES`] and [`DEFAULT_MAX_DEPTH`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Supply a logical filename hint.
    #[must_use]
    pub fn name(mut self, name: &'a str) -> Self {
        self.name = Some(name);
        self
    }

    /// Force a registered format name.
    #[must_use]
    pub fn format(mut self, format: &'a str) -> Self {
        self.format = Some(format);
        self
    }

    /// Choose the fallback for unnamed bytes/readers without a matching header.
    /// An auto-detect extension with its own default takes precedence.
    #[must_use]
    pub fn default_format(mut self, format: &'a str) -> Self {
        self.default_format = Some(format);
        self
    }

    /// Set the byte budget for this input.
    #[must_use]
    pub fn max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// Set the nesting a typed read of this input accepts. A root value is at depth 0.
    #[must_use]
    pub fn max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }
}

/// A registered format selection and its unmodified or header-stripped bytes.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Detection<'a> {
    /// Canonical registered format name.
    pub format: &'a str,
    /// Input without any UTF-8 BOM or selected shebang.
    pub payload: &'a [u8],
}

const SHEBANG_MAX_BYTES: usize = 256;
const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";

#[derive(Debug, Clone)]
enum ExtensionMode {
    Fixed(String),
    Auto(Option<String>),
}

/// A runtime registry of parser/serializer implementations for one value model.
///
/// The .cfg extension starts in auto-detect mode with its default supplied
/// by each loader. All other extensions and codecs are registered by callers.
pub struct Formats<V> {
    codecs: BTreeMap<String, Box<dyn Codec<V> + Send + Sync>>,
    suffixes: BTreeMap<String, ExtensionMode>,
    markers: Vec<(Vec<u8>, String)>,
    fallback: Option<String>,
}

impl<V> fmt::Debug for Formats<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Formats")
            .field("names", &self.codecs.keys().collect::<Vec<_>>())
            .field("suffixes", &self.suffixes)
            .field("fallback", &self.fallback)
            .finish_non_exhaustive()
    }
}

impl<V> Default for Formats<V> {
    fn default() -> Self {
        Self {
            codecs: BTreeMap::new(),
            suffixes: BTreeMap::from([("cfg".to_owned(), ExtensionMode::Auto(None))]),
            markers: Vec::new(),
            fallback: None,
        }
    }
}

impl<V> Formats<V> {
    /// Create an empty format registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a named handler and its supported filename extensions.
    ///
    /// The caller owns format names and aliases. To recognize `.yml`, for
    /// example, a codec needs that extension in its registration.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidRegistration`] on duplicate or invalid names.
    pub fn register<C, I, S>(&mut self, name: &str, extensions: I, codec: C) -> Result<(), Error>
    where
        C: Codec<V> + Send + Sync + 'static,
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let canonical = normalize(name)?;
        if self.codecs.contains_key(&canonical) {
            return Err(Error::InvalidRegistration);
        }
        let extensions = self.new_extensions(extensions)?;
        for ext in extensions {
            self.suffixes
                .insert(ext, ExtensionMode::Fixed(canonical.clone()));
        }
        self.codecs.insert(canonical, Box::new(codec));
        Ok(())
    }

    /// Assign a fixed codec to a file extension.
    ///
    /// The automatically registered extension can be overridden. Other
    /// previously assigned extensions cannot be overwritten.
    ///
    /// # Errors
    ///
    /// Rejects unregistered formats, invalid or conflicting extensions.
    pub fn register_extension(&mut self, extension: &str, format: &str) -> Result<(), Error> {
        let canonical = self.canonical(format)?.to_owned();
        for ext in self.new_extensions([extension])? {
            self.suffixes
                .insert(ext, ExtensionMode::Fixed(canonical.clone()));
        }
        Ok(())
    }

    /// Register an extension that checks headers and markers before its default.
    ///
    /// Can override the automatically registered extension; the other
    /// registered extensions reject collisions.
    ///
    /// # Errors
    ///
    /// Rejects unregistered defaults, invalid extensions and collisions.
    pub fn register_auto<I, S>(&mut self, extensions: I, default_format: &str) -> Result<(), Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let canonical = self.canonical(default_format)?.to_owned();
        let extensions = self.new_extensions(extensions)?;
        if extensions.is_empty() {
            return Err(Error::InvalidRegistration);
        }
        for ext in extensions {
            self.suffixes
                .insert(ext, ExtensionMode::Auto(Some(canonical.clone())));
        }
        Ok(())
    }

    /// Set the registry fallback for unnamed inputs when no per-call default exists.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFormat`] if the codec is unregistered.
    pub fn set_default(&mut self, name: &str) -> Result<(), Error> {
        self.fallback = Some(self.canonical(name)?.to_owned());
        Ok(())
    }

    /// Associate a native document prefix with a registered codec.
    ///
    /// Markers are checked for unnamed documents and auto-detect extensions.
    /// They never override an explicit format or fixed extension, and the
    /// marker stays in what the codec is given.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty or duplicate marker.
    pub fn register_marker(&mut self, name: &str, marker: &[u8]) -> Result<(), Error> {
        let canonical = self.canonical(name)?.to_owned();
        if marker.is_empty() || self.markers.iter().any(|(existing, _)| existing == marker) {
            return Err(Error::InvalidRegistration);
        }
        self.markers.push((marker.to_vec(), canonical));
        // Longest matching prefix wins; ties are rejected above.
        self.markers
            .sort_by_key(|(prefix, _)| core::cmp::Reverse(prefix.len()));
        Ok(())
    }

    /// Lowercased, undotted extensions that are valid and free to take.
    fn new_extensions<I, S>(&self, extensions: I) -> Result<Vec<String>, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let extensions: Vec<String> = extensions
            .into_iter()
            .map(|ext| normalize(ext.as_ref().trim_start_matches('.')))
            .collect::<Result<_, _>>()?;
        let taken = |ext: &String| ext != "cfg" && self.suffixes.contains_key(ext);
        let repeated = |index: usize, ext: &String| extensions[..index].contains(ext);
        if extensions
            .iter()
            .enumerate()
            .any(|(index, ext)| taken(ext) || repeated(index, ext))
        {
            return Err(Error::InvalidRegistration);
        }
        Ok(extensions)
    }

    fn canonical(&self, name: &str) -> Result<&str, Error> {
        let name = name.trim_start_matches('.');
        self.codecs
            .get_key_value(&name.to_ascii_lowercase())
            .map(|(key, _)| key.as_str())
            .ok_or_else(|| self.unsupported_format())
    }

    fn unsupported_format(&self) -> Error {
        Error::UnsupportedFormat {
            registered: self.codecs.keys().cloned().collect(),
        }
    }

    /// Select the registered codec without speculative parsing.
    ///
    /// Explicit formats and fixed extensions bypass detection. Auto-detect
    /// extensions honor registered headers/markers before their default.
    /// The pre-registered .cfg extension inherits the per-call default.
    /// Unnamed sources use that same default or the registry fallback.
    ///
    /// # Errors
    ///
    /// Rejects unknown extensions, invalid directives and oversized input.
    pub fn detect<'a>(
        &'a self,
        input: &'a [u8],
        options: Options<'_>,
    ) -> Result<Detection<'a>, Error> {
        if input.len() > options.max_bytes {
            return Err(Error::TooLarge {
                limit: options.max_bytes,
            });
        }
        let payload = input.strip_prefix(UTF8_BOM).unwrap_or(input);
        if let Some(name) = options.format {
            return Ok(Detection {
                format: self.canonical(name)?,
                payload,
            });
        }
        let mode = options
            .name
            .and_then(extension)
            .map(|ext| {
                self.suffixes
                    .get(&ext)
                    .ok_or_else(|| Error::UnsupportedExtension {
                        extension: ext.clone(),
                        registered: self.suffixes.keys().cloned().collect(),
                    })
            })
            .transpose()?;
        if let Some(ExtensionMode::Fixed(name)) = mode {
            return Ok(Detection {
                format: name,
                payload,
            });
        }
        if payload.starts_with(b"#!") {
            let end = payload
                .iter()
                .position(|b| *b == b'\n')
                .unwrap_or(payload.len());
            if end > SHEBANG_MAX_BYTES {
                return Err(self.unsupported_format());
            }
            let line =
                core::str::from_utf8(&payload[2..end]).map_err(|_| self.unsupported_format())?;
            let requested = line
                .split_ascii_whitespace()
                .last()
                .ok_or_else(|| self.unsupported_format())?;
            let requested = requested
                .rsplit(['/', '\\'])
                .next()
                .ok_or_else(|| self.unsupported_format())?;
            let format = self.canonical(requested)?;
            let tail = if end < payload.len() {
                &payload[end + 1..]
            } else {
                &payload[end..]
            };
            return Ok(Detection {
                format,
                payload: tail,
            });
        }
        if let Some((_, name)) = self
            .markers
            .iter()
            .find(|(prefix, _)| payload.starts_with(prefix))
        {
            return Ok(Detection {
                format: name,
                payload,
            });
        }
        let format = match mode {
            Some(ExtensionMode::Auto(Some(default))) => default.as_str(),
            Some(ExtensionMode::Auto(None)) | None => match options.default_format {
                Some(name) => self.canonical(name)?,
                None => self.fallback.as_deref().ok_or(Error::NoDefaultFormat)?,
            },
            Some(ExtensionMode::Fixed(_)) => unreachable!("fixed extension already returned"),
        };
        Ok(Detection { format, payload })
    }

    /// Decode bytes with the selected registered codec.
    ///
    /// # Errors
    ///
    /// Returns safe selection, size and parse errors.
    pub fn from_slice(&self, input: &[u8], options: Options<'_>) -> Result<V, Error> {
        let selected = self.detect(input, options)?;
        let codec = self
            .codecs
            .get(selected.format)
            .ok_or_else(|| self.unsupported_format())?;
        codec.decode(selected.payload).map_err(|e| Error::Parse {
            format: selected.format.to_owned(),
            line: e.line,
            column: e.column,
            message: e.message,
        })
    }

    /// Decode one registered document and deserialize its value into a Serde type.
    ///
    /// The consumer's value type must implement [`serde::Deserializer`], and be
    /// self-describing: the value is read through `deserialize_any`. The
    /// registered codec still decides how source bytes become that value; the
    /// library never converts through a particular document format.
    ///
    /// # Errors
    ///
    /// [`Error::Conversion`] names the refused key (`path`) and says what was
    /// wrong: an unknown, missing or duplicate field by name, a wrong type by
    /// what was expected and the kind found. It never quotes a value.
    /// [`Error::TooDeep`] beyond [`Options::max_depth`].
    pub fn from_slice_typed<T>(&self, input: &[u8], options: Options<'_>) -> Result<T, Error>
    where
        V: for<'de> Deserializer<'de>,
        T: DeserializeOwned,
    {
        convert::from_value(self.from_slice(input, options)?, options.max_depth)
    }

    /// Decode UTF-8 text with the selected registered codec.
    ///
    /// # Errors
    ///
    /// Returns safe selection and parse errors.
    pub fn from_str(&self, input: &str, options: Options<'_>) -> Result<V, Error> {
        self.from_slice(input.as_bytes(), options)
    }

    /// Decode UTF-8 text and deserialize its value into a Serde type.
    ///
    /// # Errors
    ///
    /// As [`Formats::from_slice_typed`].
    pub fn from_str_typed<T>(&self, input: &str, options: Options<'_>) -> Result<T, Error>
    where
        V: for<'de> Deserializer<'de>,
        T: DeserializeOwned,
    {
        self.from_slice_typed(input.as_bytes(), options)
    }

    /// Read a bounded byte stream, then decode with a registered codec.
    ///
    /// # Errors
    ///
    /// Returns a safe I/O, size, selection or parse error.
    pub fn from_reader(&self, mut input: impl Read, options: Options<'_>) -> Result<V, Error> {
        let mut bytes = Vec::new();
        let bound = u64::try_from(options.max_bytes).unwrap_or(u64::MAX - 1);
        input
            .by_ref()
            .take(bound.saturating_add(1))
            .read_to_end(&mut bytes)?;
        self.from_slice(&bytes, options)
    }

    /// Decode a bounded reader and deserialize its value into a Serde type.
    ///
    /// # Errors
    ///
    /// An I/O or size error, or as [`Formats::from_slice_typed`].
    pub fn from_reader_typed<T>(&self, input: impl Read, options: Options<'_>) -> Result<T, Error>
    where
        V: for<'de> Deserializer<'de>,
        T: DeserializeOwned,
    {
        convert::from_value(self.from_reader(input, options)?, options.max_depth)
    }

    /// Serialize a value with a named registered codec.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Serialize`] with the codec's message on failed encoding.
    pub fn to_vec(&self, value: &V, name: &str) -> Result<Vec<u8>, Error> {
        let canonical = self.canonical(name)?;
        self.codecs
            .get(canonical)
            .ok_or_else(|| self.unsupported_format())?
            .encode(value)
            .map_err(|e| Error::Serialize {
                format: canonical.to_owned(),
                message: e.message,
            })
    }

    /// Serialize any Serde type through a consumer-chosen value serializer.
    ///
    /// `serializer` converts the typed input into the same `V` used by
    /// registered codecs. The selected codec then encodes `V` as bytes.
    /// No serialization format is selected by this conversion step.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Conversion`] for failed Serde conversion, or
    /// [`Error::Serialize`] if the converted value cannot be encoded.
    pub fn to_vec_typed<T, S>(&self, value: &T, name: &str, serializer: S) -> Result<Vec<u8>, Error>
    where
        T: Serialize + ?Sized,
        S: Serializer<Ok = V>,
    {
        self.to_vec(&convert_for_writing(value, serializer)?, name)
    }

    /// Serialize a value with the native format's bytes and UTF-8 decoding.
    ///
    /// # Errors
    ///
    /// Returns an error if the codec emits non-UTF-8 bytes.
    pub fn to_string(&self, value: &V, name: &str) -> Result<String, Error> {
        String::from_utf8(self.to_vec(value, name)?).map_err(|e| Error::InvalidUtf8 {
            offset: e.utf8_error().valid_up_to(),
        })
    }

    /// Write the completely encoded value to a sink.
    ///
    /// # Errors
    ///
    /// Returns a codec or I/O error.
    pub fn to_writer(&self, mut writer: impl Write, value: &V, name: &str) -> Result<(), Error> {
        writer.write_all(&self.to_vec(value, name)?)?;
        Ok(())
    }

    /// Serialize a Serde value for a registered auto-detect extension.
    ///
    /// The supplied serializer first converts the typed input into the
    /// consumer-owned value. The output is marked when necessary.
    ///
    /// # Errors
    ///
    /// Rejects unknown/fixed extensions and failed Serde conversion.
    pub fn to_auto_vec_typed<T, S>(
        &self,
        value: &T,
        name: &str,
        extension: &str,
        serializer: S,
    ) -> Result<Vec<u8>, Error>
    where
        T: Serialize + ?Sized,
        S: Serializer<Ok = V>,
    {
        self.to_auto_vec(&convert_for_writing(value, serializer)?, name, extension)
    }

    /// Serialize for a registered auto-detect extension.
    ///
    /// If the extension has a pinned default, its native format needs no
    /// header. An unpinned extension always adds a format header so output
    /// remains self-describing regardless of the loader's default.
    ///
    /// # Errors
    ///
    /// Rejects unknown/fixed extensions and codec encoding failures.
    pub fn to_auto_vec(&self, value: &V, name: &str, extension: &str) -> Result<Vec<u8>, Error> {
        let canonical = self.canonical(name)?;
        let ext = normalize(extension.trim_start_matches('.'))?;
        let Some(ExtensionMode::Auto(default)) = self.suffixes.get(&ext) else {
            return Err(Error::UnsupportedExtension {
                extension: ext,
                registered: self.auto_extensions(),
            });
        };
        let mut bytes = self.to_vec(value, canonical)?;
        if default.as_deref() != Some(canonical) {
            let mut with_header = format!("#!{canonical}\n").into_bytes();
            with_header.append(&mut bytes);
            return Ok(with_header);
        }
        Ok(bytes)
    }

    fn auto_extensions(&self) -> Vec<String> {
        self.suffixes
            .iter()
            .filter(|(_, mode)| matches!(mode, ExtensionMode::Auto(_)))
            .map(|(extension, _)| extension.clone())
            .collect()
    }
}

fn convert_for_writing<T, S>(value: &T, serializer: S) -> Result<S::Ok, Error>
where
    T: Serialize + ?Sized,
    S: Serializer,
{
    // The serializer's error may quote the value being written.
    value.serialize(serializer).map_err(|_| Error::Conversion {
        path: None,
        message: Some("the value could not be converted for writing".to_owned()),
    })
}

fn normalize(input: &str) -> Result<String, Error> {
    let name = input.to_ascii_lowercase();
    if name.is_empty()
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(Error::InvalidRegistration);
    }
    Ok(name)
}

fn extension(name: &str) -> Option<String> {
    let final_component = name.rsplit(['/', '\\']).next().unwrap_or(name);
    final_component.rsplit_once('.').and_then(|(_, ext)| {
        if ext.is_empty() {
            None
        } else {
            Some(ext.to_ascii_lowercase())
        }
    })
}
