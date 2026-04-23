//! Comment-preserving config writer.
//!
//! Reads still go through `AppConfig::load_or_init` (serde + `toml`).
//! Writes go through `ConfigEditor::open → mutate → save`, which keeps
//! user-written comments, blank lines, and key ordering intact in
//! sections untouched by the mutation.

use std::path::PathBuf;

use anyhow::Context;
use toml_edit::DocumentMut;

use crate::config::AppConfig;
use crate::paths::JackinPaths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvScope {
    Global,
    Agent(String),
    Workspace(String),
    WorkspaceAgent { workspace: String, agent: String },
}

pub struct ConfigEditor {
    doc: DocumentMut,
    path: PathBuf,
}

impl ConfigEditor {
    /// Loads the existing config file as a `DocumentMut`. If the file
    /// does not exist, delegates to `AppConfig::load_or_init` to
    /// materialize defaults, then reopens the resulting file.
    pub fn open(paths: &JackinPaths) -> anyhow::Result<Self> {
        if !paths.config_file.exists() {
            AppConfig::load_or_init(paths)?;
        }
        let raw = std::fs::read_to_string(&paths.config_file)
            .with_context(|| format!("reading {}", paths.config_file.display()))?;
        let doc: DocumentMut = raw
            .parse()
            .with_context(|| format!("parsing {}", paths.config_file.display()))?;
        Ok(Self {
            doc,
            path: paths.config_file.clone(),
        })
    }

    /// Writes the mutated document atomically. Returns a freshly-deserialized
    /// `AppConfig` parsed directly from the written content so callers that
    /// still need the in-memory shape get it without a second manual
    /// `load_or_init`.
    ///
    /// Note: this deliberately bypasses `AppConfig::load_or_init`'s
    /// builtin-agent sync to avoid clobbering the just-written document with
    /// a serde round-trip. Because it uses `toml::from_str` directly, it
    /// also skips `load_or_init`'s `validate_workspaces` and
    /// `validate_reserved_names` checks. The invariant this relies on is
    /// that validation runs once at load time (via `ConfigEditor::open` →
    /// `AppConfig::load_or_init` for first-run / `AppConfig::edit_workspace`
    /// for structural workspace edits) and that the editor's typed setters
    /// preserve validity — they write keys/values into known scopes and
    /// cannot construct a workspace or reserved-name violation on their own.
    pub fn save(self) -> anyhow::Result<AppConfig> {
        let contents = self.doc.to_string();
        let tmp = self.path.with_extension("tmp");

        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)?;
            file.write_all(contents.as_bytes())?;
            file.sync_all()?;
        }

        #[cfg(not(unix))]
        std::fs::write(&tmp, &contents)?;

        std::fs::rename(&tmp, &self.path)?;

        let config: AppConfig = toml::from_str(&contents)
            .with_context(|| format!("deserializing {}", self.path.display()))?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn idempotent_save_is_byte_identical() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        let original = r#"# Top-of-file note about this config
[claude]
auth_forward = "sync"

# Agents we trust
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true

# My production workspace
[workspaces.prod]
workdir = "/workspace/prod"

[[workspaces.prod.mounts]]
src = "/workspace/prod"
dst = "/workspace/prod"

[workspaces.prod.env]
# Rotate quarterly (last: 2026-Q1)
API_TOKEN = "op://Personal/api/token"
"#;
        std::fs::write(&paths.config_file, original).unwrap();

        let editor = ConfigEditor::open(&paths).unwrap();
        editor.save().unwrap();

        let round_tripped = std::fs::read_to_string(&paths.config_file).unwrap();
        assert_eq!(round_tripped, original, "open → save must be byte-identical");
    }
}
