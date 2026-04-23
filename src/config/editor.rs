//! Comment-preserving config writer.
//!
//! Reads still go through `AppConfig::load_or_init` (serde + `toml`).
//! Writes go through `ConfigEditor::open → mutate → save`, which keeps
//! user-written comments, blank lines, and key ordering intact in
//! sections untouched by the mutation.

use std::path::PathBuf;

use anyhow::Context;
use toml_edit::{DocumentMut, Item, Table};

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

    pub fn set_env_var(&mut self, scope: EnvScope, key: &str, value_str: &str) {
        let path = env_scope_path(&scope);
        let table = table_path_mut(&mut self.doc, &path);
        table.insert(key, toml_edit::value(value_str));
    }

    pub fn remove_env_var(&mut self, scope: EnvScope, key: &str) -> bool {
        let path = env_scope_path(&scope);
        // Walk without creating: return false if any segment is missing.
        let mut current: &mut Item = self.doc.as_item_mut();
        for segment in &path {
            match current.as_table_mut().and_then(|t| t.get_mut(segment)) {
                Some(next) => current = next,
                None => return false,
            }
        }
        match current.as_table_mut() {
            Some(table) => table.remove(key).is_some(),
            None => false,
        }
    }
}

fn env_scope_path(scope: &EnvScope) -> Vec<String> {
    match scope {
        EnvScope::Global => vec!["env".to_string()],
        EnvScope::Agent(a) => vec!["agents".to_string(), a.clone(), "env".to_string()],
        EnvScope::Workspace(w) => vec!["workspaces".to_string(), w.clone(), "env".to_string()],
        EnvScope::WorkspaceAgent { workspace, agent } => vec![
            "workspaces".to_string(),
            workspace.clone(),
            "agents".to_string(),
            agent.clone(),
            "env".to_string(),
        ],
    }
}

fn table_path_mut<'a>(doc: &'a mut DocumentMut, path: &[String]) -> &'a mut Table {
    fn walk<'a>(item: &'a mut Item, path: &[String]) -> &'a mut Table {
        let table = item.as_table_mut().expect("path segment is not a table");
        if path.is_empty() {
            return table;
        }
        let entry = table
            .entry(&path[0])
            .or_insert(Item::Table(Table::new()));
        walk(entry, &path[1..])
    }
    walk(doc.as_item_mut(), path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn set_env_var_creates_global_env_table() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "").unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_env_var(EnvScope::Global, "API_TOKEN", "op://Personal/api/token");
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains("[env]"), "missing [env] table: {out}");
        assert!(
            out.contains(r#"API_TOKEN = "op://Personal/api/token""#),
            "missing entry: {out}"
        );
    }

    #[test]
    fn set_env_var_upserts_workspace_agent_scope() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[workspaces.prod]
workdir = "/workspace/prod"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_env_var(
            EnvScope::WorkspaceAgent {
                workspace: "prod".to_string(),
                agent: "agent-smith".to_string(),
            },
            "OPENAI_API_KEY",
            "op://Work/OpenAI/default",
        );
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(
            out.contains("[workspaces.prod.agents.agent-smith.env]"),
            "missing nested table: {out}"
        );
        assert!(
            out.contains(r#"OPENAI_API_KEY = "op://Work/OpenAI/default""#),
            "missing entry: {out}"
        );
    }

    #[test]
    fn set_env_var_overwrites_existing_value() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[env]
API_TOKEN = "old-value"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_env_var(EnvScope::Global, "API_TOKEN", "new-value");
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains(r#"API_TOKEN = "new-value""#), "{out}");
        assert!(!out.contains("old-value"), "{out}");
    }

    #[test]
    fn remove_env_var_returns_true_when_present() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[env]
API_TOKEN = "x"
OTHER = "y"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        let removed = editor.remove_env_var(EnvScope::Global, "API_TOKEN");
        editor.save().unwrap();

        assert!(removed);
        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!out.contains("API_TOKEN"), "{out}");
        assert!(out.contains(r#"OTHER = "y""#), "sibling gone: {out}");
    }

    #[test]
    fn remove_env_var_returns_false_when_absent() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "").unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        let removed = editor.remove_env_var(EnvScope::Global, "API_TOKEN");
        editor.save().unwrap();

        assert!(!removed);
    }

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
