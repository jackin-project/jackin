// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `AppConfig` load/init behavior: TOML read, workspace-split migration,
//! reserved-env validation, and builtin-agent sync.

use crate::ConfigError;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use jackin_core::{JackinPaths, WorkspaceName};
use sha2::{Digest as _, Sha256};
use toml_edit::DocumentMut;

use super::AppConfig;
use crate::editor::ConfigEditor;
use crate::migrations;
use crate::persist::{atomic_write, validate_workspace_file_stem};
use crate::schema::WorkspaceConfig;
use crate::validation::validate_workspace_config;
use crate::versions::{CURRENT_CONFIG_VERSION, CURRENT_WORKSPACE_VERSION};

const READ_ONLY_SNAPSHOT_ATTEMPTS: usize = 3;

/// Stable content generation for one admitted config tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigGeneration(String);

impl ConfigGeneration {
    /// Lowercase SHA-256 digest of the sorted config-relative path and byte sequence.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Sanitized source scope for a read-only config diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSourceScope {
    /// Top-level `config.toml`.
    Global,
    /// The split-workspace collection, without exposing a filesystem path.
    Workspaces,
    /// One validated workspace name.
    Workspace(String),
}

/// Machine-readable failure category for one config source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSourceIssue {
    /// Source bytes could not be read.
    Unreadable,
    /// TOML syntax or typed schema was malformed.
    Malformed,
    /// Source schema is newer than this binary supports.
    UnsupportedVersion,
    /// Source failed semantic validation.
    Invalid,
    /// Workspace filename was not a valid workspace name.
    InvalidWorkspaceName,
    /// Embedded and split definitions for one workspace disagreed.
    ConflictingWorkspaceDefinitions,
    /// The config tree changed repeatedly while it was being read.
    TransientConflict,
}

/// Sanitized diagnostic produced while building a read-only config snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSourceDiagnostic {
    /// Logical source that failed; never a filesystem path.
    pub scope: ConfigSourceScope,
    /// Stable failure category; never raw parser or credential text.
    pub issue: ConfigSourceIssue,
}

/// Valid portions of the operator config tree loaded without filesystem mutation.
#[derive(Debug, Clone)]
pub struct ReadOnlyConfigSnapshot {
    /// Parsed global config with every valid embedded/split workspace attached.
    pub config: AppConfig,
    /// Per-source failures; unrelated valid sources remain available.
    pub diagnostics: Vec<ConfigSourceDiagnostic>,
    /// Content-derived generation for every readable config source encountered.
    pub generation: ConfigGeneration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawConfigFile {
    relative_path: String,
    scope: ConfigSourceScope,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawConfigTree {
    files: Vec<RawConfigFile>,
    diagnostics: Vec<ConfigSourceDiagnostic>,
}

/// Load the complete config tree without creating, migrating, or rewriting files.
///
/// A shared advisory lock is used when a writer lock already exists. A full
/// content re-read still brackets parsing so first-writer races and external
/// editors cannot produce a torn multi-file snapshot.
pub fn load_read_only_config_snapshot(
    paths: &JackinPaths,
) -> crate::ConfigResult<ReadOnlyConfigSnapshot> {
    load_read_only_config_snapshot_with_hook(paths, |_| {})
}

fn load_read_only_config_snapshot_with_hook<F>(
    paths: &JackinPaths,
    mut between_reads: F,
) -> crate::ConfigResult<ReadOnlyConfigSnapshot>
where
    F: FnMut(usize),
{
    let _guard = crate::persist::acquire_config_read_lock(&paths.config_file)?;
    for attempt in 0..READ_ONLY_SNAPSHOT_ATTEMPTS {
        let before = read_raw_config_tree(paths);
        let snapshot = parse_raw_config_tree(&before);
        between_reads(attempt);
        let after = read_raw_config_tree(paths);
        if before == after {
            return Ok(snapshot);
        }
    }

    Ok(ReadOnlyConfigSnapshot {
        config: AppConfig::default(),
        diagnostics: vec![ConfigSourceDiagnostic {
            scope: ConfigSourceScope::Workspaces,
            issue: ConfigSourceIssue::TransientConflict,
        }],
        generation: config_generation(&[]),
    })
}

fn read_raw_config_tree(paths: &JackinPaths) -> RawConfigTree {
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    read_raw_file(
        &paths.config_file,
        "config.toml".to_owned(),
        ConfigSourceScope::Global,
        &mut files,
        &mut diagnostics,
    );

    let entries = match std::fs::read_dir(&paths.workspaces_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return RawConfigTree { files, diagnostics };
        }
        Err(_) => {
            diagnostics.push(ConfigSourceDiagnostic {
                scope: ConfigSourceScope::Workspaces,
                issue: ConfigSourceIssue::Unreadable,
            });
            return RawConfigTree { files, diagnostics };
        }
    };

    let mut workspace_paths = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            diagnostics.push(ConfigSourceDiagnostic {
                scope: ConfigSourceScope::Workspaces,
                issue: ConfigSourceIssue::Unreadable,
            });
            continue;
        };
        let path = entry.path();
        if path.extension() == Some(OsStr::new("toml")) {
            workspace_paths.push(path);
        }
    }
    workspace_paths.sort();

    for path in workspace_paths {
        let Some(stem) = path.file_stem().and_then(OsStr::to_str) else {
            diagnostics.push(ConfigSourceDiagnostic {
                scope: ConfigSourceScope::Workspaces,
                issue: ConfigSourceIssue::InvalidWorkspaceName,
            });
            continue;
        };
        let Ok(name) = WorkspaceName::parse(stem) else {
            diagnostics.push(ConfigSourceDiagnostic {
                scope: ConfigSourceScope::Workspaces,
                issue: ConfigSourceIssue::InvalidWorkspaceName,
            });
            continue;
        };
        let name = name.into_inner();
        read_raw_file(
            &path,
            format!("workspaces/{name}.toml"),
            ConfigSourceScope::Workspace(name),
            &mut files,
            &mut diagnostics,
        );
    }

    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    diagnostics.sort_by(|left, right| diagnostic_sort_key(left).cmp(&diagnostic_sort_key(right)));
    RawConfigTree { files, diagnostics }
}

