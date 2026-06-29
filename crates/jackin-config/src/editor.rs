//! Comment-preserving config writer.
//!
//! Reads still go through `AppConfig::load_or_init` (serde + `toml`).
//! Writes go through `ConfigEditor::open → mutate → save`, which keeps
//! user-written comments, blank lines, and key ordering intact in
//! sections untouched by the mutation.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use jackin_core::{Agent, AuthForwardMode, EnvValue, JackinPaths};
use toml_edit::{DocumentMut, Item, Table};

use crate::app_config::AppConfig;
use crate::app_config::persist::{load_split_config, validate_reserved_env_names};
use crate::auth::GithubAuthMode;
use crate::migrations;
use crate::persist::{atomic_write, validate_workspace_file_stem};
use crate::schema::{MountConfig, WorkspaceConfig, WorkspaceEdit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvScope {
    Global,
    GlobalGithub,
    Role(String),
    Workspace(String),
    WorkspaceRole {
        workspace: String,
        role: String,
    },
    /// `[github.env]` inside the workspace file — the github-kind env
    /// block, parallel to the regular workspace `env` map but read by
    /// [`build_github_env_layers`] instead of the regular launch-time
    /// env merge. Used to thread `GH_TOKEN` / `GH_HOST` /
    /// `GH_ENTERPRISE_TOKEN` without polluting the agent-facing env map.
    WorkspaceGithub(String),
    /// `[roles.<role>.github.env]` inside the workspace file — most
    /// specific layer of the github env layering.
    WorkspaceRoleGithub {
        workspace: String,
        role: String,
    },
}

#[derive(Debug)]
pub struct ConfigEditor {
    doc: DocumentMut,
    path: PathBuf,
    workspaces_dir: PathBuf,
    workspace_docs: BTreeMap<String, DocumentMut>,
    removed_workspaces: BTreeSet<String>,
}

impl ConfigEditor {
    /// Loads the existing config file as a `DocumentMut`. Performs both
    /// schema-version and split-workspace migration before reading, so the
    /// on-disk result matches what `AppConfig::load_or_init` would produce.
    /// The recursion-when-missing branch covers the fresh-install case
    /// where the file does not yet exist.
    pub fn open(paths: &JackinPaths) -> anyhow::Result<Self> {
        if paths.config_file.exists() {
            migrations::migrate_config_file_if_needed(&paths.config_file)?;
            let raw = std::fs::read_to_string(&paths.config_file)
                .with_context(|| format!("reading {}", paths.config_file.display()))?;
            drop(load_split_config(paths, Some(raw))?);
        } else {
            AppConfig::load_or_init(paths)?;
        }
        let raw = std::fs::read_to_string(&paths.config_file)
            .with_context(|| format!("reading {}", paths.config_file.display()))?;
        let doc: DocumentMut = raw
            .parse()
            .with_context(|| format!("parsing {}", paths.config_file.display()))?;
        let workspace_docs = load_workspace_docs(paths)?;
        Ok(Self {
            doc,
            path: paths.config_file.clone(),
            workspaces_dir: paths.workspaces_dir.clone(),
            workspace_docs,
            removed_workspaces: BTreeSet::new(),
        })
    }

    /// Atomic write + return a fresh `AppConfig` parsed from the
    /// written content.
    ///
    /// Validates the candidate before renaming over the real config —
    /// otherwise a setter that produced an unloadable shape (e.g.
    /// stub role missing `git`) would brick every subsequent CLI
    /// command until the operator hand-edits TOML to recover.
    ///
    /// Skips `load_or_init`'s builtin-role sync — the invariant is
    /// that `load_or_init` ran once at `open` time, so builtins are
    /// already in place.
    pub fn save(self) -> anyhow::Result<AppConfig> {
        let global_contents = self.doc.to_string();

        let config: AppConfig = match validate_candidate(&global_contents, &self.workspace_docs) {
            Ok(cfg) => cfg,
            Err(err) => {
                return Err(err.context(format!(
                    "rejecting candidate config (would have written to {})",
                    self.path.display()
                )));
            }
        };

        atomic_write(&self.path, &global_contents)?;
        std::fs::create_dir_all(&self.workspaces_dir)?;
        for name in self.workspace_docs.keys() {
            validate_workspace_file_stem(name)?;
        }
        for (name, doc) in &self.workspace_docs {
            atomic_write(&self.workspace_file(name), &doc.to_string())?;
        }
        for removed in &self.removed_workspaces {
            let path = self.workspace_file(removed);
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(config)
    }

    pub fn set_env_var(
        &mut self,
        scope: &EnvScope,
        key: &str,
        value: EnvValue,
    ) -> anyhow::Result<()> {
        use jackin_core::EnvValue;
        use toml_edit::{InlineTable, Item, Value, value as toml_value};

        let (doc, path) = self.doc_and_path_for_env_scope(scope);
        let table = table_path_mut(doc, &path);
        let item = match value {
            EnvValue::Plain(s) => toml_value(s),
            EnvValue::OpRef(r) => {
                let mut tbl = InlineTable::new();
                tbl.insert("op", Value::from(r.op));
                tbl.insert("path", Value::from(r.path));
                // Pin the resolving account so multi-account vaults read
                // back correctly; serialized only when set (matches the
                // `OpRef` serde skip-when-None contract).
                if let Some(account) = r.account {
                    tbl.insert("account", Value::from(account));
                }
                // Only emit `on_demand` when set, mirroring the serde
                // skip-when-false contract so existing refs stay compact.
                if r.on_demand {
                    tbl.insert("on_demand", Value::from(true));
                }
                Item::Value(Value::InlineTable(tbl))
            }
            EnvValue::Extended(e) => {
                if e.on_demand {
                    let mut tbl = InlineTable::new();
                    tbl.insert("value", Value::from(e.value));
                    tbl.insert("on_demand", Value::from(true));
                    Item::Value(Value::InlineTable(tbl))
                } else {
                    // `on_demand = false` is identical to a plain scalar; the
                    // editor collapses it back so files stay in compact form.
                    toml_value(e.value)
                }
            }
        };
        table.insert(key, item);
        Ok(())
    }

    pub fn set_env_comment(&mut self, scope: &EnvScope, key: &str, comment: Option<&str>) {
        let (doc, path) = self.doc_and_path_for_env_scope(scope);
        // Walk without creating — setting a comment on a nonexistent key
        // is a silent no-op (same contract as remove_env_var).
        let mut current: &mut Item = doc.as_item_mut();
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
        let prefix = comment.map_or_else(String::new, |text| format!("# {text}\n"));
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
    pub fn add_mount(&mut self, name: &str, mount: MountConfig, scope: Option<&str>) {
        match scope {
            None => {
                // Unscoped: [docker.mounts.<name>]
                let mount_table = table_path_mut(
                    &mut self.doc,
                    &["docker".to_owned(), "mounts".to_owned(), name.to_owned()],
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
                        "docker".to_owned(),
                        "mounts".to_owned(),
                        scope_key.to_owned(),
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
        let Some(docker) = self.doc.get_mut("docker").and_then(|i| i.as_table_mut()) else {
            return false;
        };
        let Some(mounts) = docker.get_mut("mounts").and_then(|i| i.as_table_mut()) else {
            return false;
        };
        match scope {
            None => mounts.remove(name).is_some(),
            Some(scope_key) => {
                let Some(entry) = mounts.get_mut(scope_key).and_then(|i| i.as_table_mut()) else {
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
        let table = table_path_mut(&mut self.doc, &["roles".to_owned(), agent_key.to_owned()]);
        if trusted {
            table.insert("trusted", toml_edit::value(true));
        } else {
            // Canonical representation of false is absent (matches serde
            // skip_serializing_if on RoleSource::trusted).
            table.remove("trusted");
        }
    }

    /// Write `[<agent.slug>].auth_forward = <mode>` at the global layer.
    pub fn set_global_auth_forward(&mut self, agent: Agent, mode: AuthForwardMode) {
        let table = table_path_mut(&mut self.doc, &[agent.slug().to_owned()]);
        table.insert("auth_forward", toml_edit::value(auth_forward_str(mode)));
    }

    /// Write or clear `[<agent.slug>].sync_source_dir` at the global layer.
    pub fn set_global_sync_source_dir(&mut self, agent: Agent, source: Option<&Path>) {
        let agent_path = vec![agent.slug().to_owned()];
        set_sync_source_dir_field(&mut self.doc, &agent_path, source);
    }

    /// Write `[github].auth_forward = <mode>` at the global layer.
    pub fn set_global_github_auth_forward(&mut self, mode: GithubAuthMode) {
        let table = table_path_mut(&mut self.doc, &["github".to_owned()]);
        table.insert("auth_forward", toml_edit::value(github_mode_str(mode)));
    }

    pub fn set_global_github_env_var(&mut self, key: &str, value: EnvValue) -> anyhow::Result<()> {
        self.set_env_var(&EnvScope::GlobalGithub, key, value)
    }

    pub fn remove_global_github_env_var(&mut self, key: &str) -> bool {
        self.remove_env_var(&EnvScope::GlobalGithub, key)
    }

    pub fn set_git_coauthor_trailer(&mut self, enabled: bool) {
        self.set_git_bool_field("coauthor_trailer", enabled);
    }

    pub fn set_git_dco(&mut self, enabled: bool) {
        self.set_git_bool_field("dco", enabled);
    }

    fn set_git_bool_field(&mut self, field: &str, enabled: bool) {
        let git_path = ["git".to_owned()];
        if enabled {
            let table = table_path_mut(&mut self.doc, &git_path);
            table.insert(field, toml_edit::value(true));
        } else {
            if let Some(git_table) = self
                .doc
                .as_table_mut()
                .get_mut("git")
                .and_then(|t| t.as_table_mut())
            {
                git_table.remove(field);
            }
            prune_empty_trailing_tables(&mut self.doc, &git_path, 1);
        }
    }

    /// Write or clear `[<agent>].auth_forward` inside the workspace file.
    ///
    /// `mode = Some(m)` writes the named mode; `mode = None` removes the
    /// `auth_forward` field (and the agent block if it becomes empty),
    /// dropping that layer of the resolver back to the workspace's
    /// inheritance from the global default.
    ///
    /// The agent block is keyed by `agent.slug()`, keeping the on-disk
    /// shape parallel to `set_global_auth_forward`.
    pub fn set_workspace_auth_forward(
        &mut self,
        workspace: &str,
        agent: Agent,
        mode: Option<AuthForwardMode>,
    ) {
        let agent_path = vec![agent.slug().to_owned()];
        let doc = self.workspace_doc_mut(workspace);
        if let Some(m) = mode {
            let table = table_path_mut(doc, &agent_path);
            table.insert("auth_forward", toml_edit::value(auth_forward_str(m)));
        } else {
            clear_auth_forward_field(doc, &agent_path);
        }
    }

    /// Write or clear `[<agent>].sync_source_dir` inside the workspace file.
    pub fn set_workspace_sync_source_dir(
        &mut self,
        workspace: &str,
        agent: Agent,
        source: Option<&Path>,
    ) {
        let agent_path = vec![agent.slug().to_owned()];
        let doc = self.workspace_doc_mut(workspace);
        set_sync_source_dir_field(doc, &agent_path, source);
    }

    /// Write or clear `[roles.<role>.<agent>].auth_forward` inside the workspace file.
    ///
    /// Mirrors [`Self::set_workspace_auth_forward`] one layer deeper.
    /// Used by the workspace-manager's Auth tab when the operator commits
    /// the auth-edit form on a (role × agent) row.
    pub fn set_workspace_role_auth_forward(
        &mut self,
        workspace: &str,
        role: &str,
        agent: Agent,
        mode: Option<AuthForwardMode>,
    ) {
        let agent_path = vec!["roles".to_owned(), role.to_owned(), agent.slug().to_owned()];
        let doc = self.workspace_doc_mut(workspace);
        if let Some(m) = mode {
            let table = table_path_mut(doc, &agent_path);
            table.insert("auth_forward", toml_edit::value(auth_forward_str(m)));
        } else {
            clear_auth_forward_field(doc, &agent_path);
        }
    }

    /// Write or clear `[roles.<role>.<agent>].sync_source_dir` inside the workspace file.
    pub fn set_workspace_role_sync_source_dir(
        &mut self,
        workspace: &str,
        role: &str,
        agent: Agent,
        source: Option<&Path>,
    ) {
        let agent_path = vec!["roles".to_owned(), role.to_owned(), agent.slug().to_owned()];
        let doc = self.workspace_doc_mut(workspace);
        set_sync_source_dir_field(doc, &agent_path, source);
    }

    /// Write or clear `[github].auth_forward` inside the workspace file.
    ///
    /// Mirrors [`Self::set_workspace_auth_forward`] but threads the
    /// GitHub kind's `[github]` block instead of an `Agent`-keyed
    /// child block. `mode = None` removes the `auth_forward` field
    /// (and the `github` block if it becomes empty), letting the
    /// resolver fall back to the next layer.
    pub fn set_workspace_github_auth_forward(
        &mut self,
        workspace: &str,
        mode: Option<GithubAuthMode>,
    ) {
        let github_path = vec!["github".to_owned()];
        let doc = self.workspace_doc_mut(workspace);
        if let Some(m) = mode {
            let table = table_path_mut(doc, &github_path);
            table.insert("auth_forward", toml_edit::value(github_mode_str(m)));
        } else {
            clear_auth_forward_field(doc, &github_path);
        }
    }

    /// Write or clear `[roles.<role>.github].auth_forward` inside the workspace file.
    ///
    /// Mirrors [`Self::set_workspace_role_auth_forward`] one kind
    /// dimension wider — `github` lives at the same three layers as
    /// `claude` / `codex`, but with no per-agent split.
    pub fn set_workspace_role_github_auth_forward(
        &mut self,
        workspace: &str,
        role: &str,
        mode: Option<GithubAuthMode>,
    ) {
        let github_path = vec!["roles".to_owned(), role.to_owned(), "github".to_owned()];
        let doc = self.workspace_doc_mut(workspace);
        if let Some(m) = mode {
            let table = table_path_mut(doc, &github_path);
            table.insert("auth_forward", toml_edit::value(github_mode_str(m)));
        } else {
            clear_auth_forward_field(doc, &github_path);
        }
    }

    pub fn upsert_builtin_agent(&mut self, agent_key: &str, git_url: &str) {
        // Touch only git + trusted. Leave [roles.X.env] alone —
        // operator-owned.
        let table = table_path_mut(&mut self.doc, &["roles".to_owned(), agent_key.to_owned()]);
        table.insert("git", toml_edit::value(git_url));
        table.insert("trusted", toml_edit::value(true));
    }

    /// Writes `git` and `trusted` from the given `RoleSource` into
    /// `[roles.<agent_key>]`. Does NOT touch `[roles.<agent_key>.env]` —
    /// operator-owned.
    ///
    /// Used by call sites that first invoke `resolve_role_source` (which may
    /// insert a new role into the in-memory `AppConfig`) and need the editor
    /// to persist that insert alongside whatever trust change they're about
    /// to make.
    pub fn upsert_agent_source(&mut self, agent_key: &str, source: &crate::schema::RoleSource) {
        let table = table_path_mut(&mut self.doc, &["roles".to_owned(), agent_key.to_owned()]);
        table.insert("git", toml_edit::value(source.git.clone()));
        if source.trusted {
            table.insert("trusted", toml_edit::value(true));
        } else {
            table.remove("trusted");
        }
    }

    pub fn remove_env_var(&mut self, scope: &EnvScope, key: &str) -> bool {
        let (doc, path) = self.doc_and_path_for_env_scope(scope);
        // Walk without creating: return false if any segment is missing.
        let mut current: &mut Item = doc.as_item_mut();
        for segment in &path {
            match current.as_table_mut().and_then(|t| t.get_mut(segment)) {
                Some(next) => current = next,
                None => return false,
            }
        }
        let removed = current
            .as_table_mut()
            .is_some_and(|table| table.remove(key).is_some());
        if removed {
            // Avoid leaving an empty `[…env]` (and its now-empty kind
            // parent like `[…github]`) behind on disk after the last
            // key is removed. `max_prune = 2` lets the walk peel both
            // the `env` segment and its kind parent, but no further —
            // workspace / role identifier slots stay untouched even
            // when an operator names them "env" / "github" / etc.
            prune_empty_trailing_tables(doc, &path, 2);
        }
        removed
    }

    pub fn set_last_agent(&mut self, workspace: &str, agent_key: &str) {
        let doc = self.workspace_doc_mut(workspace);
        let table = table_path_mut(doc, &[]);
        table.insert("last_role", toml_edit::value(agent_key));
    }

    /// Rename a workspace key in the `[workspaces]` table.
    ///
    /// Preserves all nested fields (mounts, env, roles overrides, etc.)
    /// because `toml_edit` renames the key in place. Fails if:
    ///   - new name is empty
    ///   - old name does not exist
    ///   - new name already exists
    pub fn rename_workspace(&mut self, old: &str, new: &str) -> anyhow::Result<()> {
        if new.is_empty() {
            anyhow::bail!("workspace name cannot be empty");
        }
        if old == new {
            return Ok(());
        }
        if !self.workspace_docs.contains_key(old) {
            anyhow::bail!("workspace {old:?} not found");
        }
        if self.workspace_docs.contains_key(new) {
            anyhow::bail!("workspace {new:?} already exists");
        }

        validate_workspace_file_stem(new)?;
        let Some(value) = self.workspace_docs.remove(old) else {
            anyhow::bail!("workspace {old:?} not found");
        };
        self.workspace_docs.insert(new.to_owned(), value);
        self.removed_workspaces.insert(old.to_owned());
        Ok(())
    }

    pub fn remove_workspace(&mut self, name: &str) -> anyhow::Result<()> {
        if self.workspace_docs.remove(name).is_none() {
            anyhow::bail!("workspace {name:?} not found");
        }
        self.removed_workspaces.insert(name.to_owned());
        Ok(())
    }

    pub fn create_workspace(&mut self, name: &str, ws: WorkspaceConfig) -> anyhow::Result<()> {
        // Delegate to AppConfig::create_workspace's validated logic
        // (collision check, workdir / mount-destination relationship,
        // plan-collapse sanity) so the editor path behaves identically
        // to the direct-mutation path. Mirrors edit_workspace's pattern.
        let mut in_memory = validate_candidate(&self.doc.to_string(), &self.workspace_docs)
            .context("re-parsing current docs into AppConfig for workspace creation")?;
        in_memory.create_workspace(name, ws)?;
        let inserted = in_memory
            .workspaces
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("workspace {name:?} disappeared after create"))?;

        let rendered =
            toml::to_string(inserted).with_context(|| format!("serializing workspace {name:?}"))?;
        let parsed: DocumentMut = rendered
            .parse()
            .with_context(|| format!("re-parsing serialized workspace {name:?}"))?;

        self.workspace_docs.insert(name.to_owned(), parsed);
        self.removed_workspaces.remove(name);

        Ok(())
    }

    pub fn edit_workspace(&mut self, name: &str, edit: WorkspaceEdit) -> anyhow::Result<()> {
        // Snapshot current on-disk state into an AppConfig.
        let mut in_memory = validate_candidate(&self.doc.to_string(), &self.workspace_docs)
            .context("re-parsing current docs into AppConfig for workspace edit")?;

        // Apply the edit using the existing validated logic. Mutates
        // in_memory or returns Err with the validation message on failure.
        in_memory.edit_workspace(name, edit)?;

        // Pull the resulting WorkspaceConfig back out and splat into the doc.
        let updated = in_memory
            .workspaces
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("workspace {name:?} disappeared after edit"))?;

        // Replace the entire workspace document. This preserves
        // comments in OTHER workspaces and in unrelated top-level sections,
        // which is what the migration cares about. Comments inside the
        // edited workspace itself are consumed — that's acceptable because
        // the edit IS the change the user is making to that workspace.
        let rendered = toml::to_string(updated)?;
        let parsed: DocumentMut = rendered.parse()?;
        self.workspace_docs.insert(name.to_owned(), parsed);

        Ok(())
    }

    /// Test-only: insert a string value at a dotted table path in the main doc.
    ///
    /// Used by tests that need to inject invalid TOML shapes (e.g. a role env
    /// block without the required `git` field) to exercise save-time rejection.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn insert_at_path(&mut self, path: &[String], key: &str, value: &str) {
        let table = table_path_mut(&mut self.doc, path);
        table.insert(key, toml_edit::value(value));
    }

    fn workspace_file(&self, name: &str) -> PathBuf {
        self.workspaces_dir.join(format!("{name}.toml"))
    }

    fn workspace_doc_mut(&mut self, workspace: &str) -> &mut DocumentMut {
        debug_assert!(validate_workspace_file_stem(workspace).is_ok());
        self.removed_workspaces.remove(workspace);
        self.workspace_docs.entry(workspace.to_owned()).or_default()
    }

    fn doc_and_path_for_env_scope(&mut self, scope: &EnvScope) -> (&mut DocumentMut, Vec<String>) {
        match scope {
            EnvScope::Global | EnvScope::GlobalGithub | EnvScope::Role(_) => {
                (&mut self.doc, env_scope_path(scope))
            }
            EnvScope::Workspace(w) => {
                let doc = self.workspace_doc_mut(w);
                (doc, vec!["env".to_owned()])
            }
            EnvScope::WorkspaceRole { workspace, role } => {
                let doc = self.workspace_doc_mut(workspace);
                (
                    doc,
                    vec!["roles".to_owned(), role.clone(), "env".to_owned()],
                )
            }
            EnvScope::WorkspaceGithub(w) => {
                let doc = self.workspace_doc_mut(w);
                (doc, vec!["github".to_owned(), "env".to_owned()])
            }
            EnvScope::WorkspaceRoleGithub { workspace, role } => {
                let doc = self.workspace_doc_mut(workspace);
                (
                    doc,
                    vec![
                        "roles".to_owned(),
                        role.clone(),
                        "github".to_owned(),
                        "env".to_owned(),
                    ],
                )
            }
        }
    }
}

const fn auth_forward_str(mode: AuthForwardMode) -> &'static str {
    match mode {
        AuthForwardMode::Ignore => "ignore",
        AuthForwardMode::Sync => "sync",
        // Tasks 10/11 will split per-mode behavior; today both env-driven
        // modes serialize to their canonical snake_case names.
        AuthForwardMode::ApiKey => "api_key",
        AuthForwardMode::OAuthToken => "oauth_token",
    }
}

const fn github_mode_str(mode: GithubAuthMode) -> &'static str {
    match mode {
        GithubAuthMode::Sync => "sync",
        GithubAuthMode::Token => "token",
        GithubAuthMode::Ignore => "ignore",
    }
}

fn env_scope_path(scope: &EnvScope) -> Vec<String> {
    match scope {
        EnvScope::Global => vec!["env".to_owned()],
        EnvScope::GlobalGithub => vec!["github".to_owned(), "env".to_owned()],
        EnvScope::Role(a) => vec!["roles".to_owned(), a.clone(), "env".to_owned()],
        EnvScope::Workspace(w) => vec!["workspaces".to_owned(), w.clone(), "env".to_owned()],
        EnvScope::WorkspaceRole { workspace, role } => vec![
            "workspaces".to_owned(),
            workspace.clone(),
            "roles".to_owned(),
            role.clone(),
            "env".to_owned(),
        ],
        EnvScope::WorkspaceGithub(w) => vec![
            "workspaces".to_owned(),
            w.clone(),
            "github".to_owned(),
            "env".to_owned(),
        ],
        EnvScope::WorkspaceRoleGithub { workspace, role } => vec![
            "workspaces".to_owned(),
            workspace.clone(),
            "roles".to_owned(),
            role.clone(),
            "github".to_owned(),
            "env".to_owned(),
        ],
    }
}

/// Subset of `load_or_init` validations the editor's typed setters
/// could plausibly violate: serde-required fields (catches stub
/// role missing `git`) and `validate_reserved_names`. Skips
/// `validate_workspaces` — only `create_workspace`/`edit_workspace`
/// mutate that geometry and they already validate.
fn validate_candidate(
    global_contents: &str,
    workspace_docs: &BTreeMap<String, DocumentMut>,
) -> anyhow::Result<AppConfig> {
    let mut config: AppConfig =
        toml::from_str(global_contents).context("deserializing candidate global config")?;
    if !config.workspaces.is_empty() {
        anyhow::bail!("global config.toml must not contain [workspaces] tables");
    }
    for (name, doc) in workspace_docs {
        validate_workspace_file_stem(name)?;
        let workspace: WorkspaceConfig = toml::from_str(&doc.to_string())
            .with_context(|| format!("deserializing candidate workspace {name:?}"))?;
        workspace.validate_auth_modes()?;
        config.workspaces.insert(name.clone(), workspace);
    }
    validate_reserved_env_names(&config)?;
    config.validate_auth_modes()?;
    Ok(config)
}

fn load_workspace_docs(paths: &JackinPaths) -> anyhow::Result<BTreeMap<String, DocumentMut>> {
    let mut docs = BTreeMap::new();
    let entries = match std::fs::read_dir(&paths.workspaces_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(docs),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid workspace filename {}", path.display()))?;
        validate_workspace_file_stem(stem)
            .with_context(|| format!("invalid workspace filename {}", path.display()))?;
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading workspace config {}", path.display()))?;
        let doc = raw
            .parse()
            .with_context(|| format!("parsing workspace config {}", path.display()))?;
        docs.insert(stem.to_owned(), doc);
    }
    Ok(docs)
}

/// Remove the `auth_forward` field at `kind_path` (a `[…claude]` /
/// `[…codex]` / `[…github]` block). If the kind block is left empty
/// afterwards, the now-empty kind segment is peeled off too. Empty
/// `[…env]` subtables are removed by [`ConfigEditor::remove_env_var`] when
/// the operator's env diff goes through; this helper does not touch
/// them itself. Walks without creating, so a reset on a layer that
/// was already empty is a no-op.
fn clear_auth_forward_field(doc: &mut DocumentMut, kind_path: &[String]) {
    let mut current: &mut Item = doc.as_item_mut();
    for segment in kind_path {
        match current.as_table_mut().and_then(|t| t.get_mut(segment)) {
            Some(next) => current = next,
            None => return,
        }
    }
    if let Some(table) = current.as_table_mut() {
        table.remove("auth_forward");
    }
    // Peel off only the trailing kind segment if it's now empty.
    // Caller passes `max_prune = 1` to bound the walk so a workspace
    // or role identifier — even one literally named "github" /
    // "claude" / "codex" / "env" — is never reached.
    prune_empty_trailing_tables(doc, kind_path, 1);
}

fn set_sync_source_dir_field(doc: &mut DocumentMut, kind_path: &[String], source: Option<&Path>) {
    if let Some(source) = source {
        let table = table_path_mut(doc, kind_path);
        table.insert(
            "sync_source_dir",
            toml_edit::value(source.display().to_string()),
        );
        return;
    }

    let mut current: &mut Item = doc.as_item_mut();
    for segment in kind_path {
        match current.as_table_mut().and_then(|t| t.get_mut(segment)) {
            Some(next) => current = next,
            None => return,
        }
    }
    if let Some(table) = current.as_table_mut() {
        table.remove("sync_source_dir");
    }
    prune_empty_trailing_tables(doc, kind_path, 1);
}

/// Walk `path` from leaf back toward root, peeling off **at most
/// `max_prune` trailing segments** whose corresponding tables are
/// empty after prior removals. Stops on the first segment whose
/// table is still non-empty.
///
/// `max_prune` is an absolute bound on how many trailing segments may
/// be removed. Callers set it based on the path's known structure so
/// the walk is bounded by *position* rather than by segment name —
/// this is what prevents the helper from stripping an operator's
/// workspace or role override, even when they happen to use a name
/// like "github" or "env" for the workspace / role identifier.
///
/// Typical bounds:
///   * `max_prune = 1` — peel only the kind segment (called from
///     [`clear_auth_forward_field`] with paths like
///     `[…, ws, "claude"]` or `[…, ws, "roles", role, "github"]`).
///   * `max_prune = 2` — peel `[…env]` and its kind parent (called
///     from [`ConfigEditor::remove_env_var`] with paths like
///     `[…, ws, "env"]` or `[…, ws, "github", "env"]`).
fn prune_empty_trailing_tables(doc: &mut DocumentMut, path: &[String], max_prune: usize) {
    let stop_at = path.len().saturating_sub(max_prune);
    for i in (stop_at..path.len()).rev() {
        let segment = &path[i];
        let parent_path = &path[..i];
        let mut walker: &mut Item = doc.as_item_mut();
        for parent_segment in parent_path {
            match walker
                .as_table_mut()
                .and_then(|t| t.get_mut(parent_segment))
            {
                Some(next) => walker = next,
                None => return,
            }
        }
        let Some(parent_table) = walker.as_table_mut() else {
            return;
        };
        let still_empty = parent_table
            .get(segment.as_str())
            .and_then(Item::as_table)
            .is_some_and(Table::is_empty);
        if !still_empty {
            return;
        }
        parent_table.remove(segment.as_str());
    }
}

fn table_path_mut<'a>(doc: &'a mut DocumentMut, path: &[String]) -> &'a mut Table {
    #[expect(
        clippy::expect_used,
        reason = "toml_edit table insertion above guarantees the just-created entry is a table"
    )]
    fn walk<'a>(item: &'a mut Item, path: &[String]) -> &'a mut Table {
        let table = item.as_table_mut().expect("path segment is not a table");
        if path.is_empty() {
            return table;
        }
        let entry = table.entry(&path[0]).or_insert(Item::Table(Table::new()));
        walk(entry, &path[1..])
    }
    walk(doc.as_item_mut(), path)
}

#[cfg(test)]
mod tests;
