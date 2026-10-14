// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::ConfigError;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use jackin_core::{EnvValue, JackinPaths, WorkspaceName};
use toml_edit::{DocumentMut, Item, Table};

use crate::accounts::account_source_fingerprint;
use crate::app_config::AppConfig;
use crate::app_config::persist::{
    load_config_contents, load_split_config_locked, validate_reserved_env_names,
};
use crate::auth::GithubAuthMode;
use crate::persist::{
    ConfigWriteGuard, StagedWrite, acquire_config_write_lock, commit_staged_config,
    publication_journal_path, stage_atomic_write, stage_delete, validate_workspace_file_stem,
};
use crate::schema::{MountConfig, WorkspaceConfig, WorkspaceEdit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvScope {
    Global,
    GlobalGithub,
    Role(String),
    Workspace(String),

    WorkspaceRole { workspace: String, role: String },

    WorkspaceGithub(String),

    WorkspaceRoleGithub { workspace: String, role: String },
}

#[derive(Debug, Default)]
pub struct BootstrapReport {
    pub fresh_install: bool,
    pub changed: bool,
    pub added_accounts: Vec<String>,
    pub added: Vec<(String, crate::AccountConfig)>,
    pub issues: Vec<crate::DiscoveryIssue>,
    pub unapplied_zshrc_models: Vec<crate::ModelProfile>,
    pub unapplied_zshrc_wrappers: Vec<crate::WrapperCallSite>,
    pub unapplied_zshrc_xdg_roots: Vec<crate::XdgRoots>,
}

fn profile_account_candidate(
    id: String,
    agent: jackin_core::Agent,
    provider: crate::AiProvider,
    directory: PathBuf,
    name: String,
    xdg_roots: Option<crate::XdgRoots>,
    source_selector: Option<crate::ProfileSelector>,
) -> (String, crate::AccountConfig) {
    (
        id,
        crate::AccountConfig {
            enabled: true,
            name,
            provider,
            credential: crate::AccountCredential::Profile {
                agent,
                directory,
                xdg_roots,
                source_selector,
            },
        },
    )
}

fn profile_scan_candidate(
    discovered: &crate::DiscoveredAccount,
) -> Option<(String, crate::AccountConfig)> {
    let provider = discovered.provider.or_else(|| {
        (discovered.agent != jackin_core::Agent::Opencode)
            .then(|| crate::AiProvider::for_agent(discovered.agent))
            .flatten()
    })?;
    let id = if discovered.agent == jackin_core::Agent::Opencode {
        format!("default-opencode-{}", provider.slug())
    } else {
        format!("default-{}", discovered.agent.slug())
    };
    Some(profile_account_candidate(
        id,
        discovered.agent,
        provider,
        discovered.directory.clone(),
        if discovered.agent == jackin_core::Agent::Opencode {
            format!("OpenCode {provider} default")
        } else {
            format!("{} default", discovered.agent.label())
        },
        None,
        discovered.source_selector.clone(),
    ))
}

fn zshrc_provider(stem: &str) -> Option<crate::AiProvider> {
    crate::AiProvider::ALL
        .iter()
        .copied()
        .find(|provider| provider.slug() == stem)
}

fn env_scan_candidate(
    provider: crate::AiProvider,
    variable: &str,
    base_url: Option<String>,
) -> (String, crate::AccountConfig) {
    api_key_scan_candidate(provider, EnvValue::from(format!("${variable}")), base_url)
}

fn api_key_scan_candidate(
    provider: crate::AiProvider,
    value: EnvValue,
    base_url: Option<String>,
) -> (String, crate::AccountConfig) {
    let id = format!("{}-api-key", provider.slug());
    let account = crate::AccountConfig {
        enabled: true,
        name: format!("{provider} API key"),
        provider,
        credential: crate::AccountCredential::ApiKey {
            value,
            base_url,
            model: None,
        },
    };
    (id, account)
}

fn oauth_scan_candidate(
    agent: jackin_core::Agent,
    variable: &str,
) -> Option<(String, crate::AccountConfig)> {
    oauth_scan_candidate_with_value(agent, EnvValue::from(format!("${variable}")))
}

fn oauth_scan_candidate_with_value(
    agent: jackin_core::Agent,
    value: EnvValue,
) -> Option<(String, crate::AccountConfig)> {
    let provider = crate::AiProvider::for_agent(agent)?;
    let id = format!("{agent}-oauth-token");
    let account = crate::AccountConfig {
        enabled: true,
        name: format!("{agent} subscription token"),
        provider,
        credential: crate::AccountCredential::OAuthToken { agent, value },
    };
    Some((id, account))
}

fn bootstrap_scan_accounts(config: &mut AppConfig, home: &Path) -> BootstrapReport {
    let mut report = BootstrapReport::default();
    let scan = crate::discover_default_accounts(home);
    report.issues = scan.issues;
    let mut register = |id: String, account: crate::AccountConfig| {
        if scan_candidate_is_blocked(
            &config.accounts,
            &config.account_scan_exclusions,
            &id,
            &account,
        ) {
            return;
        }
        config.accounts.insert(id.clone(), account.clone());
        report.added_accounts.push(id.clone());
        report.added.push((id, account));
        report.changed = true;
    };
    for discovered in scan.accounts {
        if let Some((id, account)) = profile_scan_candidate(&discovered) {
            register(id, account);
        }
    }
    let environment = std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    for candidate in
        crate::accounts::discovery::discover_environment_account_candidates(&environment)
    {
        let (id, account) =
            env_scan_candidate(candidate.provider, &candidate.variable, candidate.base_url);
        register(id, account);
    }
    for (agent, variable) in crate::discover_environment_oauth_accounts(&environment) {
        if let Some((id, account)) = oauth_scan_candidate(agent, &variable) {
            register(id, account);
        }
    }
    report
}

fn scan_source_registered(
    accounts: &BTreeMap<String, crate::AccountConfig>,
    candidate: &crate::AccountConfig,
) -> bool {
    let candidate_fingerprint = account_source_fingerprint(candidate);
    accounts
        .values()
        .any(|registered| account_source_fingerprint(registered) == candidate_fingerprint)
}

fn scan_candidate_is_blocked(
    known: &BTreeMap<String, crate::AccountConfig>,
    excluded: &BTreeSet<String>,
    id: &str,
    account: &crate::AccountConfig,
) -> bool {
    known.contains_key(id)
        || excluded.contains(&account_source_fingerprint(account))
        || scan_source_registered(known, account)
}

fn has_fresh_install_marker(path: &Path) -> crate::ConfigResult<bool> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(anyhow::Error::new(error)
                .context(format!("reading {}", path.display()))
                .into());
        }
    };
    let doc: DocumentMut = raw
        .parse()
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(doc
        .get("bootstrap")
        .and_then(Item::as_table_like)
        .and_then(|table| table.get("fresh_install"))
        .and_then(Item::as_bool)
        .unwrap_or(false))
}

