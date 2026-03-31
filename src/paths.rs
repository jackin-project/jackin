use directories::BaseDirs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct JackinPaths {
    pub home_dir: PathBuf,
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub agents_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl JackinPaths {
    pub fn detect() -> anyhow::Result<Self> {
        let base = BaseDirs::new().ok_or_else(|| anyhow::anyhow!("Cannot resolve home directory"))?;
        let home_dir = base.home_dir().to_path_buf();
        let config_dir = home_dir.join(".config/jackin");

        Ok(Self {
            config_file: config_dir.join("config.toml"),
            agents_dir: home_dir.join(".jackin/agents"),
            data_dir: home_dir.join(".jackin/data"),
            home_dir,
            config_dir,
        })
    }

    pub fn for_tests(root: &Path) -> Self {
        let home_dir = root.join("home");
        let config_dir = root.join("config");
        Self {
            config_file: config_dir.join("config.toml"),
            agents_dir: home_dir.join(".jackin/agents"),
            data_dir: home_dir.join(".jackin/data"),
            home_dir,
            config_dir,
        }
    }

    pub fn ensure_base_dirs(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(&self.agents_dir)?;
        std::fs::create_dir_all(&self.data_dir)?;
        Ok(())
    }
}