fn read_raw_file(
    path: &Path,
    relative_path: String,
    scope: ConfigSourceScope,
    files: &mut Vec<RawConfigFile>,
    diagnostics: &mut Vec<ConfigSourceDiagnostic>,
) {
    match std::fs::read(path) {
        Ok(bytes) => files.push(RawConfigFile {
            relative_path,
            scope,
            bytes,
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => diagnostics.push(ConfigSourceDiagnostic {
            scope,
            issue: ConfigSourceIssue::Unreadable,
        }),
    }
}

fn diagnostic_sort_key(diagnostic: &ConfigSourceDiagnostic) -> (u8, &str, u8) {
    let (scope, name) = match &diagnostic.scope {
        ConfigSourceScope::Global => (0, ""),
        ConfigSourceScope::Workspaces => (1, ""),
        ConfigSourceScope::Workspace(name) => (2, name.as_str()),
    };
    (scope, name, diagnostic.issue as u8)
}

fn parse_raw_config_tree(tree: &RawConfigTree) -> ReadOnlyConfigSnapshot {
    let mut diagnostics = tree.diagnostics.clone();
    let mut config = AppConfig::default();
    let mut embedded = BTreeMap::new();

    if let Some(global) = tree
        .files
        .iter()
        .find(|file| file.scope == ConfigSourceScope::Global)
    {
        match parse_global_config(&global.bytes) {
            Ok((parsed, parsed_embedded)) => {
                config = parsed;
                embedded = parsed_embedded;
            }
            Err(issue) => diagnostics.push(ConfigSourceDiagnostic {
                scope: ConfigSourceScope::Global,
                issue,
            }),
        }
    }

    let mut split = BTreeMap::new();
    for file in tree
        .files
        .iter()
        .filter(|file| matches!(file.scope, ConfigSourceScope::Workspace(_)))
    {
        let ConfigSourceScope::Workspace(name) = &file.scope else {
            continue;
        };
        match parse_workspace_config(name, &file.bytes) {
            Ok(workspace) => {
                split.insert(name.clone(), workspace);
            }
            Err(issue) => diagnostics.push(ConfigSourceDiagnostic {
                scope: file.scope.clone(),
                issue,
            }),
        }
    }

    for (name, workspace) in embedded {
        match split.get(&name) {
            Some(split_workspace) if split_workspace == &workspace => {}
            Some(_) => diagnostics.push(ConfigSourceDiagnostic {
                scope: ConfigSourceScope::Workspace(name),
                issue: ConfigSourceIssue::ConflictingWorkspaceDefinitions,
            }),
            None => {
                split.insert(name, workspace);
            }
        }
    }
    config.workspaces = split;
    if config.validate_accounts().is_err() {
        diagnostics.push(ConfigSourceDiagnostic {
            scope: ConfigSourceScope::Workspaces,
            issue: ConfigSourceIssue::Invalid,
        });
    }
    diagnostics.sort_by(|left, right| diagnostic_sort_key(left).cmp(&diagnostic_sort_key(right)));

    ReadOnlyConfigSnapshot {
        config,
        diagnostics,
        generation: config_generation(&tree.files),
    }
}

fn parse_global_config(
    bytes: &[u8],
) -> Result<(AppConfig, BTreeMap<String, WorkspaceConfig>), ConfigSourceIssue> {
    let raw = std::str::from_utf8(bytes).map_err(|_| ConfigSourceIssue::Malformed)?;
    let legacy_op_accounts =
        legacy_workspace_op_accounts(raw).map_err(|_| ConfigSourceIssue::Malformed)?;
    let mut doc = migrate_document_in_memory(
        raw,
        "config",
        CURRENT_CONFIG_VERSION,
        migrations::CONFIG_MIGRATIONS,
    )?;
    migrate_embedded_op_accounts(&mut doc).map_err(|_| ConfigSourceIssue::Malformed)?;
    let mut config: AppConfig =
        toml::from_str(&doc.to_string()).map_err(|_| ConfigSourceIssue::Malformed)?;
    let raw_embedded = std::mem::take(&mut config.workspaces);
    let mut embedded = BTreeMap::new();
    for (name, workspace) in raw_embedded {
        let workspace = migrate_legacy_workspace_value(
            &name,
            &workspace,
            legacy_op_accounts.get(&name).map(String::as_str),
        )
        .map_err(|_| ConfigSourceIssue::Malformed)?;
        validate_one_workspace(&name, &workspace)?;
        embedded.insert(name, workspace);
    }
    validate_reserved_env_names(&config).map_err(|_| ConfigSourceIssue::Invalid)?;
    config
        .validate_accounts()
        .map_err(|_| ConfigSourceIssue::Invalid)?;
    config.version = CURRENT_CONFIG_VERSION.to_owned();
    Ok((config, embedded))
}

fn parse_workspace_config(name: &str, bytes: &[u8]) -> Result<WorkspaceConfig, ConfigSourceIssue> {
    let raw = std::str::from_utf8(bytes).map_err(|_| ConfigSourceIssue::Malformed)?;
    let doc = migrate_document_in_memory(
        raw,
        "workspace config",
        CURRENT_WORKSPACE_VERSION,
        migrations::WORKSPACE_MIGRATIONS,
    )?;
    let workspace: WorkspaceConfig =
        toml::from_str(&doc.to_string()).map_err(|_| ConfigSourceIssue::Malformed)?;
    validate_one_workspace(name, &workspace)?;
    Ok(workspace)
}

fn validate_one_workspace(
    name: &str,
    workspace: &WorkspaceConfig,
) -> Result<(), ConfigSourceIssue> {
    let name = WorkspaceName::parse(name).map_err(|_| ConfigSourceIssue::InvalidWorkspaceName)?;
    validate_workspace_config(&name, workspace).map_err(|_| ConfigSourceIssue::Invalid)?;
    let mut isolated = AppConfig::default();
    isolated
        .workspaces
        .insert(name.into_inner(), workspace.clone());
    validate_reserved_env_names(&isolated).map_err(|_| ConfigSourceIssue::Invalid)
}

fn migrate_document_in_memory(
    raw: &str,
    label: &str,
    current_raw: &str,
    registry: &[migrations::MigrationStep],
) -> Result<DocumentMut, ConfigSourceIssue> {
    let mut doc: DocumentMut = raw.parse().map_err(|_| ConfigSourceIssue::Malformed)?;
    let old = migrations::doc_version(&doc, label).map_err(|_| ConfigSourceIssue::Malformed)?;
    let current =
        migrations::parse_version(current_raw).map_err(|_| ConfigSourceIssue::Malformed)?;
    if old > current {
        return Err(ConfigSourceIssue::UnsupportedVersion);
    }
    if old < current {
        migrations::apply_migrations(&mut doc, &old, &current, registry, label)
            .map_err(|_| ConfigSourceIssue::Malformed)?;
    }
    Ok(doc)
}

fn config_generation(files: &[RawConfigFile]) -> ConfigGeneration {
    let mut hasher = Sha256::new();
    for file in files {
        hash_len_prefixed(&mut hasher, file.relative_path.as_bytes());
        hash_len_prefixed(&mut hasher, &file.bytes);
    }
    ConfigGeneration(hex::encode(hasher.finalize()))
}

fn hash_len_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(bytes);
}

pub(crate) fn workspace_file_path(paths: &JackinPaths, name: &str) -> PathBuf {
    paths.workspaces_dir.join(format!("{name}.toml"))
}

/// Load global config plus split workspace files, migrating legacy embedded workspaces.
pub fn load_split_config(
    paths: &JackinPaths,
    contents_opt: Option<String>,
) -> crate::ConfigResult<AppConfig> {
    // Capture legacy per-workspace `op_account` from the raw TOML before
    // the typed parse below drops it: `WorkspaceConfig` no longer has that
    // field (it moved onto each op ref in v1alpha5), so a typed round-trip
    // would silently lose it for operators still on an embedded
    // `[workspaces.*]` config. See `migrate_legacy_workspaces`.
    let legacy_op_accounts = match contents_opt.as_deref() {
        Some(c) => legacy_workspace_op_accounts(c)?,
        None => BTreeMap::new(),
    };

    let mut config: AppConfig = match contents_opt {
        Some(c) => {
            let mut doc: DocumentMut = c
                .parse()
                .context("parsing embedded workspace configuration")?;
            migrate_embedded_op_accounts(&mut doc)?;
            toml::from_str(&doc.to_string())?
        }
        None => AppConfig::default(),
    };

    let legacy_workspaces = std::mem::take(&mut config.workspaces);
    if !legacy_workspaces.is_empty() {
        migrate_legacy_workspaces(paths, &config, &legacy_workspaces, &legacy_op_accounts)?;
    }

    config.workspaces = load_workspace_files(&paths.workspaces_dir)?;
    Ok(config)
}

/// Upgrade embedded legacy fields before strict `WorkspaceConfig` deserialization.
fn migrate_embedded_op_accounts(doc: &mut DocumentMut) -> crate::ConfigResult<()> {
    let Some(workspaces) = doc
        .get_mut("workspaces")
        .and_then(toml_edit::Item::as_table_mut)
    else {
        return Ok(());
    };
    for (_, item) in workspaces.iter_mut() {
        let Some(table) = item.as_table_mut() else {
            continue;
        };
        if !table.contains_key("op_account") {
            continue;
        }
        let mut workspace = DocumentMut::new();
        *workspace.as_table_mut() = table.clone();
        migrations::migrate_workspace_op_account_to_refs(&mut workspace)?;
        *table = workspace.as_table().clone();
    }
    Ok(())
}

/// Read and migrate every `*.toml` workspace file under `workspaces_dir`.
pub fn load_workspace_files(
    workspaces_dir: &Path,
) -> crate::ConfigResult<BTreeMap<String, WorkspaceConfig>> {
    let mut workspaces = BTreeMap::new();
    let entries = match std::fs::read_dir(workspaces_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(workspaces),
        Err(e) => {
            return Err(anyhow::Error::new(e)
                .context(format!(
                    "reading workspaces directory {}",
                    workspaces_dir.display()
                ))
                .into());
        }
    };

    for entry in entries {
        let entry = entry.with_context(|| {
            format!("scanning workspaces directory {}", workspaces_dir.display())
        })?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
            anyhow::Error::from(ConfigError::msg(format!(
                "invalid workspace filename {}",
                path.display()
            )))
        })?;
        let name = WorkspaceName::parse(stem)
            .with_context(|| format!("invalid workspace filename {}", path.display()))?;
        migrations::migrate_workspace_file_if_needed(&path)?;
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading workspace config {}", path.display()))?;
        let workspace = toml::from_str(&raw)
            .with_context(|| format!("parsing workspace config {}", path.display()))?;
        workspaces.insert(name.into_inner(), workspace);
    }
    Ok(workspaces)
}