#[derive(Debug)]
pub struct ConfigEditor {
    _lock: ConfigWriteGuard,
    home_dir: PathBuf,
    doc: DocumentMut,
    path: PathBuf,
    workspaces_dir: PathBuf,
    workspace_docs: BTreeMap<String, DocumentMut>,
    removed_workspaces: BTreeSet<String>,
}

fn apply_xdg_profile_candidate(
    editor: &mut ConfigEditor,
    known: &mut BTreeMap<String, crate::AccountConfig>,
    excluded: &BTreeSet<String>,
    report: &mut BootstrapReport,
    roots: &crate::XdgRoots,
    candidate: Option<(String, crate::AccountConfig)>,
) -> crate::ConfigResult<()> {
    let Some((id, account)) = candidate else {
        report.unapplied_zshrc_xdg_roots.push(roots.clone());
        return Ok(());
    };
    let is_excluded = excluded.contains(&account_source_fingerprint(&account));
    let source_registered = scan_source_registered(known, &account);
    if scan_candidate_is_blocked(known, excluded, &id, &account) {
        if known.contains_key(&id) && !is_excluded && !source_registered {
            report.unapplied_zshrc_xdg_roots.push(roots.clone());
        }
        return Ok(());
    }
    editor.upsert_account(&id, &account)?;
    known.insert(id.clone(), account.clone());
    report.added_accounts.push(id.clone());
    report.added.push((id, account));
    report.changed = true;
    Ok(())
}

