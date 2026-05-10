//! Comment-preserving config writer.
//!
//! Reads still go through `AppConfig::load_or_init` (serde + `toml`).
//! Writes go through `ConfigEditor::open → mutate → save`, which keeps
//! user-written comments, blank lines, and key ordering intact in
//! sections untouched by the mutation.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use anyhow::Context;
use toml_edit::{DocumentMut, Item, Table};

use crate::config::AppConfig;
use crate::paths::JackinPaths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvScope {
    Global,
    Role(String),
    Workspace(String),
    WorkspaceRole {
        workspace: String,
        role: String,
    },
    /// `[github.env]` inside the workspace file — the github-kind env
    /// block, parallel to the regular workspace `env` map but read by
    /// [`crate::config::build_github_env_layers`] instead of the
    /// regular launch-time env merge. Used to thread `GH_TOKEN` /
    /// `GH_HOST` / `GH_ENTERPRISE_TOKEN` without polluting the
    /// agent-facing env map.
    WorkspaceGithub(String),
    /// `[roles.<role>.github.env]` inside the workspace file — most
    /// specific layer of the github env layering.
    WorkspaceRoleGithub {
        workspace: String,
        role: String,
    },
}

pub struct ConfigEditor {
    doc: DocumentMut,
    path: PathBuf,
    workspaces_dir: PathBuf,
    workspace_docs: BTreeMap<String, DocumentMut>,
    removed_workspaces: BTreeSet<String>,
}