/// Extract `[workspaces.<name>].op_account` string values from a raw
/// legacy `config.toml`. Absent `op_account` is skipped (the caller treats
/// a missing entry as "no account to preserve"), but a present-but-
/// non-string value bails loudly — it is operator data the v1alpha5
/// migration (`migrate_workspace_op_account_to_refs`) refuses to silently
/// drop, and this legacy-split path must honour the same contract. A TOML
/// parse error is not handled here: the same `contents` is parsed with `?`
/// upstream in the `load_or_init` flow before this runs.
fn legacy_workspace_op_accounts(contents: &str) -> anyhow::Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let Ok(doc) = contents.parse::<DocumentMut>() else {
        return Ok(out);
    };
    let Some(workspaces) = doc.get("workspaces").and_then(|w| w.as_table()) else {
        return Ok(out);
    };
    for (name, ws) in workspaces {
        let Some(item) = ws.get("op_account") else {
            continue;
        };
        match item.as_str() {
            Some(acct) => {
                out.insert(name.to_owned(), acct.to_owned());
            }
            None => {
                return Err(ConfigError::msg(format!(
                    "workspace {name:?}: `op_account` must be a string, found {item:?}"
                ))
                .into());
            }
        }
    }
    Ok(out)
}

fn migrate_legacy_workspaces(
    paths: &JackinPaths,
    global_config: &AppConfig,
    workspaces: &BTreeMap<String, WorkspaceConfig>,
    legacy_op_accounts: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    // Crash-recovery ordering: the global rewrite is the commit point. If
    // we crash before it, the legacy `[workspaces.*]` tables remain
    // authoritative and the next load_or_init re-runs this function. The
    // exists+equal short-circuit below keeps that re-entry idempotent.
    std::fs::create_dir_all(&paths.workspaces_dir).with_context(|| {
        format!(
            "creating workspaces directory {}",
            paths.workspaces_dir.display()
        )
    })?;
    for (name, workspace) in workspaces {
        validate_workspace_file_stem(name)?;
        let path = workspace_file_path(paths, name);
        let contents = legacy_workspace_contents(
            name,
            workspace,
            legacy_op_accounts.get(name).map(String::as_str),
        )?;
        if path.exists() {
            // Idempotent re-entry: compare against the bytes we would write
            // (account already stamped), not the legacy struct — otherwise a
            // crash-recovery re-run would see the stamped on-disk file differ
            // from the unstamped legacy struct and bail. Both sides are
            // parsed to ignore formatting drift.
            let existing_raw = std::fs::read_to_string(&path)
                .with_context(|| format!("reading existing workspace {}", path.display()))?;
            let existing: WorkspaceConfig = toml::from_str(&existing_raw)
                .with_context(|| format!("parsing existing workspace {}", path.display()))?;
            let desired: WorkspaceConfig = toml::from_str(&contents)
                .with_context(|| format!("parsing migrated workspace {name:?}"))?;
            if existing == desired {
                continue;
            }
            return Err(ConfigError::msg(format!(
                "cannot migrate workspace {name:?}: {} already exists with different contents \
                 than the legacy config.toml. Reconcile the two copies (delete the split file to \
                 take the legacy version, or remove [workspaces.{name}] from config.toml to take \
                 the split file) and re-run.",
                path.display()
            ))
            .into());
        }
        atomic_write(&path, &contents)?;
    }

    // Lossy: serde round-trip drops comments and blank lines from
    // `config.toml`. Acceptable here because this path runs once at legacy
    // migration; steady-state edits go through `ConfigEditor`.
    let global_contents = toml::to_string_pretty(global_config).with_context(|| {
        format!(
            "serializing migrated global config for {}",
            paths.config_file.display()
        )
    })?;
    atomic_write(&paths.config_file, &global_contents)?;
    Ok(())
}