impl ConfigEditor {
    pub fn open(paths: &JackinPaths) -> crate::ConfigResult<Self> {
        Self::open_detailed(paths).map(|(editor, _)| editor)
    }

    pub fn open_detailed(paths: &JackinPaths) -> crate::ConfigResult<(Self, BootstrapReport)> {
        let lock = acquire_config_write_lock(&paths.config_file)?;
        Self::open_with_lock(paths, lock)
    }

    pub(crate) fn open_with_lock(
        paths: &JackinPaths,
        lock: ConfigWriteGuard,
    ) -> crate::ConfigResult<(Self, BootstrapReport)> {
        paths.ensure_base_dirs()?;
        let mut report = BootstrapReport::default();
        let initial_contents = if paths.config_file.exists() {
            None
        } else {
            let mut initial = AppConfig::default();
            initial.sync_builtin_agents();
            report = bootstrap_scan_accounts(&mut initial, &paths.home_dir);
            report.fresh_install = true;
            initial.bootstrap = Some(crate::BootstrapState::initialized());
            initial.validate_accounts()?;
            Some(toml::to_string_pretty(&initial)?)
        };
        let raw = match initial_contents.as_ref() {
            Some(contents) => Some(contents.clone()),
            None => load_config_contents(paths)?,
        };
        let mut loaded = load_split_config_locked(paths, raw)?;
        if let Some(contents) = initial_contents {
            loaded.add_pending_write(paths.config_file.clone(), contents);
        }
        if !report.fresh_install && has_fresh_install_marker(&paths.config_file)? {
            let serialized = {
                let config = loaded.config_mut();
                report = bootstrap_scan_accounts(&mut *config, &paths.home_dir);
                report.fresh_install = true;
                config.bootstrap = Some(crate::BootstrapState::initialized());
                config.validate_accounts()?;
                let mut snapshot = config.clone();
                snapshot.workspaces.clear();
                toml::to_string_pretty(&snapshot)?
            };
            loaded.add_pending_write(paths.config_file.clone(), serialized);
        }
        if loaded.has_pending_writes() {
            loaded.validate_for_editor()?;
        }
        drop(loaded.commit(&publication_journal_path(&paths.config_file))?);
        let raw = std::fs::read_to_string(&paths.config_file)
            .with_context(|| format!("reading {}", paths.config_file.display()))?;
        let doc: DocumentMut = raw
            .parse()
            .with_context(|| format!("parsing {}", paths.config_file.display()))?;
        let workspace_docs = load_workspace_docs(paths)?;
        let editor = Self {
            _lock: lock,
            home_dir: paths.home_dir.clone(),
            doc,
            path: paths.config_file.clone(),
            workspaces_dir: paths.workspaces_dir.clone(),
            workspace_docs,
            removed_workspaces: BTreeSet::new(),
        };
        Ok((editor, report))
    }

    pub fn scan_for_accounts(&mut self) -> crate::ConfigResult<BootstrapReport> {
        let home = self.home_dir.clone();
        let mut codex_home = None;
        let environment = std::env::vars_os()
            .filter_map(|(name, value)| {
                if name == OsStr::new("CODEX_HOME") {
                    codex_home = Some(value.clone());
                }
                Some((name.into_string().ok()?, value.into_string().ok()?))
            })
            .collect();
        self.scan_for_accounts_with_codex_home(&home, &environment, codex_home.as_deref())
    }

    #[cfg(test)]
    fn scan_for_accounts_with(
        &mut self,
        home: &Path,
        environment: &BTreeMap<String, String>,
    ) -> crate::ConfigResult<BootstrapReport> {
        let codex_home = environment.get("CODEX_HOME").map(OsStr::new);
        self.scan_for_accounts_with_codex_home(home, environment, codex_home)
    }