impl ConfigEditor {
    /// Loads the existing config file as a `DocumentMut`. If the file
    /// does not exist, delegates to `AppConfig::load_or_init` to
    /// materialize defaults, then reopens the resulting file.
    pub fn open(paths: &JackinPaths) -> anyhow::Result<Self> {
        if paths.config_file.exists() {
            crate::config::migrations::migrate_config_file_if_needed(&paths.config_file)?;
            let raw = std::fs::read_to_string(&paths.config_file)
                .with_context(|| format!("reading {}", paths.config_file.display()))?;
            let _ = crate::config::persist::load_split_config(paths, Some(raw))?;
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

        crate::config::persist::atomic_write(&self.path, &global_contents)?;
        std::fs::create_dir_all(&self.workspaces_dir)?;
        for name in self.workspace_docs.keys() {
            crate::config::persist::validate_workspace_file_stem(name)?;
        }
        for (name, doc) in &self.workspace_docs {
            crate::config::persist::atomic_write(&self.workspace_file(name), &doc.to_string())?;
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
        value: crate::operator_env::EnvValue,
    ) -> anyhow::Result<()> {
        use crate::operator_env::EnvValue;
        use toml_edit::{InlineTable, Item, Value, value as toml_value};

        let (doc, path) = self.doc_and_path_for_env_scope(scope);
        let table = table_path_mut(doc, &path);
        let item = match value {
            EnvValue::Plain(s) => toml_value(s),
            EnvValue::OpRef(r) => {
                let mut tbl = InlineTable::new();
                tbl.insert("op", Value::from(r.op));
                tbl.insert("path", Value::from(r.path));
                Item::Value(Value::InlineTable(tbl))
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
                    &["docker".to_string(), "mounts".to_string(), name.to_string()],
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
        let table = table_path_mut(&mut self.doc, &["roles".to_string(), agent_key.to_string()]);
        if trusted {
            table.insert("trusted", toml_edit::value(true));
        } else {
            // Canonical representation of false is absent (matches serde
            // skip_serializing_if on RoleSource::trusted).
            table.remove("trusted");
        }
    }

    /// Write `[<agent.slug>].auth_forward = <mode>` at the global layer.
    pub fn set_global_auth_forward(
        &mut self,
        agent: crate::agent::Agent,
        mode: crate::config::AuthForwardMode,
    ) {
        let table = table_path_mut(&mut self.doc, &[agent.slug().to_string()]);
        table.insert("auth_forward", toml_edit::value(auth_forward_str(mode)));
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
        agent: crate::agent::Agent,
        mode: Option<crate::config::AuthForwardMode>,
    ) {
        let agent_path = vec![agent.slug().to_string()];
        let doc = self.workspace_doc_mut(workspace);
        if let Some(m) = mode {
            let table = table_path_mut(doc, &agent_path);
            table.insert("auth_forward", toml_edit::value(auth_forward_str(m)));
        } else {
            clear_auth_forward_field(doc, &agent_path);
        }
    }

    /// Write or clear `op_account` inside the workspace file.
    ///
    /// Pins every `op` invocation made on behalf of the workspace to
    /// the named 1P account. Operator can pass UUID, label, or email
    /// — `op` accepts all three.
    pub fn set_workspace_op_account(&mut self, workspace: &str, account: Option<&str>) {
        use toml_edit::value as toml_value;
        let doc = self.workspace_doc_mut(workspace);
        let table = table_path_mut(doc, &[]);
        match account {
            Some(acc) => {
                table.insert("op_account", toml_value(acc));
            }
            None => {
                table.remove("op_account");
            }
        }
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
        agent: crate::agent::Agent,
        mode: Option<crate::config::AuthForwardMode>,
    ) {
        let agent_path = vec![
            "roles".to_string(),
            role.to_string(),
            agent.slug().to_string(),
        ];
        let doc = self.workspace_doc_mut(workspace);
        if let Some(m) = mode {
            let table = table_path_mut(doc, &agent_path);
            table.insert("auth_forward", toml_edit::value(auth_forward_str(m)));
        } else {
            clear_auth_forward_field(doc, &agent_path);
        }
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
        mode: Option<crate::config::GithubAuthMode>,
    ) {
        let github_path = vec!["github".to_string()];
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
        mode: Option<crate::config::GithubAuthMode>,
    ) {
        let github_path = vec!["roles".to_string(), role.to_string(), "github".to_string()];
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
        let table = table_path_mut(&mut self.doc, &["roles".to_string(), agent_key.to_string()]);
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
    pub fn upsert_agent_source(&mut self, agent_key: &str, source: &crate::config::RoleSource) {
        let table = table_path_mut(&mut self.doc, &["roles".to_string(), agent_key.to_string()]);
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

        crate::config::persist::validate_workspace_file_stem(new)?;
        let value = self.workspace_docs.remove(old).expect("checked above");
        self.workspace_docs.insert(new.to_string(), value);
        self.removed_workspaces.insert(old.to_string());
        Ok(())
    }

    pub fn remove_workspace(&mut self, name: &str) -> anyhow::Result<()> {
        if self.workspace_docs.remove(name).is_none() {
            anyhow::bail!("workspace {name:?} not found");
        }
        self.removed_workspaces.insert(name.to_string());
        Ok(())
    }

    pub fn create_workspace(
        &mut self,
        name: &str,
        ws: crate::workspace::WorkspaceConfig,
    ) -> anyhow::Result<()> {
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

        self.workspace_docs.insert(name.to_string(), parsed);
        self.removed_workspaces.remove(name);

        Ok(())
    }

    pub fn edit_workspace(
        &mut self,
        name: &str,
        edit: crate::workspace::WorkspaceEdit,
    ) -> anyhow::Result<()> {
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
        self.workspace_docs.insert(name.to_string(), parsed);

        Ok(())
    }

    fn workspace_file(&self, name: &str) -> PathBuf {
        self.workspaces_dir.join(format!("{name}.toml"))
    }

    fn workspace_doc_mut(&mut self, workspace: &str) -> &mut DocumentMut {
        crate::config::persist::validate_workspace_file_stem(workspace)
            .expect("workspace name must be valid for split config filename");
        self.removed_workspaces.remove(workspace);
        self.workspace_docs
            .entry(workspace.to_string())
            .or_default()
    }

    fn doc_and_path_for_env_scope(&mut self, scope: &EnvScope) -> (&mut DocumentMut, Vec<String>) {
        match scope {
            EnvScope::Global | EnvScope::Role(_) => (&mut self.doc, env_scope_path(scope)),
            EnvScope::Workspace(w) => {
                let doc = self.workspace_doc_mut(w);
                (doc, vec!["env".to_string()])
            }
            EnvScope::WorkspaceRole { workspace, role } => {
                let doc = self.workspace_doc_mut(workspace);
                (
                    doc,
                    vec!["roles".to_string(), role.clone(), "env".to_string()],
                )
            }
            EnvScope::WorkspaceGithub(w) => {
                let doc = self.workspace_doc_mut(w);
                (doc, vec!["github".to_string(), "env".to_string()])
            }
            EnvScope::WorkspaceRoleGithub { workspace, role } => {
                let doc = self.workspace_doc_mut(workspace);
                (
                    doc,
                    vec![
                        "roles".to_string(),
                        role.clone(),
                        "github".to_string(),
                        "env".to_string(),
                    ],
                )
            }
        }
    }
}

const fn auth_forward_str(mode: crate::config::AuthForwardMode) -> &'static str {
    match mode {
        crate::config::AuthForwardMode::Ignore => "ignore",
        crate::config::AuthForwardMode::Sync => "sync",
        // Tasks 10/11 will split per-mode behavior; today both env-driven
        // modes serialize to their canonical snake_case names.
        crate::config::AuthForwardMode::ApiKey => "api_key",
        crate::config::AuthForwardMode::OAuthToken => "oauth_token",
    }
}

const fn github_mode_str(mode: crate::config::GithubAuthMode) -> &'static str {
    match mode {
        crate::config::GithubAuthMode::Sync => "sync",
        crate::config::GithubAuthMode::Token => "token",
        crate::config::GithubAuthMode::Ignore => "ignore",
    }
}

fn env_scope_path(scope: &EnvScope) -> Vec<String> {
    match scope {
        EnvScope::Global => vec!["env".to_string()],
        EnvScope::Role(a) => vec!["roles".to_string(), a.clone(), "env".to_string()],
        EnvScope::Workspace(w) => vec!["workspaces".to_string(), w.clone(), "env".to_string()],
        EnvScope::WorkspaceRole { workspace, role } => vec![
            "workspaces".to_string(),
            workspace.clone(),
            "roles".to_string(),
            role.clone(),
            "env".to_string(),
        ],
        EnvScope::WorkspaceGithub(w) => vec![
            "workspaces".to_string(),
            w.clone(),
            "github".to_string(),
            "env".to_string(),
        ],
        EnvScope::WorkspaceRoleGithub { workspace, role } => vec![
            "workspaces".to_string(),
            workspace.clone(),
            "roles".to_string(),
            role.clone(),
            "github".to_string(),
            "env".to_string(),
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
        crate::config::persist::validate_workspace_file_stem(name)?;
        let workspace = toml::from_str(&doc.to_string())
            .with_context(|| format!("deserializing candidate workspace {name:?}"))?;
        config.workspaces.insert(name.clone(), workspace);
    }
    crate::operator_env::validate_reserved_names(&config)?;
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
        crate::config::persist::validate_workspace_file_stem(stem)
            .with_context(|| format!("invalid workspace filename {}", path.display()))?;
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading workspace config {}", path.display()))?;
        let doc = raw
            .parse()
            .with_context(|| format!("parsing workspace config {}", path.display()))?;
        docs.insert(stem.to_string(), doc);
    }
    Ok(docs)
}

/// Remove the `auth_forward` field at `kind_path` (a `[…claude]` /
/// `[…codex]` / `[…github]` block). If the kind block is left empty
/// afterwards, the now-empty kind segment is peeled off too. Empty
/// `[…env]` subtables are removed by [`Self::remove_env_var`] when
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
///     from [`Self::remove_env_var`] with paths like
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
            .is_some_and(toml_edit::Table::is_empty);
        if !still_empty {
            return;
        }
        parent_table.remove(segment.as_str());
    }
}

fn table_path_mut<'a>(doc: &'a mut DocumentMut, path: &[String]) -> &'a mut Table {
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
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn workspace_file_contents(paths: &JackinPaths, name: &str) -> String {
        std::fs::read_to_string(paths.workspaces_dir.join(format!("{name}.toml"))).unwrap()
    }

    #[test]
    fn set_env_var_creates_global_env_table() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "").unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor
            .set_env_var(
                &EnvScope::Global,
                "API_TOKEN",
                "op://Personal/api/token".into(),
            )
            .unwrap();
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
        editor
            .set_env_var(
                &EnvScope::WorkspaceRole {
                    workspace: "prod".to_string(),
                    role: "agent-smith".to_string(),
                },
                "OPENAI_API_KEY",
                "op://Work/OpenAI/default".into(),
            )
            .unwrap();
        editor.save().unwrap();

        let out = workspace_file_contents(&paths, "prod");
        assert!(
            out.contains("[roles.agent-smith.env]"),
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
        editor
            .set_env_var(&EnvScope::Global, "API_TOKEN", "new-value".into())
            .unwrap();
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
        let removed = editor.remove_env_var(&EnvScope::Global, "API_TOKEN");
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
        let removed = editor.remove_env_var(&EnvScope::Global, "API_TOKEN");
        editor.save().unwrap();

        assert!(!removed);
    }

    #[test]
    fn remove_env_var_agent_scope() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[roles.agent-smith]
