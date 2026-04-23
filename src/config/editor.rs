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

    pub fn set_env_comment(&mut self, scope: EnvScope, key: &str, comment: Option<&str>) {
        let path = env_scope_path(&scope);
        // Walk without creating — setting a comment on a nonexistent key
        // is a silent no-op (same contract as remove_env_var).
        let mut current: &mut Item = self.doc.as_item_mut();
        for segment in &path {
            match current.as_table_mut().and_then(|t| t.get_mut(segment)) {
                Some(next) => current = next,
                None => return,
            }
        }
        let Some(table) = current.as_table_mut() else {
            return;
        };
        let Some(mut key_mut) = table.key_mut(key) else {
            return;
        };
        let decor = key_mut.leaf_decor_mut();
        let prefix = match comment {
            Some(text) => format!("# {text}\n"),
            None => String::new(),
        };
        decor.set_prefix(prefix);
    }

    /// Adds or replaces a named mount, mirroring `AppConfig::add_mount`.
    ///
    /// Unscoped (`scope = None`): writes `[docker.mounts.<name>]` — a single
    /// `MountConfig` entry keyed by `name`.
    ///
    /// Scoped (`scope = Some(scope_key)`): writes `[docker.mounts.<scope_key>]`
    /// with `name` as an inner key — i.e. the shape is
    /// `docker.mounts[scope_key][name]`, matching how `AppConfig` stores
    /// `MountEntry::Scoped`. Note this means `scope_key` is the OUTER key, not
    /// `name` — the same ordering used by `AppConfig::add_mount`.
    pub fn add_mount(
        &mut self,
        name: &str,
        mount: crate::workspace::MountConfig,
        scope: Option<&str>,
    ) {
        match scope {
            None => {
                // Unscoped: [docker.mounts.<name>]
                let mount_table = table_path_mut(
                    &mut self.doc,
                    &[
                        "docker".to_string(),
                        "mounts".to_string(),
                        name.to_string(),
                    ],
                );
                mount_table.clear();
                mount_table.insert("src", toml_edit::value(mount.src));
                mount_table.insert("dst", toml_edit::value(mount.dst));
                if mount.readonly {
                    mount_table.insert("readonly", toml_edit::value(true));
                }
            }
            Some(scope_key) => {
                // Scoped: [docker.mounts.<scope_key>] with name as inner key.
                // Matches AppConfig::add_mount which stores MountEntry::Scoped
                // keyed by scope_key at the outer level.
                let scoped_table = table_path_mut(
                    &mut self.doc,
                    &[
                        "docker".to_string(),
                        "mounts".to_string(),
                        scope_key.to_string(),
                    ],
                );
                // Build a sub-table for this named mount.
                let mut entry_table = Table::new();
                entry_table.insert("src", toml_edit::value(mount.src));
                entry_table.insert("dst", toml_edit::value(mount.dst));
                if mount.readonly {
                    entry_table.insert("readonly", toml_edit::value(true));
                }
                scoped_table.insert(name, Item::Table(entry_table));
            }
        }
    }

    /// Removes a named mount, mirroring `AppConfig::remove_mount`. Returns
    /// `true` if an entry was present and removed.
    ///
    /// Unscoped (`scope = None`): removes `docker.mounts[name]`.
    /// Scoped (`scope = Some(scope_key)`): removes the `name` entry from
    /// `docker.mounts[scope_key]`. If that scope table becomes empty after
    /// the removal, the scope table itself is removed too — matching
    /// `AppConfig::remove_mount`'s cleanup so empty scope tables do not
    /// accumulate in the on-disk config.
    pub fn remove_mount(&mut self, name: &str, scope: Option<&str>) -> bool {
        let Some(docker) = self
            .doc
            .get_mut("docker")
            .and_then(|i| i.as_table_mut())
        else {
            return false;
        };
        let Some(mounts) = docker.get_mut("mounts").and_then(|i| i.as_table_mut()) else {
            return false;
        };
        match scope {
            None => mounts.remove(name).is_some(),
            Some(scope_key) => {
                let Some(entry) = mounts
                    .get_mut(scope_key)
                    .and_then(|i| i.as_table_mut())
                else {
                    return false;
                };
                let removed = entry.remove(name).is_some();
                if removed && entry.is_empty() {
                    mounts.remove(scope_key);
                }
                removed
            }
        }
    }

    pub fn set_agent_trust(&mut self, agent_key: &str, trusted: bool) {
        let table = table_path_mut(
            &mut self.doc,
            &["agents".to_string(), agent_key.to_string()],
        );
        if trusted {
            table.insert("trusted", toml_edit::value(true));
        } else {
            // Canonical representation of false is absent (matches serde
            // skip_serializing_if on AgentSource::trusted).
            table.remove("trusted");
        }
    }

    pub fn set_agent_auth_forward(
        &mut self,
        agent_key: &str,
        mode: crate::config::AuthForwardMode,
    ) {
        let claude_table = table_path_mut(
            &mut self.doc,
            &["agents".to_string(), agent_key.to_string(), "claude".to_string()],
        );
        claude_table.insert("auth_forward", toml_edit::value(auth_forward_str(mode)));
    }

    pub fn set_global_auth_forward(&mut self, mode: crate::config::AuthForwardMode) {
        let claude_table = table_path_mut(&mut self.doc, &["claude".to_string()]);
        claude_table.insert("auth_forward", toml_edit::value(auth_forward_str(mode)));
    }

    pub fn upsert_builtin_agent(&mut self, agent_key: &str, git_url: &str) {
        // Touch only git + trusted. Leave [agents.X.claude] and
        // [agents.X.env] alone — those are operator-owned.
        let table = table_path_mut(
            &mut self.doc,
            &["agents".to_string(), agent_key.to_string()],
        );
        table.insert("git", toml_edit::value(git_url));
        table.insert("trusted", toml_edit::value(true));
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

    pub fn set_last_agent(&mut self, workspace: &str, agent_key: &str) {
        let table = table_path_mut(
            &mut self.doc,
            &["workspaces".to_string(), workspace.to_string()],
        );
        table.insert("last_agent", toml_edit::value(agent_key));
    }

    pub fn remove_workspace(&mut self, name: &str) -> anyhow::Result<()> {
        let Some(workspaces) = self.doc.get_mut("workspaces").and_then(|i| i.as_table_mut()) else {
            anyhow::bail!("workspace {name:?} not found");
        };
        if workspaces.remove(name).is_none() {
            anyhow::bail!("workspace {name:?} not found");
        }
        Ok(())
    }

    pub fn create_workspace(
        &mut self,
        name: &str,
        ws: crate::workspace::WorkspaceConfig,
    ) -> anyhow::Result<()> {
        // Collision check first — match today's create_workspace behavior.
        if self
            .doc
            .get("workspaces")
            .and_then(|i| i.as_table())
            .and_then(|t| t.get(name))
            .is_some()
        {
            anyhow::bail!("workspace {name:?} already exists");
        }

        // Serialize the WorkspaceConfig to toml_edit items via string round-trip:
        // toml::to_string on the struct, parse as DocumentMut, splat the body
        // into the new [workspaces.<name>] table.
        let rendered = toml::to_string(&ws)
            .with_context(|| format!("serializing workspace {name:?}"))?;
        let parsed: DocumentMut = rendered
            .parse()
            .with_context(|| format!("re-parsing serialized workspace {name:?}"))?;

        let workspaces_table = table_path_mut(
            &mut self.doc,
            &["workspaces".to_string(), name.to_string()],
        );
        for (key, item) in parsed.as_table().iter() {
            workspaces_table.insert(key, item.clone());
        }

        Ok(())
    }

    pub fn edit_workspace(
        &mut self,
        name: &str,
        edit: crate::workspace::WorkspaceEdit,
    ) -> anyhow::Result<()> {
        // Snapshot current on-disk state into an AppConfig.
        let mut in_memory: AppConfig = toml::from_str(&self.doc.to_string())
            .context("re-parsing current doc into AppConfig for workspace edit")?;

        // Apply the edit using the existing validated logic. Mutates
        // in_memory or returns Err with the validation message on failure.
        in_memory.edit_workspace(name, edit)?;

        // Pull the resulting WorkspaceConfig back out and splat into the doc.
        let updated = in_memory
            .workspaces
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("workspace {name:?} disappeared after edit"))?;

        // Replace the entire [workspaces.<name>] table. This preserves
        // comments in OTHER workspaces and in unrelated top-level sections,
        // which is what the migration cares about. Comments inside the
        // edited workspace itself are consumed — that's acceptable because
        // the edit IS the change the user is making to that workspace.
        let rendered = toml::to_string(updated)?;
        let parsed: DocumentMut = rendered.parse()?;
        let target = table_path_mut(
            &mut self.doc,
            &["workspaces".to_string(), name.to_string()],
        );
        target.clear();
        for (key, item) in parsed.as_table().iter() {
            target.insert(key, item.clone());
        }

        Ok(())
    }
}

