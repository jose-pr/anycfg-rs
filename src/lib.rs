#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations)]
#![warn(
    clippy::std_instead_of_core,
    clippy::std_instead_of_alloc,
    clippy::alloc_instead_of_core
)]

extern crate alloc;

mod codec;
mod convert;
mod error;
mod formats;
mod loader;
mod paths;

pub use codec::{Callbacks, Codec, CodecError};
pub use error::Error;
pub use formats::{DEFAULT_MAX_DEPTH, DEFAULT_MAX_SOURCE_BYTES, Detection, Formats, Options};
pub use loader::{DEFAULT_MAX_TOTAL_BYTES, Loader, Merge, MergeValues, ReplaceValues, Source};
pub use paths::{Search, default_configs, system_config_path, user_config_path};