    fn scan_for_accounts_with_codex_home(
        &mut self,
        home: &Path,
        environment: &BTreeMap<String, String>,
        codex_home: Option<&OsStr>,
    ) -> crate::ConfigResult<BootstrapReport> {
        let mut report = BootstrapReport::default();
        let scan =
            crate::accounts::discovery::discover_default_accounts_with_codex_home(home, codex_home);
        report.issues = scan.issues;
        let mut candidates = Vec::new();
        for discovered in scan.accounts {
            if let Some(candidate) = profile_scan_candidate(&discovered) {
                candidates.push(candidate);
            }
        }
        for candidate in
            crate::accounts::discovery::discover_environment_account_candidates(environment)
        {
            candidates.push(env_scan_candidate(
                candidate.provider,
                &candidate.variable,
                candidate.base_url,
            ));
        }
        for (agent, variable) in crate::discover_environment_oauth_accounts(environment) {
            if let Some(candidate) = oauth_scan_candidate(agent, &variable) {
                candidates.push(candidate);
            }
        }
        let existing: AppConfig = toml::from_str(&self.doc.to_string())?;
        let excluded = existing.account_scan_exclusions;
        let mut known = existing.accounts;
        for (id, account) in candidates {
            if scan_candidate_is_blocked(&known, &excluded, &id, &account) {
                continue;
            }
            self.upsert_account(&id, &account)?;
            known.insert(id.clone(), account.clone());
            report.added_accounts.push(id.clone());
            report.added.push((id, account));
            report.changed = true;
        }
        Ok(report)
    }

    pub fn apply_zshrc_plan(
        &mut self,
        plan: &crate::ZshrcImportPlan,
    ) -> crate::ConfigResult<BootstrapReport> {
        let mut report = BootstrapReport::default();
        let existing: AppConfig = toml::from_str(&self.doc.to_string())?;
        let excluded = existing.account_scan_exclusions;
        let mut known = existing.accounts;
        let home = self.home_dir.clone();
        self.apply_zshrc_directories(&plan.directories, &home, &mut known, &excluded, &mut report)?;
        self.apply_zshrc_xdg_roots(
            plan.xdg_roots.as_ref(),
            &home,
            &mut known,
            &excluded,
            &mut report,
        )?;
        self.apply_zshrc_op_refs(
            &plan.op_refs,
            &plan.models,
            &mut known,
            &excluded,
            &mut report,
        )?;
        self.apply_zshrc_models(&plan.models, &mut known, &mut report)?;
        report.unapplied_zshrc_wrappers = plan.wrappers.clone();
        Ok(report)
    }

    fn register_zshrc_account(
        &mut self,
        known: &mut BTreeMap<String, crate::AccountConfig>,
        excluded: &BTreeSet<String>,
        report: &mut BootstrapReport,
        id: String,
        account: crate::AccountConfig,
    ) -> crate::ConfigResult<()> {
        if scan_candidate_is_blocked(known, excluded, &id, &account) {
            return Ok(());
        }
        self.upsert_account(&id, &account)?;
        known.insert(id.clone(), account.clone());
        report.added_accounts.push(id.clone());
        report.added.push((id, account));
        report.changed = true;
        Ok(())
    }

    fn apply_zshrc_directories(
        &mut self,
        directories: &[crate::DirectoryCandidate],
        home: &Path,
        known: &mut BTreeMap<String, crate::AccountConfig>,
        excluded: &BTreeSet<String>,
        report: &mut BootstrapReport,
    ) -> crate::ConfigResult<()> {
        for directory in directories {
            let Some(provider) = crate::AiProvider::for_agent(directory.agent) else {
                continue;
            };
            match crate::discover_account_directory(directory.agent, &directory.directory, home) {
                Ok(Some(found)) => {
                    let id = format!("custom-{}", directory.agent.slug());
                    let account = crate::AccountConfig {
                        enabled: true,
                        name: format!("{} custom", directory.agent.label()),
                        provider,
                        credential: crate::AccountCredential::Profile {
                            agent: directory.agent,
                            directory: found.directory,
                            xdg_roots: None,
                            source_selector: found.source_selector,
                        },
                    };
                    self.register_zshrc_account(known, excluded, report, id, account)?;
                }
                Ok(None) => {}
                Err(error) => report.issues.push(crate::DiscoveryIssue {
                    agent: directory.agent,
                    directory: directory.directory.clone(),
                    error,
                }),
            }
        }
        Ok(())
    }