fn auth_forward_str(mode: crate::config::AuthForwardMode) -> &'static str {
    match mode {
        crate::config::AuthForwardMode::Ignore => "ignore",
        crate::config::AuthForwardMode::Sync => "sync",
        crate::config::AuthForwardMode::Token => "token",
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
    fn set_env_comment_adds_line_above_key() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[env]
API_TOKEN = "op://vault-id/item-id/field"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_env_comment(
            EnvScope::Global,
            "API_TOKEN",
            Some("op://Personal/Google/password"),
        );
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(
            out.contains("# op://Personal/Google/password\nAPI_TOKEN"),
            "expected comment directly above key: {out}"
        );
    }

    #[test]
    fn set_env_comment_replaces_existing_comment() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            "[env]\n# old annotation\nAPI_TOKEN = \"x\"\n",
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_env_comment(EnvScope::Global, "API_TOKEN", Some("new annotation"));
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains("# new annotation"), "{out}");
        assert!(!out.contains("# old annotation"), "{out}");
    }

    #[test]
    fn set_env_comment_none_removes_annotation() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            "[env]\n# some note\nAPI_TOKEN = \"x\"\n",
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_env_comment(EnvScope::Global, "API_TOKEN", None);
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!out.contains("# some note"), "{out}");
        assert!(out.contains(r#"API_TOKEN = "x""#), "key still present: {out}");
    }

    #[test]
    fn mutating_sibling_preserves_comment_above_other_key() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        let original = "[env]\n# rotate quarterly\nAPI_TOKEN = \"x\"\nOTHER = \"y\"\n";
        std::fs::write(&paths.config_file, original).unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_env_var(EnvScope::Global, "OTHER", "z");
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(
            out.contains("# rotate quarterly\nAPI_TOKEN = \"x\""),
            "sibling mutation wiped adjacent comment: {out}"
        );
        assert!(out.contains(r#"OTHER = "z""#), "{out}");
    }

    #[test]
    fn mutating_one_workspace_preserves_comments_in_another() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        let original = r#"# workspace a — keep this comment
[workspaces.a]
workdir = "/a"

# workspace b — also keep
[workspaces.b]
workdir = "/b"
"#;
        std::fs::write(&paths.config_file, original).unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_env_var(EnvScope::Workspace("a".to_string()), "K", "v");
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains("# workspace b — also keep"), "{out}");
        assert!(out.contains("# workspace a — keep this comment"), "{out}");
    }

    #[test]
    fn fixture_round_trip_is_byte_identical() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        let original = include_str!("fixtures/config.round_trip.toml");
        std::fs::write(&paths.config_file, original).unwrap();

        let editor = ConfigEditor::open(&paths).unwrap();
        editor.save().unwrap();

        let round_tripped = std::fs::read_to_string(&paths.config_file).unwrap();
        assert_eq!(
            round_tripped, original,
            "fixture round-trip is lossy — toml_edit is dropping something"
        );
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

    #[test]
    #[cfg(unix)]
    fn saved_file_is_0600_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "[env]\nK = \"v\"\n").unwrap();

        let editor = ConfigEditor::open(&paths).unwrap();
        editor.save().unwrap();

        let perms = std::fs::metadata(&paths.config_file).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600, "config file must be 0600");
    }

    #[test]
    fn save_leaves_no_tmp_file_on_success() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "[env]\nK = \"v\"\n").unwrap();

        let editor = ConfigEditor::open(&paths).unwrap();
        editor.save().unwrap();

        let tmp = paths.config_file.with_extension("tmp");
        assert!(!tmp.exists(), "expected .tmp to be renamed away");
    }

    // ---- mount tests ----

    #[test]
    fn add_mount_unscoped_creates_single_mount_entry() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "").unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.add_mount(
            "shared-home",
            crate::workspace::MountConfig {
                src: "/home/user".to_string(),
                dst: "/workspace/home".to_string(),
                readonly: false,
            },
            None,
        );
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains("[docker.mounts.shared-home]"), "{out}");
        assert!(out.contains(r#"src = "/home/user""#), "{out}");
    }

    #[test]
    fn add_mount_scoped_creates_nested_entry() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "").unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        // Behavioral equivalence with AppConfig::add_mount:
        // scope is the OUTER key; name is the INNER key.
        // So scope=agent-smith produces [docker.mounts.agent-smith] with creds = {...}
        editor.add_mount(
            "creds",
            crate::workspace::MountConfig {
                src: "/run/secrets/x".to_string(),
                dst: "/secrets/x".to_string(),
                readonly: true,
            },
            Some("agent-smith"),
        );
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        // The scoped shape: [docker.mounts.agent-smith] with creds sub-table
        assert!(out.contains("[docker.mounts.agent-smith]"), "{out}");
        assert!(out.contains(r#"src = "/run/secrets/x""#), "{out}");
        assert!(out.contains("readonly = true"), "{out}");
    }

    #[test]
    fn remove_mount_unscoped_deletes_entry() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[docker.mounts.shared-home]
src = "/home/user"
dst = "/workspace/home"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        let removed = editor.remove_mount("shared-home", None);
        editor.save().unwrap();

        assert!(removed);
        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!out.contains("shared-home"), "{out}");
    }

    #[test]
    fn remove_mount_returns_false_for_missing() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "").unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        let removed = editor.remove_mount("nope", None);
        editor.save().unwrap();
        assert!(!removed);
    }

    #[test]
    fn remove_mount_scoped_last_entry_deletes_scope_table() {
        // Matches AppConfig::remove_mount cleanup: when the last named mount
        // in a scope is removed, the scope table itself is removed.
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[docker.mounts.agent-smith]
creds = { src = "/run/secrets/x", dst = "/secrets/x" }
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        let removed = editor.remove_mount("creds", Some("agent-smith"));
        editor.save().unwrap();

        assert!(removed);
        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!out.contains("agent-smith"), "empty scope table should be gone: {out}");
    }

    #[test]
    fn remove_mount_scoped_preserves_scope_when_siblings_remain() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[docker.mounts.agent-smith]
