//! Where configuration is looked for. Each test runs a child process with the environment it is
//! testing, so none depends on this machine's or changes this process's.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anycfg::{Error, Search, default_configs, system_config_path, user_config_path};
use tempfile::TempDir;

const VARIABLE: &str = "ANYCFG_TEST_CONFIG";
const NAME: &str = "app/settings.cfg";

/// A directory standing in for the home, user and system directories, and a working directory.
struct World {
    root: TempDir,
}

impl World {
    fn new() -> Self {
        let world = Self {
            root: tempfile::tempdir().unwrap(),
        };
        for directory in ["home", "user", "system", "work"] {
            fs::create_dir(world.path(directory)).unwrap();
        }
        world
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    fn create(&self, relative: &str) -> PathBuf {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "").unwrap();
        path
    }

    /// What `child_reports` prints when run in this world.
    fn run(&self, mode: &str, configure: impl FnOnce(&mut Command)) -> String {
        let mut child = Command::new(env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "child_reports",
                "--include-ignored",
                "--nocapture",
            ])
            .env("ANYCFG_TEST_MODE", mode)
            .env_remove(VARIABLE)
            .env_remove("ANYCFG_TEST_EXPLICIT")
            .env("HOME", self.path("home"))
            .env("USERPROFILE", self.path("home"))
            .env("XDG_CONFIG_HOME", self.path("user"))
            .env("APPDATA", self.path("user"))
            .env("PROGRAMDATA", self.path("system"))
            .current_dir(self.path("work"));
        configure(&mut child);
        let output = child.output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(output.status.success(), "{stdout}");
        stdout
            .lines()
            .find_map(|line| line.strip_prefix("report: "))
            .unwrap_or_else(|| panic!("no report in {stdout}"))
            .replace('\\', "/")
    }

    fn search(&self, configure: impl FnOnce(&mut Command)) -> String {
        self.run("search", configure)
    }
}