    fn apply_zshrc_xdg_roots(
        &mut self,
        roots: Option<&crate::XdgRoots>,
        home: &Path,
        known: &mut BTreeMap<String, crate::AccountConfig>,
        excluded: &BTreeSet<String>,
        report: &mut BootstrapReport,
    ) -> crate::ConfigResult<()> {
        let Some(roots) = roots else {
            return Ok(());
        };
        let opencode_data = roots.data.join("opencode");
        let opencode_config = roots.config.join("opencode");
        if opencode_data.exists() || opencode_config.exists() {
            report.unapplied_zshrc_xdg_roots.push(roots.clone());
            report.issues.push(crate::DiscoveryIssue {
                agent: jackin_core::Agent::Opencode,
                directory: opencode_data,
                error: crate::DiscoveryError::Unsupported(
                    "OpenCode XDG roots from shell imports require an explicit profile directory",
                ),
            });
        } else {
            let directory = roots.data.join("amp");
            let provider = crate::AiProvider::Amp;
            match crate::discover_account_directory(jackin_core::Agent::Amp, &directory, home) {
                Ok(Some(found)) => {
                    let candidate = profile_account_candidate(
                        "custom-amp".to_owned(),
                        jackin_core::Agent::Amp,
                        provider,
                        found.directory,
                        "Amp custom".to_owned(),
                        Some(roots.clone()),
                        found.source_selector,
                    );
                    apply_xdg_profile_candidate(
                        self,
                        known,
                        excluded,
                        report,
                        roots,
                        Some(candidate),
                    )?;
                }
                Ok(None) => report.unapplied_zshrc_xdg_roots.push(roots.clone()),
                Err(error) => report.issues.push(crate::DiscoveryIssue {
                    agent: jackin_core::Agent::Amp,
                    directory,
                    error,
                }),
            }
        }
        Ok(())
    }

    fn apply_zshrc_op_refs(
        &mut self,
        op_refs: &[crate::OpReadCandidate],
        models: &[crate::ModelProfile],
        known: &mut BTreeMap<String, crate::AccountConfig>,
        excluded: &BTreeSet<String>,
        report: &mut BootstrapReport,
    ) -> crate::ConfigResult<()> {
        for op_ref in op_refs {
            if op_ref.reference.on_demand {
                continue;
            }
            let value = EnvValue::OpRef(op_ref.reference.clone());
            let probe = BTreeMap::from([(op_ref.var.clone(), String::from("1"))]);
            let seeded = if crate::discover_environment_oauth_accounts(&probe).is_empty() {
                crate::discover_environment_accounts(&probe)
                    .into_iter()
                    .next()
                    .map(|(provider, _)| {
                        let base_url = models
                            .iter()
                            .find(|model| zshrc_provider(&model.name) == Some(provider))
                            .and_then(|model| model.base_url.clone());
                        api_key_scan_candidate(provider, value, base_url)
                    })
            } else {
                oauth_scan_candidate_with_value(jackin_core::Agent::Claude, value)
            };
            let Some((id, account)) = seeded else {
                continue;
            };
            self.register_zshrc_account(known, excluded, report, id, account)?;
        }
        Ok(())
    }