creds = { src = "/a", dst = "/a" }
logs = { src = "/b", dst = "/b" }
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        let removed = editor.remove_mount("creds", Some("agent-smith"));
        editor.save().unwrap();

        assert!(removed);
        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains("[docker.mounts.agent-smith]"), "scope table should still exist: {out}");
        assert!(!out.contains("creds"), "{out}");
        assert!(out.contains("logs"), "{out}");
    }

    #[test]
    fn set_agent_trust_toggles_trusted_field() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[agents.my-agent]
git = "https://example.com/a.git"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_agent_trust("my-agent", true);
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains("trusted = true"), "{out}");
    }

    #[test]
    fn set_agent_trust_false_removes_field() {
        // Canonical TOML representation of trusted=false is absent (serde
        // skip_serializing_if on AgentSource::trusted).
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[agents.my-agent]
git = "x"
trusted = true
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_agent_trust("my-agent", false);
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!out.contains("trusted"), "{out}");
    }

    #[test]
    fn set_agent_auth_forward_writes_claude_subtable() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[agents.my-agent]
git = "x"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_agent_auth_forward("my-agent", crate::config::AuthForwardMode::Token);
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains("[agents.my-agent.claude]"), "{out}");
        assert!(out.contains(r#"auth_forward = "token""#), "{out}");
    }

    #[test]
    fn set_global_auth_forward_writes_root_claude_table() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "").unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_global_auth_forward(crate::config::AuthForwardMode::Sync);
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains("[claude]"), "{out}");
        assert!(out.contains(r#"auth_forward = "sync""#), "{out}");
    }

    #[test]
    fn upsert_builtin_agent_creates_entry_when_missing() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "").unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.upsert_builtin_agent(
            "agent-smith",
            "https://github.com/jackin-project/jackin-agent-smith.git",
        );
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains("[agents.agent-smith]"), "{out}");
        assert!(out.contains("trusted = true"), "{out}");
    }

    #[test]
    fn upsert_builtin_agent_preserves_existing_claude_override() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[agents.agent-smith]