git = "https://example.com/a.git"
"#,
        )
        .unwrap();

        let scope = EnvScope::Role("agent-smith".to_string());
        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor
            .set_env_var(&scope, "LOG_LEVEL", "debug".into())
            .unwrap();
        assert!(
            editor.remove_env_var(&scope, "LOG_LEVEL"),
            "first remove should return true"
        );
        assert!(
            !editor.remove_env_var(&scope, "LOG_LEVEL"),
            "second remove should return false"
        );
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!out.contains("LOG_LEVEL"), "key not purged: {out}");
    }

    #[test]
    fn remove_env_var_workspace_scope() {
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

        let scope = EnvScope::Workspace("prod".to_string());
        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor
            .set_env_var(&scope, "DB_URL", "op://Work/Prod/db-url".into())
            .unwrap();
        assert!(
            editor.remove_env_var(&scope, "DB_URL"),
            "first remove should return true"
        );
        assert!(
            !editor.remove_env_var(&scope, "DB_URL"),
            "second remove should return false"
        );
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!out.contains("DB_URL"), "key not purged: {out}");
    }

    #[test]
    fn remove_env_var_workspace_agent_scope() {
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

        let scope = EnvScope::WorkspaceRole {
            workspace: "prod".to_string(),
            role: "agent-smith".to_string(),
        };
        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor
            .set_env_var(&scope, "OPENAI_API_KEY", "op://Work/OpenAI/default".into())
            .unwrap();
        assert!(
            editor.remove_env_var(&scope, "OPENAI_API_KEY"),
            "first remove should return true"
        );
        assert!(
            !editor.remove_env_var(&scope, "OPENAI_API_KEY"),
            "second remove should return false"
        );
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!out.contains("OPENAI_API_KEY"), "key not purged: {out}");
    }

    #[test]
    fn remove_env_var_leaves_sibling_keys_intact() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "").unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor
            .set_env_var(&EnvScope::Global, "KEY_A", "value-a".into())
            .unwrap();
        editor
            .set_env_var(&EnvScope::Global, "KEY_B", "value-b".into())
            .unwrap();
        editor.save().unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        assert!(editor.remove_env_var(&EnvScope::Global, "KEY_A"));
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!out.contains("KEY_A"), "KEY_A still present: {out}");
        assert!(
            out.contains(r#"KEY_B = "value-b""#),
            "sibling KEY_B gone: {out}"
        );
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
            &EnvScope::Global,
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
        editor.set_env_comment(&EnvScope::Global, "API_TOKEN", Some("new annotation"));
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
        editor.set_env_comment(&EnvScope::Global, "API_TOKEN", None);
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!out.contains("# some note"), "{out}");
        assert!(
            out.contains(r#"API_TOKEN = "x""#),
            "key still present: {out}"
        );
    }

    #[test]
    fn mutating_sibling_preserves_comment_above_other_key() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        let original = "[env]\n# rotate quarterly\nAPI_TOKEN = \"x\"\nOTHER = \"y\"\n";
        std::fs::write(&paths.config_file, original).unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor
            .set_env_var(&EnvScope::Global, "OTHER", "z".into())
            .unwrap();
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
        std::fs::write(&paths.config_file, "").unwrap();
        std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
        std::fs::write(
            paths.workspaces_dir.join("a.toml"),
            r#"# workspace a — keep this comment
workdir = "/a"
"#,
        )
        .unwrap();
        std::fs::write(
            paths.workspaces_dir.join("b.toml"),
            r#"# workspace b — also keep
workdir = "/b"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor
            .set_env_var(&EnvScope::Workspace("a".to_string()), "K", "v".into())
            .unwrap();
        editor.save().unwrap();

        let out = workspace_file_contents(&paths, "b");
        assert!(out.contains("# workspace b — also keep"), "{out}");
        let out_a = workspace_file_contents(&paths, "a");
        assert!(out_a.contains("K = \"v\""), "{out_a}");
        let global = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!global.contains("[workspaces."), "{global}");
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
            round_tripped.contains("[workspaces."),
            false,
            "global file should contain only global config after split:\n{round_tripped}"
        );
        assert!(paths.workspaces_dir.join("prod.toml").exists());
        assert!(paths.workspaces_dir.join("playground.toml").exists());
    }

    #[test]
    fn idempotent_save_is_byte_identical() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        let original = r#"version = "v1alpha1"
