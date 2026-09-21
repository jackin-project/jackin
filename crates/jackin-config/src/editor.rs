// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Comment-preserving config writer.
//!
//! Reads still go through `AppConfig::load_or_init` (serde + `toml`).
//! Writes go through `ConfigEditor::open → mutate → save`, which keeps
//! user-written comments, blank lines, and key ordering intact in
//! sections untouched by the mutation.

use crate::ConfigError;
use std::collections::{BTreeMap, BTreeSet};
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

/// Which env map a setter/remover targets in the config tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvScope {
    /// Top-level `[env]` in `config.toml`.
    Global,
    /// Top-level `[github.env]` in `config.toml`.
    GlobalGithub,
    /// `[roles.<name>.env]` in `config.toml`.
    Role(String),
    /// `[env]` inside a split workspace file.
    Workspace(String),
    /// `[roles.<role>.env]` inside a split workspace file.
    WorkspaceRole {
        /// Workspace file stem.
        workspace: String,
        /// Role key under that workspace.
        role: String,
    },
    /// `[github.env]` inside the workspace file — the github-kind env
    /// block, parallel to the regular workspace `env` map but read by
    /// [`crate::build_github_env_layers`] instead of the regular launch-time
    /// env merge. Used to thread `GH_TOKEN` / `GH_HOST` /
    /// `GH_ENTERPRISE_TOKEN` without polluting the agent-facing env map.
    WorkspaceGithub(String),
    /// `[roles.<role>.github.env]` inside the workspace file — most
    /// specific layer of the github env layering.
    WorkspaceRoleGithub {
        /// Workspace file stem.
        workspace: String,
        /// Role key under that workspace.
        role: String,
    },
}

/// Outcome of a first-run bootstrap scan: registered account IDs plus
/// every discovery issue (surfaced to CLI/Settings callers, never dropped).
#[derive(Debug, Default)]
pub struct BootstrapReport {
    /// True when a fresh-install scan ran during this open.
    pub fresh_install: bool,
    /// True when the editor contains a scan mutation that must be saved.
    pub changed: bool,
    /// Account IDs registered by the bootstrap scan.
    pub added_accounts: Vec<String>,
    /// Full `(id, account)` pairs for `added_accounts`, in the same order.
    /// Draft-merge callers (Settings scan) join these into the pending
    /// draft instead of saving immediately. Credentials are references
    /// (`$VAR`), 1Password refs, or profile directories — discovery
    /// never reads secret values.
    pub added: Vec<(String, crate::AccountConfig)>,
    /// Discovery issues observed during the scan.
    pub issues: Vec<crate::DiscoveryIssue>,
    /// `.zshrc` model profiles that could not be attached to a persisted
    /// API-key account. These are model/endpoint literals only; no secret
    /// values are carried here.
    pub unapplied_zshrc_models: Vec<crate::ModelProfile>,
    /// `.zshrc` wrapper call sites that have no launch executor yet. The
    /// parser's wrapper identity/arguments are retained so callers can report
    /// the exact unimplemented input instead of dropping it.
    pub unapplied_zshrc_wrappers: Vec<crate::WrapperCallSite>,
    /// Complete Amp XDG triples for which no credential evidence was found.
    /// A discovered triple is persisted on the profile account instead.
    pub unapplied_zshrc_xdg_roots: Vec<crate::XdgRoots>,
}

/// Build a profile candidate with an explicit identity and optional Amp XDG
/// roots. Callers must resolve a provider before creating the account; this
/// keeps a multi-provider store entry source-bound instead of inferring from
/// the agent alone.
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

/// Synthesize the registry entry for a discovered default profile.
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

/// Map the shell variable stem used by [`crate::ModelProfile`] to a provider.
/// Only canonical provider slugs from the provider catalog are accepted.
fn zshrc_provider(stem: &str) -> Option<crate::AiProvider> {
    crate::AiProvider::ALL
        .iter()
        .copied()
        .find(|provider| provider.slug() == stem)
}

