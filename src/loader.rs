//! Ordered configuration sources and merge policies.

use std::borrow::Cow;
use std::fs::File;
use std::io::{Read, Take};
use std::path::Path;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{Error, Format, Options, from_slice_with, suffix_format};

/// How a later configuration source affects the accumulated document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Merge {
    /// The latest document becomes the entire configuration.
    Replace,
    /// Objects merge at the top level; a later value replaces the whole earlier value.
    Shallow,
    /// Objects merge recursively; arrays, scalars and null replace earlier values.
    #[default]
    Deep,
}

/// The bytes behind one configuration source; reading is deferred until load time.
enum Input<'a> {
    Bytes(&'a [u8]),
    Reader(Box<dyn Read + 'a>),
    Path(&'a Path),
}

/// An ordered configuration layer. Its optional name is a format hint, not a path to open.
///
/// Construct with [`Source::bytes`], [`Source::text`], [`Source::reader`] or
/// [`Source::path`]; use [`Source::name`] for logical names such as `override.yaml`.
pub struct Source<'a> {
    input: Input<'a>,
    name: Option<&'a str>,
    format: Option<Format>,
}

impl core::fmt::Debug for Source<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Source")
            .field("name", &self.name)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<'a> Source<'a> {
    /// Borrow a configuration document without copying it.
    #[must_use]
    pub fn bytes(bytes: &'a [u8]) -> Self {
        Self { input: Input::Bytes(bytes), name: None, format: None }
    }

    /// Borrow a UTF-8 configuration document.
    #[must_use]
    pub fn text(text: &'a str) -> Self {
        Self::bytes(text.as_bytes())
    }

    /// Take ownership of a synchronous reader; it is read only during loading.
    #[must_use]
    pub fn reader(reader: impl Read + 'a) -> Self {
        Self { input: Input::Reader(Box::new(reader)), name: None, format: None }
    }

    /// Read a local file during loading, deriving its format hint from its name.
    #[must_use]
    pub fn path(path: &'a Path) -> Self {
        Self { input: Input::Path(path), name: None, format: None }
    }

    /// Assign a logical name, which supplies an extension without opening a file.
    #[must_use]
    pub fn name(mut self, name: &'a str) -> Self {
        self.name = Some(name);
        self
    }

    /// Select the format explicitly; this takes precedence over the name and header.
    #[must_use]
    pub fn format(mut self, format: Format) -> Self {
        self.format = Some(format);
        self
    }
}

/// Deserialize multiple ordered sources into one Serde type.
///
/// Each source selects its format separately. No files are opened unless a
/// [`Source::path`] is supplied, and no includes, commands or substitutions
/// are evaluated. Deserialization runs once, after all layers have merged.
#[derive(Debug, Clone, Copy)]
pub struct Loader {
    merge: Merge,
    max_source_bytes: usize,
    max_total_bytes: usize,
}

impl Default for Loader {
    fn default() -> Self {
        Self { merge: Merge::Deep, max_source_bytes: 8 * 1024 * 1024, max_total_bytes: 64 * 1024 * 1024 }
    }
}

impl Loader {
    /// Construct with deep merging and bounded source reads.
    #[must_use]
    pub fn new() -> Self { Self::default() }

    /// Select how later sources combine with earlier ones.
    #[must_use]
    pub fn merge(mut self, merge: Merge) -> Self { self.merge = merge; self }

    /// Maximum bytes read from any single source, including its header.
    #[must_use]
    pub fn max_source_bytes(mut self, limit: usize) -> Self { self.max_source_bytes = limit; self }

    /// Maximum combined bytes read across all layers.
    #[must_use]
    pub fn max_total_bytes(mut self, limit: usize) -> Self { self.max_total_bytes = limit; self }

    /// Parse, merge and deserialize sources in iteration order (later wins).
    ///
    /// # Errors
    ///
    /// Refuses an empty iterator, invalid source, size/depth limit, an
    /// incompatible merge root, or a document that cannot deserialize into `T`.
    /// Every source error carries its zero-based layer index.
    pub fn load<'a, T, I>(&self, sources: I) -> Result<T, Error>
    where
        T: DeserializeOwned,
        I: IntoIterator<Item = Source<'a>>,
    {
        let mut document: Option<Value> = None;
        let mut total_bytes = 0usize;
        for (index, source) in sources.into_iter().enumerate() {
            let result = self.read_source(source, &mut total_bytes);
            let next = result.map_err(|cause| Error::Source { index, cause: Box::new(cause) })?;
            if let Some(previous) = &mut document {
                merge_document(previous, next, self.merge, 0)
                    .map_err(|cause| Error::Source { index, cause: Box::new(cause) })?;
            } else {
                document = Some(next);
            }
        }
        let document = document.ok_or(Error::NoSources)?;
        serde_json::from_value(document).map_err(|_| Error::InvalidShape)
    }

    fn read_source(&self, source: Source<'_>, total_bytes: &mut usize) -> Result<Value, Error> {
        let name: Option<Cow<'_, str>> = match (&source.input, source.name) {
            (_, Some(name)) => Some(Cow::Borrowed(name)),
            (Input::Path(path), None) => Some(Cow::Owned(path.to_string_lossy().into_owned())),
            _ => None,
        };
        if source.format.is_none() {
            if let Some(name) = &name { suffix_format(name)?; }
        }
        let bytes: Cow<'_, [u8]> = match source.input {
            Input::Bytes(bytes) => Cow::Borrowed(bytes),
            Input::Reader(reader) => Cow::Owned(read_bounded(reader, self.max_source_bytes)?),
            Input::Path(path) => Cow::Owned(read_bounded(File::open(path)?, self.max_source_bytes)?),
        };
        if bytes.len() > self.max_source_bytes {
            return Err(Error::TooLarge { limit: self.max_source_bytes });
        }
        *total_bytes = total_bytes.checked_add(bytes.len())
            .ok_or(Error::TooLarge { limit: self.max_total_bytes })?;
        if *total_bytes > self.max_total_bytes {
            return Err(Error::TooLarge { limit: self.max_total_bytes });
        }
        let mut options = Options::new().max_bytes(self.max_source_bytes);
        if let Some(format) = source.format { options = options.format(format); }
        if let Some(ref name) = name { options = options.name(name); }
        from_slice_with(bytes.as_ref(), options)
    }
}

fn read_bounded(reader: impl Read, limit: usize) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    let bound = u64::try_from(limit).unwrap_or(u64::MAX - 1);
    let mut reader: Take<_> = reader.take(bound.saturating_add(1));
    reader.read_to_end(&mut bytes)?;
    if bytes.len() > limit { return Err(Error::TooLarge { limit }); }
    Ok(bytes)
}

const MAX_MERGE_DEPTH: usize = 64;

fn merge_document(previous: &mut Value, later: Value, merge: Merge, depth: usize) -> Result<(), Error> {
    if merge == Merge::Replace {
        *previous = later;
        return Ok(());
    }
    match (previous, later) {
        (Value::Object(before), Value::Object(after)) => {
            if depth >= MAX_MERGE_DEPTH { return Err(Error::TooDeep { limit: MAX_MERGE_DEPTH }); }
            for (key, value) in after {
                if merge == Merge::Deep {
                    if let Some(old) = before.get_mut(&key) {
                        if old.is_object() && value.is_object() {
                            merge_document(old, value, merge, depth + 1)?;
                        } else {
                            *old = value;
                        }
                        continue;
                    }
                }
                before.insert(key, value);
            }
            Ok(())
        }
        _ if depth == 0 => Err(Error::MergeShape),
        (before, after) => { *before = after; Ok(()) }
    }
}
