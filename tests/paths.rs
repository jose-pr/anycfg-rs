use std::path::{Path, PathBuf};
use anycfg::{Error, default_configs, system_config_path, user_config_path};

#[test]
fn system_config_paths_follow_the_platform_convention() {
    let base = system_config_path(None);
    assert!(base.is_absolute());
    assert_eq!(system_config_path(Some(Path::new("app/settings.cfg"))), base.join("app/settings.cfg"));
    if !cfg!(windows) { assert_eq!(base, PathBuf::from("/etc")); }
}

#[test]
fn user_config_paths_follow_the_platform_convention() {
    let base = user_config_path(None).unwrap();
    assert!(base.is_absolute());
    assert_eq!(user_config_path(Some(Path::new("app/settings.cfg"))).unwrap(), base.join("app/settings.cfg"));
    if !cfg!(windows) {
        if std::env::var_os("XDG_CONFIG_HOME").filter(|x| Path::new(x).is_absolute()).is_none() {
            assert!(base.ends_with(".config"));
        }
    }
}

#[test]
fn existing_default_config_paths_are_ordered_with_absences() {
    let [system, user] = default_configs("anycfg-test-never-created_1a9d7f.cfg").unwrap();
    assert_eq!(system, None);
    assert_eq!(user, None);
}

#[test]
fn relative_nested_configuration_names_are_supported() {
    let paths = default_configs("app/config.cfg").unwrap();
    assert_eq!(paths.len(), 2);
}

#[test]
fn default_config_names_cannot_escape_the_standard_directories() {
    for name in ["", "../escape.cfg", "/tmp/escape.cfg", "./app.cfg"] {
        assert!(matches!(default_configs(name), Err(Error::InvalidConfigName)));
    }
}
