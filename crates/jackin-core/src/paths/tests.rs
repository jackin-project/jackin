use super::*;
use std::ffi::OsString;

fn fake_home() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn jackin_home_dir_relocates_data_roles_cache() {
    let home = fake_home();
    let jackin_root = tempfile::tempdir().unwrap();
    let paths = JackinPaths::resolve_with_env(
        home.path(),
        Some(OsString::from(jackin_root.path()).as_os_str()),
        None,
    );
    assert_eq!(paths.jackin_home, jackin_root.path());
    assert_eq!(paths.data_dir, jackin_root.path().join("data"));
    assert_eq!(paths.roles_dir, jackin_root.path().join("roles"));
    assert_eq!(paths.cache_dir, jackin_root.path().join("cache"));
    assert_eq!(paths.config_dir, home.path().join(".config/jackin"));
}

#[test]
fn jackin_config_dir_relocates_config_only() {
    let home = fake_home();
    let config_root = tempfile::tempdir().unwrap();
    let paths = JackinPaths::resolve_with_env(
        home.path(),
        None,
        Some(OsString::from(config_root.path()).as_os_str()),
    );
    assert_eq!(paths.config_dir, config_root.path().to_path_buf());
    assert_eq!(paths.config_file, config_root.path().join("config.toml"));
    assert_eq!(paths.workspaces_dir, config_root.path().join("workspaces"));
    assert_eq!(paths.data_dir, home.path().join(".jackin/data"));
}

#[test]
fn env_overrides_are_independent() {
    let home = fake_home();
    let jackin_root = tempfile::tempdir().unwrap();
    let config_root = tempfile::tempdir().unwrap();
    let paths = JackinPaths::resolve_with_env(
        home.path(),
        Some(OsString::from(jackin_root.path()).as_os_str()),
        Some(OsString::from(config_root.path()).as_os_str()),
    );
    assert_eq!(paths.data_dir, jackin_root.path().join("data"));
    assert_eq!(paths.config_dir, config_root.path().to_path_buf());
}

#[test]
fn no_overrides_falls_back_to_home_relative_defaults() {
    let home = fake_home();
    let paths = JackinPaths::resolve_with_env(home.path(), None, None);
    assert_eq!(paths.data_dir, home.path().join(".jackin/data"));
    assert_eq!(paths.config_dir, home.path().join(".config/jackin"));
}

#[test]
fn paths_error_home_dir_message_parity() {
    let err = PathsError::HomeDirUnresolved;
    assert_eq!(err.to_string(), "Cannot resolve home directory");
}

#[test]
fn ensure_base_dirs_creates_layout() {
    let root = fake_home();
    let paths = JackinPaths::for_tests(root.path());
    paths.ensure_base_dirs().unwrap();
    assert!(paths.config_dir.is_dir());
    assert!(paths.roles_dir.is_dir());
    assert!(paths.data_dir.is_dir());
    assert!(paths.cache_dir.is_dir());
}
