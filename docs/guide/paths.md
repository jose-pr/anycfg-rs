# Configuration locations

`anycfg` exposes platform-standard **system** and **user** configuration locations, modeled on the Python library. The path functions don't create directories or write files.

## Platform directories

| Platform | System base | User base |
| --- | --- | --- |
| Windows | `%PROGRAMDATA%` (fallback `C:\\ProgramData`) | `%APPDATA%` (fallback `~/AppData/Roaming`) |
| Linux / Unix | `/etc` | absolute `$XDG_CONFIG_HOME` or `~/.config` |
| macOS | `/etc` | absolute `$XDG_CONFIG_HOME` or `~/.config` |

Relative `XDG_CONFIG_HOME` values are ignored. The system directory does not use `XDG_CONFIG_DIRS`.

## Discover directories and files

~~~rust
use anycfg::{user_config_path, system_config_path};
use std::path::Path;

# fn main() -> Result<(), anycfg::Error> {
let user_dir = user_config_path(None)?;
let system_dir = system_config_path(None);
let user_file = user_config_path(Some(Path::new("myapp/settings.cfg")))?;
# let _ = (user_dir, system_dir, user_file);
# Ok(())
# }
~~~

For layered files, `default_configs(name)` returns an array in **system then user** order. Missing or non-file paths are represented as `None`.

~~~rust
use anycfg::{default_configs, Source};

# fn main() -> Result<(), anycfg::Error> {
let paths = default_configs("myapp/settings.cfg")?;

let sources: Vec<_> = paths.iter()
    .map(|p| p.as_deref().map(Source::path))
    .collect();

// Pass 'sources' to a Loader with your registered formats and merge adapter.
assert_eq!(sources.len(), 2);
# Ok(())
# }
~~~

The relative name may contain subdirectories; empty names, absolute paths and `..` traversal are rejected by `default_configs`. These checks do not restrict arbitrary `Source::path` values passed directly by an application.

## Precedence

`Loader` reads sources from first to last. For a mapping-aware merge adapter, later user settings typically override machine-wide defaults. If neither file exists, use `load_or` with a caller-provided fallback.

Directory discovery never supplies a codec: `Loader::new(&formats, "toml")` still needs a registered format and a merge adapter.

See [Sources and streams](sources.md) and [Merging layers](merging.md).