# Top-of-file note about this config
[claude]
auth_forward = "sync"

# Roles we trust
[roles.agent-smith]
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

        let global = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!global.contains("[workspaces."), "{global}");
        let workspace = workspace_file_contents(&paths, "prod");
        assert!(
            workspace.contains(r#"workdir = "/workspace/prod""#),
            "{workspace}"
        );
        assert!(
            workspace.contains(r#"API_TOKEN = "op://Personal/api/token""#),
            "{workspace}"
        );
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

        let tmp_path = paths.config_file.with_extension("tmp");
        assert!(!tmp_path.exists(), "expected .tmp to be renamed away");
    }

    /// `save()` must reject before rename so an invalid mutation
    /// can't brick subsequent CLI commands.
    #[test]
    fn save_rejects_invalid_candidate_and_preserves_on_disk_config() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        std::fs::write(&paths.config_file, "[env]\nVALID_KEY = \"valid-value\"\n").unwrap();
        AppConfig::load_or_init(&paths).unwrap();
        let baseline = std::fs::read_to_string(&paths.config_file).unwrap();

        // Inject `[roles.ghost.env]` without the required
        // `[roles.ghost].git` — fails serde parsing.
        let mut editor = ConfigEditor::open(&paths).unwrap();
        let agents_table = table_path_mut(
            &mut editor.doc,
            &["roles".to_string(), "ghost".to_string(), "env".to_string()],
        );
        agents_table.insert("LOG_LEVEL", toml_edit::value("debug"));

        let err = editor.save().unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("rejecting candidate config"),
            "expected rejection message; got: {msg}"
        );

        let after = std::fs::read_to_string(&paths.config_file).unwrap();
        assert_eq!(
            after, baseline,
            "rejected save must leave the on-disk config byte-identical"
        );

        // No leftover .tmp file.
        let tmp_path = paths.config_file.with_extension("tmp");
        assert!(
            !tmp_path.exists(),
            "rejected save must clean up its temp file at {}",
            tmp_path.display()
        );
    }

    #[test]
    fn save_rejects_reserved_name_candidate_and_preserves_on_disk_config() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        std::fs::write(&paths.config_file, "[env]\nVALID_KEY = \"v\"\n").unwrap();
        AppConfig::load_or_init(&paths).unwrap();
        let baseline = std::fs::read_to_string(&paths.config_file).unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        // Bypass the CLI pre-flight via the unchecked setter.
        editor
            .set_env_var(&EnvScope::Global, "DOCKER_HOST", "tcp://bad".into())
            .unwrap();

        let err = editor.save().unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("DOCKER_HOST") && msg.contains("reserved"),
            "expected reserved-name rejection; got: {msg}"
        );

        let after = std::fs::read_to_string(&paths.config_file).unwrap();
        assert_eq!(
            after, baseline,
            "rejected save must not touch on-disk config"
        );
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
                isolation: crate::isolation::MountIsolation::Shared,
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
                isolation: crate::isolation::MountIsolation::Shared,
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
        assert!(
            !out.contains("agent-smith"),
            "empty scope table should be gone: {out}"
        );
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
        assert!(
            out.contains("[docker.mounts.agent-smith]"),
            "scope table should still exist: {out}"
        );
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
            r#"[roles.my-role]