git = "OLD-URL"
trusted = false

[agents.agent-smith.claude]
auth_forward = "token"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.upsert_builtin_agent(
            "agent-smith",
            "https://github.com/jackin-project/jackin-agent-smith.git",
        );
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains(r#"git = "https://github.com/jackin-project/jackin-agent-smith.git""#), "{out}");
        assert!(out.contains("trusted = true"), "{out}");
        assert!(out.contains(r#"auth_forward = "token""#), "claude override wiped: {out}");
    }

    #[test]
    fn create_workspace_adds_table() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        let mount_src = temp.path().join("src");
        std::fs::create_dir_all(&mount_src).unwrap();
        std::fs::write(&paths.config_file, "").unwrap();

        let ws = crate::workspace::WorkspaceConfig {
            workdir: "/workspace/new".to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: mount_src.display().to_string(),
                dst: "/workspace/new".to_string(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.create_workspace("new-ws", ws).unwrap();
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains("[workspaces.new-ws]"), "{out}");
        assert!(out.contains(r#"workdir = "/workspace/new""#), "{out}");
    }

    #[test]
    fn set_last_agent_preserves_other_fields() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        let original = r#"[workspaces.prod]
workdir = "/workspace/prod"
default_agent = "agent-smith"
"#;
        std::fs::write(&paths.config_file, original).unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_last_agent("prod", "agent-smith");
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains(r#"last_agent = "agent-smith""#), "{out}");
        assert!(out.contains(r#"default_agent = "agent-smith""#), "{out}");
    }

    #[test]
    fn remove_workspace_deletes_table() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[workspaces.a]
workdir = "/a"

[workspaces.b]
workdir = "/b"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.remove_workspace("a").unwrap();
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!out.contains("[workspaces.a]"), "{out}");
        assert!(out.contains("[workspaces.b]"), "{out}");
    }
}
