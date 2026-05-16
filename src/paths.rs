use directories::BaseDirs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct JackinPaths {
    pub home_dir: PathBuf,
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub workspaces_dir: PathBuf,
    pub roles_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub run_dir: PathBuf,
}

impl JackinPaths {
    pub fn detect() -> anyhow::Result<Self> {
        let base =
            BaseDirs::new().ok_or_else(|| anyhow::anyhow!("Cannot resolve home directory"))?;
        Ok(Self::resolve_with_env(
            base.home_dir(),
            std::env::var_os("JACKIN_HOME_DIR").as_deref(),
            std::env::var_os("JACKIN_CONFIG_DIR").as_deref(),
        ))
    }

    /// Build a `JackinPaths` from explicit inputs. Factored out so the
    /// `JACKIN_HOME_DIR` / `JACKIN_CONFIG_DIR` override semantics can be
    /// exercised without mutating process-wide env vars (`std::env::set_var`
    /// is unsafe and globally forbidden in this crate).
    #[must_use]
    pub fn resolve_with_env(
        home_dir: &Path,
        jackin_home_override: Option<&std::ffi::OsStr>,
        jackin_config_override: Option<&std::ffi::OsStr>,
    ) -> Self {
        let config_dir =
            jackin_config_override.map_or_else(|| home_dir.join(".config/jackin"), PathBuf::from);
        let jackin_home =
            jackin_home_override.map_or_else(|| home_dir.join(".jackin"), PathBuf::from);
        Self {
            config_file: config_dir.join("config.toml"),
            workspaces_dir: config_dir.join("workspaces"),
            roles_dir: jackin_home.join("roles"),
            data_dir: jackin_home.join("data"),
            cache_dir: jackin_home.join("cache"),
            run_dir: jackin_home.join("run"),
            home_dir: home_dir.to_path_buf(),
            config_dir,
        }
    }

    pub fn for_tests(root: &Path) -> Self {
        let home_dir = root.join("home");
        let config_dir = root.join("config");
        Self {
            config_file: config_dir.join("config.toml"),
            workspaces_dir: config_dir.join("workspaces"),
            roles_dir: home_dir.join(".jackin/roles"),
            data_dir: home_dir.join(".jackin/data"),
            cache_dir: home_dir.join(".jackin/cache"),
            run_dir: home_dir.join(".jackin/run"),
            home_dir,
            config_dir,
        }
    }

    pub fn ensure_base_dirs(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(&self.roles_dir)?;
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.cache_dir)?;
        std::fs::create_dir_all(&self.run_dir)?;
        Ok(())
    }
}

#[cfg(test)]
mod env_override_tests {
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

        assert_eq!(paths.data_dir, jackin_root.path().join("data"));
        assert_eq!(paths.roles_dir, jackin_root.path().join("roles"));
        assert_eq!(paths.cache_dir, jackin_root.path().join("cache"));
        // Config dir unaffected by JACKIN_HOME_DIR — the two overrides
        // must be independent.
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
        // Data tree unaffected by JACKIN_CONFIG_DIR.
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
        assert_eq!(paths.config_file, config_root.path().join("config.toml"));
    }

    #[test]
    fn no_overrides_falls_back_to_home_relative_defaults() {
        let home = fake_home();
        let paths = JackinPaths::resolve_with_env(home.path(), None, None);
        assert_eq!(paths.data_dir, home.path().join(".jackin/data"));
        assert_eq!(paths.config_dir, home.path().join(".config/jackin"));
    }
}