    fn apply_zshrc_models(
        &mut self,
        models: &[crate::ModelProfile],
        known: &mut BTreeMap<String, crate::AccountConfig>,
        report: &mut BootstrapReport,
    ) -> crate::ConfigResult<()> {
        for model in models {
            let Some(provider) = zshrc_provider(&model.name) else {
                report.unapplied_zshrc_models.push(model.clone());
                continue;
            };
            let id = format!("{}-api-key", provider.slug());
            let Some(existing) = known.get(&id).cloned() else {
                report.unapplied_zshrc_models.push(model.clone());
                continue;
            };
            let mut account = existing.clone();
            let crate::AccountCredential::ApiKey {
                model: account_model,
                base_url: account_url,
                ..
            } = &mut account.credential
            else {
                report.unapplied_zshrc_models.push(model.clone());
                continue;
            };
            if model.model.is_some() {
                *account_model = model.model.clone();
            }
            if model.base_url.is_some() {
                *account_url = model.base_url.clone();
            }
            if account != existing {
                self.upsert_account(&id, &account)?;
                known.insert(id, account);
                report.changed = true;
            }
        }
        Ok(())
    }

    pub fn save(self) -> crate::ConfigResult<AppConfig> {
        self.save_with_stager(stage_atomic_write)
    }

    fn save_with_stager<F>(self, mut stage: F) -> crate::ConfigResult<AppConfig>
    where
        F: FnMut(&Path, &str) -> crate::ConfigResult<StagedWrite>,
    {
        for name in self
            .workspace_docs
            .keys()
            .chain(self.removed_workspaces.iter())
        {
            validate_workspace_file_stem(name)?;
        }
        let global_contents = self.doc.to_string();

        let candidate = validate_candidate(&global_contents, &self.workspace_docs).map_err(|err| {
            ConfigError::from(err.context(format!(
                "rejecting candidate config (would have written to {})",
                self.path.display()
            )))
        });
        let config = crate::telemetry::finish_operation(
            jackin_telemetry::schema::enums::ConfigScope::Global,
            jackin_telemetry::schema::enums::ConfigOperation::Validate,
            candidate,
        )?;

        crate::telemetry::finish_operation(
            jackin_telemetry::schema::enums::ConfigScope::Global,
            jackin_telemetry::schema::enums::ConfigOperation::Save,
            (|| {
                std::fs::create_dir_all(&self.workspaces_dir)?;
                let mut staged = Vec::with_capacity(self.workspace_docs.len() + 1);
                staged.push(stage(&self.path, &global_contents)?);
                for (name, doc) in &self.workspace_docs {
                    staged.push(stage(&self.workspace_file(name), &doc.to_string())?);
                }
                let mut deletes = Vec::with_capacity(self.removed_workspaces.len());
                for removed in &self.removed_workspaces {
                    if let Some(delete) = stage_delete(&self.workspace_file(removed))? {
                        deletes.push(delete);
                    }
                }
                let journal = publication_journal_path(&self.path);
                commit_staged_config(&journal, &mut staged, &mut deletes)?;
                Ok(config)
            })(),
        )
    }