fn legacy_workspace_contents(
    name: &str,
    workspace: &WorkspaceConfig,
    legacy_op_account: Option<&str>,
) -> anyhow::Result<String> {
    let contents = toml::to_string_pretty(workspace)
        .with_context(|| format!("serializing workspace {name:?}"))?;
    let Some(account) = legacy_op_account else {
        return Ok(contents);
    };
    let mut doc: DocumentMut = contents
        .parse()
        .with_context(|| format!("re-parsing serialized workspace {name:?}"))?;
    doc.insert("op_account", toml_edit::value(account));
    migrations::migrate_workspace_op_account_to_refs(&mut doc)
        .with_context(|| format!("stamping legacy op_account onto refs for workspace {name:?}"))?;
    Ok(doc.to_string())
}

fn migrate_legacy_workspace_value(
    name: &str,
    workspace: &WorkspaceConfig,
    legacy_op_account: Option<&str>,
) -> anyhow::Result<WorkspaceConfig> {
    let raw = legacy_workspace_contents(name, workspace, legacy_op_account)?;
    toml::from_str(&raw).with_context(|| format!("parsing migrated workspace {name:?}"))
}

/// Reject operator env maps that declare any reserved runtime name.
pub fn validate_reserved_env_names(config: &AppConfig) -> crate::ConfigResult<()> {
    let mut offenses: Vec<String> = Vec::new();
    let mut check = |layer: &str, env: &BTreeMap<String, jackin_core::EnvValue>| {
        for key in env.keys() {
            if jackin_core::is_reserved(key) {
                offenses.push(format!(
                    "  - {key:?} is reserved by the jackin runtime; declared in {layer}"
                ));
            }
        }
    };

    check("global env", &config.env);
    for (role_name, role_source) in &config.roles {
        check(&format!("role \"{role_name}\" env"), &role_source.env);
    }
    for (ws_name, ws) in &config.workspaces {
        check(&format!("workspace \"{ws_name}\" env"), &ws.env);
        for (role_name, override_) in &ws.roles {
            check(
                &format!("workspace \"{ws_name}\" role \"{role_name}\" env"),
                &override_.env,
            );
        }
    }

    if offenses.is_empty() {
        return Ok(());
    }
    Err(ConfigError::msg(format!(
        "config contains reserved jackin runtime env vars:\n{}",
        offenses.join("\n")
    )))
}