/// Synthesize the registry entry for an environment-provided API key.
/// The credential is a `$VAR` reference — the value is never read.
fn env_scan_candidate(
    provider: crate::AiProvider,
    variable: &str,
    base_url: Option<String>,
) -> (String, crate::AccountConfig) {
    api_key_scan_candidate(provider, EnvValue::from(format!("${variable}")), base_url)
}

/// Synthesize the registry entry for a provider API key with an explicit
/// credential value (environment reference or 1Password ref).
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

/// Synthesize the registry entry for an environment-provided subscription
/// token. `None` if the agent ever loses its native provider (today only
/// Claude is discovered, which always has one).
fn oauth_scan_candidate(
    agent: jackin_core::Agent,
    variable: &str,
) -> Option<(String, crate::AccountConfig)> {
    oauth_scan_candidate_with_value(agent, EnvValue::from(format!("${variable}")))
}

/// Synthesize the registry entry for a subscription token with an explicit
/// credential value (environment reference or 1Password ref).
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

/// Scan default evidence + environment into `config`, registering only
/// IDs that do not collide with existing accounts. Never overwrites an
/// operator-registered account.
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

/// Whether `candidate`'s credential source is already registered under any
/// ID. Uses the same source fingerprint as `upsert_account` so scans skip
/// instead of erroring when the operator renamed an account ID or used a path
/// alias.
fn scan_source_registered(
    accounts: &BTreeMap<String, crate::AccountConfig>,
    candidate: &crate::AccountConfig,
) -> bool {
    let candidate_fingerprint = account_source_fingerprint(candidate);
    accounts
        .values()
        .any(|registered| account_source_fingerprint(registered) == candidate_fingerprint)
}

/// Apply the same ID, tombstone, and credential-source collision policy to
/// every discovery helper.
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

/// Read an installer `fresh_install` marker without changing it.
///
/// The marker is cleared only in the same successful write that commits the
/// bootstrap result, so a failed bootstrap remains retryable.
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

