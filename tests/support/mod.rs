//! What the integration tests share: the registry `examples/three_formats.rs` builds.

#[allow(dead_code, reason = "each test binary uses part of the example")]
#[path = "../../examples/three_formats.rs"]
pub mod three_formats;

use core::error::Error as StdError;

use anycfg::Error;

/// All an error shows: its message, its `Debug`, and each cause in its source chain.
pub fn everything_shown(error: &Error) -> String {
    let mut shown = format!("{error}\n{error:?}\n");
    let mut source = StdError::source(error);
    while let Some(cause) = source {
        shown.push_str(&format!("{cause}\n{cause:?}\n"));
        source = cause.source();
    }
    shown
}