/// `true` when `raw` is legacy-versioned and still embeds non-empty `[workspaces]`.
pub fn config_needs_split_migration(raw: &str) -> crate::ConfigResult<bool> {
    let doc: DocumentMut = raw.parse().context("parsing config.toml")?;
    let version = migrations::doc_version(&doc, "config")?;
    let has_legacy_workspaces = doc
        .get("workspaces")
        .and_then(toml_edit::Item::as_table)
        .is_some_and(|workspaces| !workspaces.is_empty());
    Ok(version == migrations::SchemaVersion::Legacy && has_legacy_workspaces)
}

fn load_config_contents(paths: &JackinPaths) -> crate::ConfigResult<Option<String>> {
    match std::fs::read_to_string(&paths.config_file) {
        Ok(raw) if config_needs_split_migration(&raw)? => Ok(Some(raw)),
        Ok(_) => {
            migrations::migrate_config_file_if_needed(&paths.config_file)?;
            std::fs::read_to_string(&paths.config_file)
                .with_context(|| {
                    format!("re-reading {} after migration", paths.config_file.display())
                })
                .map(Some)
                .map_err(Into::into)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(anyhow::Error::new(error)
            .context(format!("reading {}", paths.config_file.display()))
            .into()),
    }
}

impl AppConfig {
    /// Load `config.toml` (migrate as needed), split workspaces, sync builtins, validate.
    pub fn load_or_init(paths: &JackinPaths) -> crate::ConfigResult<Self> {
        let loaded = (|| {
            paths.ensure_base_dirs()?;
            let contents_opt = load_config_contents(paths)?;
            load_split_config(paths, contents_opt)
        })();
        let mut config = crate::telemetry::finish_operation(
            jackin_telemetry::schema::enums::ConfigScope::Global,
            jackin_telemetry::schema::enums::ConfigOperation::Load,
            loaded,
        )?;

        // Pre-sync validation: gives the operator a reserved-name error
        // rather than save()'s "rejecting candidate config" wrapper.
        // ConfigEditor::save runs the same check via validate_candidate;
        // this call covers the path where save() is never invoked because
        // builtins did not drift.
        crate::telemetry::finish_operation(
            jackin_telemetry::schema::enums::ConfigScope::Global,
            jackin_telemetry::schema::enums::ConfigOperation::Validate,
            (|| {
                validate_reserved_env_names(&config)?;
                config.validate_accounts()
            })(),
        )?;

        let builtins_changed = config.sync_builtin_agents();

        if builtins_changed {
            let mut editor = ConfigEditor::open(paths)?;
            for &(name, git) in super::roles::BUILTIN_ROLES {
                editor.upsert_builtin_agent(name, git);
            }
            // Take save()'s post-write parse: it preserves [roles.X.env] that
            // sync_builtin_agents cleared in-memory.
            config = editor.save()?;
        }

        crate::telemetry::finish_operation(
            jackin_telemetry::schema::enums::ConfigScope::Workspace,
            jackin_telemetry::schema::enums::ConfigOperation::Validate,
            config.validate_workspaces(),
        )?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests;