/// Comment-preserving mutator for `config.toml` and split workspace files.
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
    /// Loads the existing config file as a `DocumentMut`. Performs both
    /// schema-version and split-workspace migration before reading, so the
    /// on-disk result matches what `AppConfig::load_or_init` would produce.
    /// Fresh installs are bootstrapped directly while this editor already owns
    /// the write lock, avoiding recursive editor acquisition.
    pub fn open(paths: &JackinPaths) -> crate::ConfigResult<Self> {
        Self::open_detailed(paths).map(|(editor, _)| editor)
    }

    /// [`open`](Self::open) plus the bootstrap report for callers that
    /// surface first-run discovery results (CLI report, Settings scan).
    pub fn open_detailed(paths: &JackinPaths) -> crate::ConfigResult<(Self, BootstrapReport)> {
        let lock = acquire_config_write_lock(&paths.config_file)?;
        Self::open_with_lock(paths, lock)
    }

    pub(crate) fn open_with_lock(
        paths: &JackinPaths,
        lock: ConfigWriteGuard,
    ) -> crate::ConfigResult<(Self, BootstrapReport)> {
        paths.ensure_base_dirs()?;
        // The passed-in lock's acquisition already forward-rolled any
        // pending publication.
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
            // Installer-created config: run the first account scan exactly
            // once, then clear the marker. The file is installer-shaped
            // (no operator comments to preserve), so a typed round-trip
            // write is safe. Workspaces live in split files, never embedded,
            // so they are excluded from the global rewrite; the write
            // commits through pending_writes together with any migration
            // rewrites.
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
            // Registry, binding, and reserved-env semantics must pass before
            // migration bytes are committed. Workspace geometry remains
            // editable through create/edit, and launch-instance references
            // are enforced at save/load instead, so the editor can open a
            // config whose instance account is not registered yet for repair.
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

    /// Scan default evidence + the process environment for importable
    /// accounts, registering only IDs and credential sources not already
    /// present. Same id-synthesis/dedup rules as the first-run bootstrap
    /// (skip on ID collision; Omp/Hermes have no native billing and are
    /// skipped), plus a credential-source check so a re-scan skips instead
    /// of erroring when the operator renamed an account ID. Never
    /// overwrites an operator-registered account.
    ///
    /// Runs under this editor's config lock, so concurrent scans serialize
    /// and the loser dedupes to a no-op. Performs blocking filesystem /
    /// Keychain I/O — console callers must run it on a worker thread.
    ///
    /// # Errors
    /// Returns an error if a synthesized account fails validation.
    pub fn scan_for_accounts(&mut self) -> crate::ConfigResult<BootstrapReport> {
        let home = self.home_dir.clone();
        let environment = std::env::vars_os()
            .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
            .collect();
        self.scan_for_accounts_with(&home, &environment)
    }

    /// [`scan_for_accounts`](Self::scan_for_accounts) with explicit
    /// discovery inputs (deterministic seam for tests; production passes
    /// the live home directory and process environment).
    fn scan_for_accounts_with(
        &mut self,
        home: &Path,
        environment: &BTreeMap<String, String>,
    ) -> crate::ConfigResult<BootstrapReport> {
        let mut report = BootstrapReport::default();
        let scan = crate::discover_default_accounts(home);
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
            // Skip-on-collision in both dimensions: an operator
            // registration (same ID, or same credential source under
            // another ID) always wins over scan synthesis.
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

    /// Apply a `.zshrc` import plan, seeding verified profile accounts for
    /// config-dir/XDG overrides plus `op read` references as key/token values.
    /// Same skip-on-collision rules as
    /// [`scan_for_accounts`](Self::scan_for_accounts): never overwrites an
    /// operator registration, and override directories without credential
    /// evidence seed nothing. Model/endpoint groups update matching API-key
    /// accounts when present; wrapper call sites and otherwise-unapplied
    /// model/XDG entries are returned in the report so no parsed field
    /// disappears silently.
    ///
    /// # Errors
    /// Returns an error if a seeded account fails validation.
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
        // Arbitrary shell wrappers cannot safely be executed or serialized
        // into the current launch protocol. Retain the parsed call sites in
        // the report so callers surface them instead of dropping them.
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
                    // Shell overrides are distinct profiles from the
                    // default-home discovery entry. Reusing
                    // `default-{agent}` made a valid custom profile vanish
                    // after the default profile had already been scanned.
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
            // The generic XDG triple is currently an Amp profile contract.
            // OpenCode's data root has provider-keyed auth and may also
            // contain a database; without an explicit source directory,
            // importing it as Amp would persist an unrelated identity.
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
            // Presence-only probe: the synthetic value is never read, only
            // its non-emptiness gates provider attribution.
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
            // Variables with no provider home stay in the plan for an
            // explicit `account add`.
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

    /// Set an env key at `scope` (plain string, op ref, or extended value).
    pub fn set_env_var(
        &mut self,
        scope: &EnvScope,
        key: &str,
        value: EnvValue,
    ) -> crate::ConfigResult<()> {
        use jackin_core::EnvValue;
        use toml_edit::{InlineTable, Item, Value, value as toml_value};

        if jackin_core::is_account_env(key) {
            return Err(ConfigError::msg(format!(
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

    /// Set or clear the TOML key prefix comment for an env entry (no-op if missing).
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

    /// Write or clear `[roles.<agent_key>].trusted`.
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

    /// Write `[github].auth_forward = <mode>` at the global layer.
    pub fn set_global_github_auth_forward(&mut self, mode: GithubAuthMode) {
        let table = table_path_mut(&mut self.doc, &["github".to_owned()]);
        table.insert("auth_forward", toml_edit::value(github_mode_str(mode)));
    }

    /// Set a key under global `[github.env]`.
    pub fn set_global_github_env_var(
        &mut self,
        key: &str,
        value: EnvValue,
    ) -> crate::ConfigResult<()> {
        self.set_env_var(&EnvScope::GlobalGithub, key, value)
    }

    /// Remove a key from global `[github.env]`; returns whether it existed.
    pub fn remove_global_github_env_var(&mut self, key: &str) -> bool {
        self.remove_env_var(&EnvScope::GlobalGithub, key)
    }

    /// Enable or clear `[git].coauthor_trailer`.
    pub fn set_git_coauthor_trailer(&mut self, enabled: bool) {
        self.set_git_bool_field("coauthor_trailer", enabled);
    }

    /// Enable or clear `[git].dco`.
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

    /// Write or clear `[github].auth_forward` inside the workspace file.
    ///
    /// Mirrors [`Self::set_workspace_auth_forward`] but threads the
    /// GitHub kind's `[github]` block instead of an `Agent`-keyed
    /// child block. `mode = None` removes the `auth_forward` field
    /// (and the `github` block if it becomes empty), letting the
    /// resolver fall back to the next layer.
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

    /// Write or clear `[roles.<role>.github].auth_forward` inside the workspace file.
    ///
    /// Persists GitHub policy in the role layer; one kind
    /// dimension wider — `github` lives at the same three layers as
    /// `claude` / `codex`, but with no per-agent split.
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

    /// Ensure a built-in role has the expected `git` URL and `trusted = true`.
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

    /// Remove an env key at `scope`; prunes empty parent tables. Returns whether removed.
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

    /// Persist sticky `last_role` for a workspace (role key used last).
    pub fn set_last_agent(&mut self, workspace: &WorkspaceName, agent_key: &str) {
        let doc = self.workspace_doc_mut(workspace.as_str());
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

    /// Mark a workspace for deletion on the next [`Self::save`].
    pub fn remove_workspace(&mut self, name: &WorkspaceName) -> crate::ConfigResult<()> {
        if self.workspace_docs.remove(name.as_str()).is_none() {
            return Err(ConfigError::WorkspaceNotFound(name.as_str().to_owned()));
        }
        self.removed_workspaces.insert(name.as_str().to_owned());
        Ok(())
    }

    /// Validate and stage a new workspace document for the next [`Self::save`].
    pub fn create_workspace(
        &mut self,
        name: &WorkspaceName,
        ws: WorkspaceConfig,
    ) -> crate::ConfigResult<()> {
        // Delegate to AppConfig::create_workspace's validated logic
        // (collision check, workdir / mount-destination relationship,
        // plan-collapse sanity) so the editor path behaves identically
        // to the direct-mutation path. Mirrors edit_workspace's pattern.
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

    /// Apply a [`WorkspaceEdit`] via in-memory validation, then replace the workspace doc.
    pub fn edit_workspace(
        &mut self,
        name: &WorkspaceName,
        edit: WorkspaceEdit,
    ) -> crate::ConfigResult<()> {
        // Snapshot current on-disk state into an AppConfig.
        let mut in_memory = validate_candidate(&self.doc.to_string(), &self.workspace_docs)
            .context("re-parsing current docs into AppConfig for workspace edit")?;

        // Apply the edit using the existing validated logic. Mutates
        // in_memory or returns Err with the validation message on failure.
        in_memory.edit_workspace(name, edit)?;

        // Pull the resulting WorkspaceConfig back out and splat into the doc.
        let updated = in_memory.workspaces.get(name.as_str()).ok_or_else(|| {
            anyhow::Error::from(ConfigError::WorkspaceDisappearedAfterEdit(
                name.as_str().to_owned(),
            ))
        })?;

        // Replace the entire workspace document. This preserves
        // comments in OTHER workspaces and in unrelated top-level sections,
        // which is what the migration cares about. Comments inside the
        // edited workspace itself are consumed — that's acceptable because
        // the edit IS the change the user is making to that workspace.
        let rendered = toml::to_string(updated)?;
        let parsed: DocumentMut = rendered.parse()?;
        self.workspace_docs.insert(name.as_str().to_owned(), parsed);

        Ok(())
    }

    /// Test-only: insert a string value at a dotted table path in the main doc.
    ///
    /// Used by tests that need to inject invalid TOML shapes (e.g. a role env
    /// block without the required `git` field) to exercise save-time rejection.
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
