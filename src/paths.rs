//! Conventional system and per-user configuration paths.
//!
//! Resolving paths does not create files, change directories or read documents.

use std::env;
use std::path::{Component, Path, PathBuf};

use crate::Error;

fn home() -> Result<PathBuf, Error> {
    env::home_dir().ok_or(Error::HomeUnavailable)
}

fn append(base: PathBuf, name: Option<&Path>) -> PathBuf {
    match name {
        Some(name) if !name.as_os_str().is_empty() => base.join(name),
        _ => base,
    }
}

/// Return the current user's configuration directory or a path beneath it.
///
/// On Windows, this uses `%APPDATA%`, then `~/AppData/Roaming`. On Unix and
/// macOS, it uses an absolute `$XDG_CONFIG_HOME`, then `~/.config`.
/// A relative XDG override is ignored.
///
/// # Errors
///
/// Returns [`Error::HomeUnavailable`] if no home can be determined for a
/// platform-specific fallback.
pub fn user_config_path(name: Option<&Path>) -> Result<PathBuf, Error> {
    let base = if cfg!(windows) {
        env::var_os("APPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .map(Ok)
            .unwrap_or_else(|| home().map(|dir| dir.join("AppData").join("Roaming")))?
    } else {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .map(Ok)
            .unwrap_or_else(|| home().map(|dir| dir.join(".config")))?
    };
    Ok(append(base, name))
}

/// Return the machine-wide configuration directory or a path beneath it.
///
/// Windows uses `%PROGRAMDATA%`, falling back to `C:\\ProgramData`.
/// Other platforms use `/etc` (not `$XDG_CONFIG_DIRS`).
#[must_use]
pub fn system_config_path(name: Option<&Path>) -> PathBuf {
    let base = if cfg!(windows) {
        env::var_os("PROGRAMDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:\\ProgramData"))
    } else {
        PathBuf::from("/etc")
    };
    append(base, name)
}

/// Find existing system and user configuration files, in overriding order.
///
/// Returns `[system, user]`; each entry is `None` if the file is absent or
/// points to a directory. The returned paths can be converted to
/// [`crate::Source::path`] and passed to [`crate::Loader::load`]. No paths are
/// opened beyond the `is_file` checks. Missing files are not errors.
///
/// `name` must be a nonempty relative filename, optionally with subdirectories.
/// An absolute path or an escaping `..` component is rejected.
///
/// # Errors
///
/// Rejects an invalid name or an unavailable home directory.
pub fn default_configs(name: impl AsRef<Path>) -> Result<[Option<PathBuf>; 2], Error> {
    let name = name.as_ref();
    if name.as_os_str().is_empty()
        || name.is_absolute()
        || name
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(Error::InvalidConfigName);
    }
    let system = system_config_path(Some(name));
    let user = user_config_path(Some(name))?;
    Ok([
        system.is_file().then_some(system),
        user.is_file().then_some(user),
    ])
}