/// A path as a report shows it: joining `app/settings.cfg` on Windows mixes the separators.
fn shown(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn found(path: &Path) -> String {
    format!("found {}", shown(path))
}

/// The other side of [`World::run`]; `--include-ignored` runs it there.
#[test]
#[ignore = "run as a child process by the other tests in this file"]
fn child_reports() {
    let explicit = env::var_os("ANYCFG_TEST_EXPLICIT").map(PathBuf::from);
    let report = match env::var("ANYCFG_TEST_MODE").unwrap().as_str() {
        "search" => {
            let search = Search::new(NAME)
                .explicit(explicit.as_deref())
                .variable(VARIABLE);
            match search.find() {
                Ok(Some(path)) => found(&path),
                Ok(None) => "nothing".to_owned(),
                Err(error) => format!("error {error}"),
            }
        }
        "user" => user_config_path(Some(Path::new(NAME)))
            .unwrap()
            .display()
            .to_string(),
        "system" => system_config_path(Some(Path::new(NAME)))
            .display()
            .to_string(),
        "defaults" => {
            let [system, user] = default_configs(NAME).unwrap();
            let label = |path: Option<PathBuf>| path.map_or("none".to_owned(), |path| shown(&path));
            format!("system {} user {}", label(system), label(user))
        }
        other => panic!("no such mode: {other}"),
    };
    println!("report: {report}");
}

#[test]
fn nothing_anywhere_is_not_a_failure() {
    let world = World::new();
    assert_eq!(world.search(|_| ()), "nothing");
}

#[test]
fn the_working_directory_is_never_searched() {
    let world = World::new();
    world.create("work/app/settings.cfg");
    world.create("work/settings.cfg");
    assert_eq!(world.search(|_| ()), "nothing");
}

#[test]
fn the_users_file_is_found() {
    let world = World::new();
    let user = world.create("user/app/settings.cfg");
    assert_eq!(world.search(|_| ()), found(&user));
}

// Only Windows lets the environment choose the system directory; a unit test in `src/paths.rs`
// holds the order on every platform.
#[cfg(windows)]
#[test]
fn the_systems_file_is_found_alone_and_the_users_comes_before_it() {
    let world = World::new();
    let system = world.create("system/app/settings.cfg");
    assert_eq!(world.search(|_| ()), found(&system));
    let user = world.create("user/app/settings.cfg");
    assert_eq!(world.search(|_| ()), found(&user));
}

#[test]
fn the_variables_file_comes_before_the_users() {
    let world = World::new();
    world.create("user/app/settings.cfg");
    let named = world.create("named.toml");
    let report = world.search(|child| {
        child.env(VARIABLE, &named);
    });
    assert_eq!(report, found(&named));
}

#[test]
fn an_empty_variable_counts_as_unset() {
    let world = World::new();
    let user = world.create("user/app/settings.cfg");
    let report = world.search(|child| {
        child.env(VARIABLE, "");
    });
    assert_eq!(report, found(&user));
}

#[test]
fn a_missing_file_the_variable_names_is_an_error_naming_both_even_with_a_users_file() {
    let world = World::new();
    world.create("user/app/settings.cfg");
    let missing = world.path("missing.toml");
    let report = world.search(|child| {
        child.env(VARIABLE, &missing);
    });
    assert_eq!(
        report,
        format!(
            "error no configuration file at {}, which {VARIABLE} names",
            shown(&missing)
        )
    );
}

#[test]
fn the_explicit_file_comes_before_the_variables() {
    let world = World::new();
    let explicit = world.create("explicit.toml");
    let named = world.create("named.toml");
    let report = world.search(|child| {
        child
            .env(VARIABLE, &named)
            .env("ANYCFG_TEST_EXPLICIT", &explicit);
    });
    assert_eq!(report, found(&explicit));
}

#[test]
fn a_missing_explicit_file_is_an_error_naming_it_even_with_others_present() {
    let world = World::new();
    world.create("user/app/settings.cfg");
    let named = world.create("named.toml");
    let missing = world.path("missing.toml");
    let report = world.search(|child| {
        child
            .env(VARIABLE, &named)
            .env("ANYCFG_TEST_EXPLICIT", &missing);
    });
    assert_eq!(
        report,
        format!("error no configuration file at {}", shown(&missing))
    );
}

#[test]
fn a_directory_is_not_a_file() {
    let world = World::new();
    let directory = world.path("user");
    let report = world.search(|child| {
        child.env("ANYCFG_TEST_EXPLICIT", &directory);
    });
    assert_eq!(
        report,
        format!("error no configuration file at {}", shown(&directory))
    );
    world.create("user/app/settings.cfg/inner");
    assert_eq!(world.search(|_| ()), "nothing");
}

#[test]
fn a_relative_path_the_person_named_is_taken_against_the_working_directory() {
    let world = World::new();
    world.create("work/here.toml");
    let report = world.search(|child| {
        child.env("ANYCFG_TEST_EXPLICIT", "here.toml");
    });
    assert_eq!(report, "found here.toml");
}

#[test]
fn a_search_name_cannot_escape_the_directories() {
    for name in ["", "../escape.cfg", "/tmp/escape.cfg", "./app.cfg"] {
        assert!(
            matches!(Search::new(name).find(), Err(Error::InvalidConfigName)),
            "{name:?}"
        );
    }
}

#[test]
fn the_user_directory_follows_the_platform_convention() {
    let world = World::new();
    let user = world.path("user").join(NAME);
    assert_eq!(world.run("user", |_| ()), shown(&user));
    let fallback = if cfg!(windows) {
        world.path("home").join("AppData/Roaming").join(NAME)
    } else {
        world.path("home").join(".config").join(NAME)
    };
    let report = world.run("user", |child| {
        child.env_remove("APPDATA").env_remove("XDG_CONFIG_HOME");
    });
    assert_eq!(report, shown(&fallback));
    if !cfg!(windows) {
        let report = world.run("user", |child| {
            child.env("XDG_CONFIG_HOME", "relative/ignored");
        });
        assert_eq!(report, shown(&fallback));
    }
}

#[test]
fn the_system_directory_follows_the_platform_convention() {
    let world = World::new();
    let expected = if cfg!(windows) {
        world.path("system").join(NAME)
    } else {
        Path::new("/etc").join(NAME)
    };
    assert_eq!(world.run("system", |_| ()), shown(&expected));
}

#[test]
fn default_configs_lists_the_system_file_then_the_users_with_absences() {
    let world = World::new();
    assert_eq!(world.run("defaults", |_| ()), "system none user none");
    let user = world.create("user/app/settings.cfg");
    assert_eq!(
        world.run("defaults", |_| ()),
        format!("system none user {}", shown(&user))
    );
    // Only Windows lets the environment choose the system directory.
    if cfg!(windows) {
        let system = world.create("system/app/settings.cfg");
        assert_eq!(
            world.run("defaults", |_| ()),
            format!("system {} user {}", shown(&system), shown(&user))
        );
    }
}

#[test]
fn default_config_names_cannot_escape_the_standard_directories() {
    for name in ["", "../escape.cfg", "/tmp/escape.cfg", "./app.cfg"] {
        assert!(matches!(
            default_configs(name),
            Err(Error::InvalidConfigName)
        ));
    }
}