git = "https://example.com/a.git"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_agent_trust("my-role", true);
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains("trusted = true"), "{out}");
    }

    #[test]
    fn set_agent_trust_false_removes_field() {
        // Canonical TOML representation of trusted=false is absent (serde
        // skip_serializing_if on RoleSource::trusted).
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[roles.my-role]
git = "x"
trusted = true
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_agent_trust("my-role", false);
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!out.contains("trusted"), "{out}");
    }

    #[test]
    fn set_global_auth_forward_writes_per_agent_table() {
        for (agent, header) in [
            (crate::agent::Agent::Claude, "[claude]"),
            (crate::agent::Agent::Codex, "[codex]"),
            (crate::agent::Agent::Amp, "[amp]"),
        ] {
            let temp = tempdir().unwrap();
            let paths = JackinPaths::for_tests(temp.path());
            paths.ensure_base_dirs().unwrap();
            std::fs::write(&paths.config_file, "").unwrap();

            let mut editor = ConfigEditor::open(&paths).unwrap();
            editor.set_global_auth_forward(agent, crate::config::AuthForwardMode::Sync);
            editor.save().unwrap();

            let out = std::fs::read_to_string(&paths.config_file).unwrap();
            assert!(out.contains(header), "expected {header} in:\n{out}");
            assert!(out.contains(r#"auth_forward = "sync""#), "{out}");
        }
    }

    #[test]
    fn set_workspace_auth_forward_writes_workspace_agent_block() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"
[workspaces.proj]
workdir = "/tmp/proj"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_workspace_auth_forward(
            "proj",
            crate::agent::Agent::Claude,
            Some(crate::config::AuthForwardMode::ApiKey),
        );
        editor.save().unwrap();

        let out = workspace_file_contents(&paths, "proj");
        assert!(out.contains("[claude]"), "{out}");
        assert!(out.contains(r#"auth_forward = "api_key""#), "{out}");
    }

    #[test]
    fn set_workspace_auth_forward_clears_when_mode_none() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"
[workspaces.proj]
workdir = "/tmp/proj"

[workspaces.proj.claude]
auth_forward = "api_key"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_workspace_auth_forward("proj", crate::agent::Agent::Claude, None);
        editor.save().unwrap();

        let out = workspace_file_contents(&paths, "proj");
        assert!(
            !out.contains("[claude]"),
            "agent block must be removed when mode = None; {out}"
        );
        assert!(
            !out.contains("auth_forward"),
            "auth_forward field must be cleared; {out}"
        );
    }

    #[test]
    fn set_workspace_role_auth_forward_writes_role_agent_block() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"
[workspaces.proj]
workdir = "/tmp/proj"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_workspace_role_auth_forward(
            "proj",
            "smith",
            crate::agent::Agent::Codex,
            Some(crate::config::AuthForwardMode::ApiKey),
        );
        editor.save().unwrap();

        let out = workspace_file_contents(&paths, "proj");
        assert!(out.contains("[roles.smith.codex]"), "{out}");
        assert!(out.contains(r#"auth_forward = "api_key""#), "{out}");
    }

    #[test]
    fn set_workspace_role_auth_forward_clears_when_mode_none() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"
[workspaces.proj]
workdir = "/tmp/proj"

[workspaces.proj.roles.smith.claude]
auth_forward = "oauth_token"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_workspace_role_auth_forward("proj", "smith", crate::agent::Agent::Claude, None);
        editor.save().unwrap();

        let out = workspace_file_contents(&paths, "proj");
        assert!(!out.contains("[roles.smith.claude]"), "{out}");
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
        assert!(out.contains("[roles.agent-smith]"), "{out}");
        assert!(out.contains("trusted = true"), "{out}");
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
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..Default::default()
        };

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.create_workspace("new-ws", ws).unwrap();
        editor.save().unwrap();

        let out = workspace_file_contents(&paths, "new-ws");
        assert!(
            !std::fs::read_to_string(&paths.config_file)
                .unwrap()
                .contains("[workspaces.")
        );
        assert!(out.contains(r#"workdir = "/workspace/new""#), "{out}");
    }

    #[test]
    fn create_workspace_rejects_invalid_workdir_mount_combo() {
        // Editor delegates to AppConfig::create_workspace, which validates
        // that the workdir is equal-to / inside / parent-of some mount dst.
        // A workdir that doesn't line up with any mount dst must be rejected.
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        let mount_src = temp.path().join("src");
        std::fs::create_dir_all(&mount_src).unwrap();
        std::fs::write(&paths.config_file, "").unwrap();

        let ws = crate::workspace::WorkspaceConfig {
            workdir: "/elsewhere".to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: mount_src.display().to_string(),
                dst: "/workspace/unrelated".to_string(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..Default::default()
        };

        let mut editor = ConfigEditor::open(&paths).unwrap();
        let err = editor.create_workspace("bad-ws", ws).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("workspace") || msg.contains("mount") || msg.contains("workdir"),
            "expected validation error mentioning workspace/mount/workdir: {msg}"
        );
    }

    #[test]
    fn set_last_agent_preserves_other_fields() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        let original = r#"[workspaces.prod]
workdir = "/workspace/prod"
default_role = "agent-smith"
"#;
        std::fs::write(&paths.config_file, original).unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_last_agent("prod", "agent-smith");
        editor.save().unwrap();

        let out = workspace_file_contents(&paths, "prod");
        assert!(out.contains(r#"last_role = "agent-smith""#), "{out}");
        assert!(out.contains(r#"default_role = "agent-smith""#), "{out}");
    }

    #[test]
    fn upsert_agent_source_preserves_existing_env() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[roles.foo]
git = "OLD"

[roles.foo.env]
MY_VAR = "preserved"
"#,
        )
        .unwrap();

        let source = crate::config::RoleSource {
            git: "NEW".to_string(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        };
        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.upsert_agent_source("foo", &source);
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains(r#"git = "NEW""#), "{out}");
        assert!(out.contains(r#"MY_VAR = "preserved""#), "{out}");
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
        assert!(!paths.workspaces_dir.join("a.toml").exists());
        assert!(paths.workspaces_dir.join("b.toml").exists());
    }

    #[test]
    fn rename_workspace_preserves_nested_fields() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[workspaces.old-name]
workdir = "/a"

[[workspaces.old-name.mounts]]
src = "/s"
dst = "/a"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.rename_workspace("old-name", "new-name").unwrap();
        editor.save().unwrap();

        let out = workspace_file_contents(&paths, "new-name");
        assert!(!paths.workspaces_dir.join("old-name.toml").exists());
        assert!(
            out.contains(r#"workdir = "/a""#),
            "nested field preserved: {out}"
        );
        assert!(out.contains("[[mounts]]"), "array table preserved: {out}");
        assert!(!out.contains("old-name"), "{out}");
    }

    #[test]
    fn rename_workspace_write_failure_preserves_old_file() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
        std::fs::write(&paths.config_file, "").unwrap();
        std::fs::write(
            paths.workspaces_dir.join("old-name.toml"),
            r#"workdir = "/a"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.rename_workspace("old-name", "new-name").unwrap();
        std::fs::create_dir(paths.workspaces_dir.join("new-name.toml")).unwrap();

        let err = editor.save().unwrap_err();

        assert!(
            err.to_string().contains("Is a directory")
                || err.to_string().contains("is a directory"),
            "{err}"
        );
        assert!(
            paths.workspaces_dir.join("old-name.toml").exists(),
            "failed rename save must leave the original workspace file in place"
        );
    }

    #[test]
    fn rename_workspace_rejects_collision() {
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
        let err = editor.rename_workspace("a", "b").unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");
    }

    #[test]
    fn rename_workspace_rejects_empty_new_name() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "[workspaces.a]\nworkdir = \"/a\"\n").unwrap();
        let mut editor = ConfigEditor::open(&paths).unwrap();
        let err = editor.rename_workspace("a", "").unwrap_err();
        assert!(err.to_string().contains("empty"), "{err}");
    }

    #[test]
    fn set_env_var_writes_inline_table_for_op_ref() {
        use crate::operator_env::{EnvValue, OpRef};

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "[env]\n").unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor
            .set_env_var(
                &EnvScope::Global,
                "CLAUDE_CODE_OAUTH_TOKEN",
                EnvValue::OpRef(OpRef {
                    op: "op://abc/def/fld".into(),
                    path: "Private/Claude/security/auth token".into(),
                }),
            )
            .unwrap();
        editor.save().unwrap();

        let serialized = std::fs::read_to_string(&paths.config_file).unwrap();
        // Inline-table form, not a scalar string with quoted JSON.
        assert!(
            serialized.contains(r#"CLAUDE_CODE_OAUTH_TOKEN = { op = "op://abc/def/fld", path = "Private/Claude/security/auth token" }"#),
            "expected inline-table emit, got:\n{serialized}"
        );
    }

    #[test]
    fn set_env_var_writes_scalar_string_for_plain() {
        use crate::operator_env::EnvValue;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "[env]\n").unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor
            .set_env_var(
                &EnvScope::Global,
                "DB_URL",
                EnvValue::Plain("postgres://localhost".into()),
            )
            .unwrap();
        editor.save().unwrap();

        let serialized = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(
            serialized.contains(r#"DB_URL = "postgres://localhost""#),
            "expected scalar-string emit, got:\n{serialized}"
        );
    }

    /// Pin the cleanup path for the github kind: clearing both the
    /// `auth_forward` field and the `[github.env]` keys at workspace
    /// scope must leave NO empty `[workspaces.<ws>.github]` or
    /// `[workspaces.<ws>.github.env]` tables on disk. Regression guard
    /// for the orphan-table I1 finding.
    #[test]
    fn clearing_workspace_github_prunes_empty_tables() {
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

        // Seed: `[workspaces.prod.github]` with auth_forward + a
        // GH_TOKEN env entry.
        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor
            .set_workspace_github_auth_forward("prod", Some(crate::config::GithubAuthMode::Token));
        let env_scope = EnvScope::WorkspaceGithub("prod".to_string());
        editor
            .set_env_var(&env_scope, "GH_TOKEN", "op://Work/gh/pat".into())
            .unwrap();
        editor.save().unwrap();

        // Sanity: both the kind block and its env subtable land on disk.
        let after_save = workspace_file_contents(&paths, "prod");
        assert!(after_save.contains("[github]"));
        assert!(after_save.contains("auth_forward"));
        assert!(after_save.contains("GH_TOKEN"));

        // Operator presses `D` on github WorkspaceMode (mode → None)
        // and the env diff drops GH_TOKEN.
        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_workspace_github_auth_forward("prod", None);
        assert!(editor.remove_env_var(&env_scope, "GH_TOKEN"));
        editor.save().unwrap();

        let cleaned = workspace_file_contents(&paths, "prod");
        assert!(
            !cleaned.contains("github"),
            "stale [github] / [github.env] table left on disk:\n{cleaned}"
        );
        assert!(
            cleaned.contains("workdir"),
            "workspace block was wrongly removed by the cascade:\n{cleaned}"
        );
        assert!(
            cleaned.contains("workdir"),
            "sibling workdir field was wrongly stripped:\n{cleaned}"
        );
    }

    /// Same cascade contract for the per-(workspace × role) layer.
    #[test]
    fn clearing_workspace_role_github_prunes_empty_tables() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[workspaces.prod]
workdir = "/workspace/prod"

[workspaces.prod.roles.scratch]
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_workspace_role_github_auth_forward(
            "prod",
            "scratch",
            Some(crate::config::GithubAuthMode::Token),
        );
        let env_scope = EnvScope::WorkspaceRoleGithub {
            workspace: "prod".to_string(),
            role: "scratch".to_string(),
        };
        editor
            .set_env_var(&env_scope, "GH_TOKEN", "op://Work/gh/pat".into())
            .unwrap();
        editor.save().unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_workspace_role_github_auth_forward("prod", "scratch", None);
        assert!(editor.remove_env_var(&env_scope, "GH_TOKEN"));
        editor.save().unwrap();

        let cleaned = workspace_file_contents(&paths, "prod");
        assert!(
            !cleaned.contains("github"),
            "stale [github] / [github.env] table left on disk:\n{cleaned}"
        );
    }

    /// Clearing `[…github] auth_forward` while sibling kinds (`[…claude]` /
    /// `[…codex]`) are still set must NOT cascade-prune the siblings.
    #[test]
    fn clearing_one_kind_preserves_sibling_kinds() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[workspaces.prod]
workdir = "/workspace/prod"

[workspaces.prod.claude]
auth_forward = "ignore"

[workspaces.prod.codex]
auth_forward = "ignore"

[workspaces.prod.github]
auth_forward = "ignore"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_workspace_github_auth_forward("prod", None);
        editor.save().unwrap();

        let cleaned = workspace_file_contents(&paths, "prod");
        assert!(
            !cleaned.contains("[github]"),
            "github block should be removed:\n{cleaned}"
        );
        assert!(
            cleaned.contains("[claude]"),
            "claude block must survive:\n{cleaned}"
        );
        assert!(
            cleaned.contains("[codex]"),
            "codex block must survive:\n{cleaned}"
        );
    }

    /// Removing the last `[…github.env]` key while `[…github]` still
    /// has `auth_forward` set must prune ONLY `[…env]`. The kind block
    /// stays.
    #[test]
    fn pruning_empty_env_preserves_kind_block_with_auth_forward() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[workspaces.prod]
workdir = "/workspace/prod"

[workspaces.prod.github]
auth_forward = "token"

[workspaces.prod.github.env]
GH_TOKEN = "ghp_real"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        let env_scope = EnvScope::WorkspaceGithub("prod".to_string());
        assert!(editor.remove_env_var(&env_scope, "GH_TOKEN"));
        editor.save().unwrap();

        let cleaned = workspace_file_contents(&paths, "prod");
        assert!(
            !cleaned.contains("[github.env]"),
            "empty env subtable must be pruned:\n{cleaned}"
        );
        assert!(
            cleaned.contains("[github]"),
            "kind block must survive (still has auth_forward):\n{cleaned}"
        );
        assert!(
            cleaned.contains("auth_forward = \"token\""),
            "auth_forward value must survive:\n{cleaned}"
        );
    }

    /// Workspace with sibling content (allowed_roles, mounts) must
    /// survive a github clear. Position-based prune bound prevents
    /// the walker from reaching the workspace identifier slot.
    #[test]
    fn clearing_github_preserves_workspace_sibling_content() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[workspaces.prod]
workdir = "/workspace/prod"
allowed_roles = ["agent-smith", "the-architect"]

[workspaces.prod.github]
auth_forward = "token"

[workspaces.prod.github.env]
GH_TOKEN = "ghp_real"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_workspace_github_auth_forward("prod", None);
        let env_scope = EnvScope::WorkspaceGithub("prod".to_string());
        assert!(editor.remove_env_var(&env_scope, "GH_TOKEN"));
        editor.save().unwrap();

        let cleaned = workspace_file_contents(&paths, "prod");
        assert!(
            !cleaned.contains("[github"),
            "github / github.env tables should be pruned:\n{cleaned}"
        );
        assert!(
            cleaned.contains("workdir"),
            "workspace block must survive:\n{cleaned}"
        );
        assert!(
            cleaned.contains("workdir"),
            "workdir field must survive:\n{cleaned}"
        );
        assert!(
            cleaned.contains("allowed_roles"),
            "allowed_roles must survive:\n{cleaned}"
        );
    }

    /// Position-based prune protects against an operator workspace
    /// literally named "github" / "claude" / "codex" / "env" — the
    /// walk depth is bounded so the workspace identifier slot at
    /// path[1] is never reached.
    #[test]
    fn workspace_named_github_survives_github_clear() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[workspaces.github]
workdir = "/workspace/edge-case"

[workspaces.github.github]
auth_forward = "ignore"
"#,
        )
        .unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_workspace_github_auth_forward("github", None);
        editor.save().unwrap();

        let cleaned = workspace_file_contents(&paths, "github");
        // Inner [github] gone (kind block); workspace file preserved.
        assert!(
            cleaned.contains("workdir"),
            "workspace named 'github' must survive:\n{cleaned}"
        );
        assert!(
            cleaned.contains("workdir"),
            "workdir on workspace 'github' must survive:\n{cleaned}"
        );
    }
}