    pub fn set_env_var(
        &mut self,
        scope: &EnvScope,
        key: &str,
        value: EnvValue,
    ) -> crate::ConfigResult<()> {
        use jackin_core::EnvValue;
        use toml_edit::{InlineTable, Item, Value, value as toml_value};

        if jackin_core::is_account_env(key) {
            return Err(ConfigError::msg(format_args!(
                "env name {key:?} belongs to account credentials and cannot be set here"
            )));
        }

        let (doc, path) = self.doc_and_path_for_env_scope(scope);
        let table = table_path_mut(doc, &path);
        let item = match value {
            EnvValue::Plain(s) => toml_value(s),
            EnvValue::OpRef(r) => {
                let mut tbl = InlineTable::new();
                tbl.insert("op", Value::from(r.op));
                let mut breadcrumb = InlineTable::new();
                breadcrumb.insert("version", Value::from(1));
                breadcrumb.insert("value", Value::from(r.path));
                tbl.insert("breadcrumb", Value::InlineTable(breadcrumb));
                if let Some(account) = r.account {
                    tbl.insert("account", Value::from(account));
                }
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
                    toml_value(e.value)
                }
            }
        };
        table.insert(key, item);
        Ok(())
    }

    pub fn set_env_comment(&mut self, scope: &EnvScope, key: &str, comment: Option<&str>) {
        let (doc, path) = self.doc_and_path_for_env_scope(scope);
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

    pub fn add_mount(&mut self, name: &str, mount: MountConfig, scope: Option<&str>) {
        match scope {
            None => {
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
                let scoped_table = table_path_mut(
                    &mut self.doc,
                    &[
                        "docker".to_owned(),
                        "mounts".to_owned(),
                        scope_key.to_owned(),
                    ],
                );
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
            table.remove("trusted");
        }
    }

    pub fn set_global_github_auth_forward(&mut self, mode: GithubAuthMode) {
        let table = table_path_mut(&mut self.doc, &["github".to_owned()]);
        table.insert("auth_forward", toml_edit::value(github_mode_str(mode)));
    }

    pub fn set_global_github_env_var(
        &mut self,
        key: &str,
        value: EnvValue,
    ) -> crate::ConfigResult<()> {
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

    pub fn set_workspace_github_auth_forward(
        &mut self,
        workspace: &WorkspaceName,
        mode: Option<GithubAuthMode>,
    ) {
        let github_path = vec!["github".to_owned()];
        let doc = self.workspace_doc_mut(workspace.as_str());
        if let Some(m) = mode {
            let table = table_path_mut(doc, &github_path);
            table.insert("auth_forward", toml_edit::value(github_mode_str(m)));
        } else {
            clear_auth_forward_field(doc, &github_path);
        }
    }

    pub fn set_workspace_role_github_auth_forward(
        &mut self,
        workspace: &WorkspaceName,
        role: &str,
        mode: Option<GithubAuthMode>,
    ) {
        let github_path = vec!["roles".to_owned(), role.to_owned(), "github".to_owned()];
        let doc = self.workspace_doc_mut(workspace.as_str());
        if let Some(m) = mode {
            let table = table_path_mut(doc, &github_path);
            table.insert("auth_forward", toml_edit::value(github_mode_str(m)));
        } else {
            clear_auth_forward_field(doc, &github_path);
        }
    }

    pub fn upsert_builtin_agent(&mut self, agent_key: &str, git_url: &str) {
        let table = table_path_mut(&mut self.doc, &["roles".to_owned(), agent_key.to_owned()]);
        table.insert("git", toml_edit::value(git_url));
        table.insert("trusted", toml_edit::value(true));
    }

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
            prune_empty_trailing_tables(doc, &path, 2);
        }
        removed
    }

    pub fn set_last_agent(&mut self, workspace: &WorkspaceName, agent_key: &str) {
        let doc = self.workspace_doc_mut(workspace.as_str());
        let table = table_path_mut(doc, &[]);
        table.insert("last_role", toml_edit::value(agent_key));
    }

    pub fn rename_workspace(
        &mut self,
        old: &WorkspaceName,
        new: &WorkspaceName,
    ) -> crate::ConfigResult<()> {
        if old == new {
            return Ok(());
        }
        if !self.workspace_docs.contains_key(old.as_str()) {
            return Err(ConfigError::WorkspaceNotFound(old.as_str().to_owned()));
        }
        if self.workspace_docs.contains_key(new.as_str()) {
            return Err(ConfigError::WorkspaceAlreadyExists(new.as_str().to_owned()));
        }

        let Some(value) = self.workspace_docs.remove(old.as_str()) else {
            return Err(ConfigError::WorkspaceNotFound(old.as_str().to_owned()));
        };
        self.workspace_docs.insert(new.as_str().to_owned(), value);
        self.removed_workspaces.insert(old.as_str().to_owned());
        Ok(())
    }

    pub fn remove_workspace(&mut self, name: &WorkspaceName) -> crate::ConfigResult<()> {
        if self.workspace_docs.remove(name.as_str()).is_none() {
            return Err(ConfigError::WorkspaceNotFound(name.as_str().to_owned()));
        }
        self.removed_workspaces.insert(name.as_str().to_owned());
        Ok(())
    }

    pub fn create_workspace(
        &mut self,
        name: &WorkspaceName,
        ws: WorkspaceConfig,
    ) -> crate::ConfigResult<()> {
        let mut in_memory = validate_candidate(&self.doc.to_string(), &self.workspace_docs)
            .context("re-parsing current docs into AppConfig for workspace creation")?;
        in_memory.create_workspace(name, ws)?;
        let inserted = in_memory.workspaces.get(name.as_str()).ok_or_else(|| {
            anyhow::Error::from(ConfigError::WorkspaceDisappearedAfterCreate(
                name.as_str().to_owned(),
            ))
        })?;

        let rendered =
            toml::to_string(inserted).with_context(|| format!("serializing workspace {name:?}"))?;
        let parsed: DocumentMut = rendered
            .parse()
            .with_context(|| format!("re-parsing serialized workspace {name:?}"))?;

        self.workspace_docs.insert(name.as_str().to_owned(), parsed);
        self.removed_workspaces.remove(name.as_str());

        Ok(())
    }

    pub fn edit_workspace(
        &mut self,
        name: &WorkspaceName,
        edit: WorkspaceEdit,
    ) -> crate::ConfigResult<()> {
        let mut in_memory = validate_candidate(&self.doc.to_string(), &self.workspace_docs)
            .context("re-parsing current docs into AppConfig for workspace edit")?;

        in_memory.edit_workspace(name, edit)?;

        let updated = in_memory.workspaces.get(name.as_str()).ok_or_else(|| {
            anyhow::Error::from(ConfigError::WorkspaceDisappearedAfterEdit(
                name.as_str().to_owned(),
            ))
        })?;

        let rendered = toml::to_string(updated)?;
        let parsed: DocumentMut = rendered.parse()?;
        self.workspace_docs.insert(name.as_str().to_owned(), parsed);

        Ok(())
    }

    pub fn insert_at_path(&mut self, path: &[String], key: &str, value: &str) {
        let table = table_path_mut(&mut self.doc, path);
        table.insert(key, toml_edit::value(value));
    }

    fn workspace_file(&self, name: &str) -> PathBuf {
        self.workspaces_dir.join(format!("{name}.toml"))
    }

    fn workspace_doc_mut(&mut self, workspace: &str) -> &mut DocumentMut {
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
                let doc = self.workspace_doc_mut(workspace.as_str());
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
                let doc = self.workspace_doc_mut(workspace.as_str());
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

fn validate_candidate(
    global_contents: &str,
    workspace_docs: &BTreeMap<String, DocumentMut>,
) -> anyhow::Result<AppConfig> {
    let mut config: AppConfig =
        toml::from_str(global_contents).context("deserializing candidate global config")?;
    if !config.workspaces.is_empty() {
        return Err(ConfigError::GlobalHasWorkspacesTable.into());
    }
    for (name, doc) in workspace_docs {
        validate_workspace_file_stem(name)?;
        let workspace: WorkspaceConfig = toml::from_str(&doc.to_string())
            .with_context(|| format!("deserializing candidate workspace {name:?}"))?;

        config.workspaces.insert(name.clone(), workspace);
    }
    validate_reserved_env_names(&config)?;
    config.validate_accounts()?;
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
        let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
            anyhow::Error::from(ConfigError::InvalidWorkspaceFilename(
                path.display().to_string(),
            ))
        })?;
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
    prune_empty_trailing_tables(doc, kind_path, 1);
}

fn prune_empty_trailing_tables(doc: &mut DocumentMut, path: &[String], max_prune: usize) {
    let stop_at = path.len().saturating_sub(max_prune);
    for i in (stop_at..path.len()).rev() {
        let Some(segment) = path.get(i) else {
            return;
        };
        let parent_path = path.get(..i).unwrap_or(&[]);
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
        let Some((first, rest)) = path.split_first() else {
            return table;
        };
        let entry = table.entry(first).or_insert(Item::Table(Table::new()));
        walk(entry, rest)
    }
    walk(doc.as_item_mut(), path)
}

#[cfg(test)]
mod tests;

mod accounts;
