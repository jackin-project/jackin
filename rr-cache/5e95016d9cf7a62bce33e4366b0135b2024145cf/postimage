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
use crate::editor::{ConfigEditor, recover_pending_publication};
use crate::migrations;
use crate::persist::{
    acquire_config_write_lock, commit_staged_config, ensure_replaceable_target, stage_atomic_write,
    validate_workspace_file_stem,
};
use crate::schema::WorkspaceConfig;
use crate::validation::validate_workspace_config;
use crate::versions::{CURRENT_CONFIG_VERSION, CURRENT_WORKSPACE_VERSION};

const READ_ONLY_SNAPSHOT_ATTEMPTS: usize = 3;

struct PendingConfigWrite {
    path: PathBuf,
    contents: String,
}

pub(crate) struct LoadedConfig {
    config: AppConfig,
    pending_writes: Vec<PendingConfigWrite>,
}

impl LoadedConfig {
    pub(crate) fn add_pending_write(&mut self, path: PathBuf, contents: String) {
        if let Some(existing) = self
            .pending_writes
            .iter_mut()
            .find(|write| write.path == path)
        {
            existing.contents = contents;
        } else {
            self.pending_writes
                .push(PendingConfigWrite { path, contents });
        }
    }

    pub(crate) fn has_pending_writes(&self) -> bool {
        !self.pending_writes.is_empty()
    }

    pub(crate) fn config_mut(&mut self) -> &mut AppConfig {
        &mut self.config
    }

    pub(crate) fn validate(&self) -> crate::ConfigResult<()> {
        validate_config_semantics(&self.config)
    }

    pub(crate) fn validate_for_editor(&self) -> crate::ConfigResult<()> {
        validate_editor_config_semantics(&self.config)
    }

    pub(crate) fn commit(self) -> crate::ConfigResult<AppConfig> {
        let Self {
            config,
            pending_writes,
        } = self;

        commit_pending_config_writes(pending_writes)?;
        Ok(config)
    }
}

fn commit_pending_config_writes(
    pending_writes: Vec<PendingConfigWrite>,
) -> crate::ConfigResult<()> {
    for write in &pending_writes {
        ensure_replaceable_target(&write.path)?;
    }

    let mut staged = Vec::with_capacity(pending_writes.len());
    for write in pending_writes {
        staged.push(stage_atomic_write(&write.path, &write.contents)?);
    }
    let mut deletes = Vec::new();
    commit_staged_config(&mut staged, &mut deletes)
}

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
    migrate_embedded_workspaces(&mut doc)?;
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
    let (normalized, _, _) = normalize_workspace_contents(raw).map_err(|error| match error {
        WorkspaceNormalizationError::UnsupportedVersion(_) => ConfigSourceIssue::UnsupportedVersion,
        WorkspaceNormalizationError::Other(_) => ConfigSourceIssue::Malformed,
    })?;
    let workspace: WorkspaceConfig =
        toml::from_str(&normalized).map_err(|_| ConfigSourceIssue::Malformed)?;
    validate_one_workspace(name, &workspace)?;
    Ok(workspace)
}

#[derive(Debug)]
enum WorkspaceNormalizationError {
    UnsupportedVersion(ConfigError),
    Other(ConfigError),
}

impl WorkspaceNormalizationError {
    fn into_config_error(self) -> ConfigError {
        match self {
            Self::UnsupportedVersion(error) | Self::Other(error) => error,
        }
    }
}

impl From<ConfigError> for WorkspaceNormalizationError {
    fn from(error: ConfigError) -> Self {
        Self::Other(error)
    }
}

/// Normalize one workspace document before typed comparison or deserialization.
///
/// The split migration path can encounter a file written by an older binary
/// while an embedded legacy workspace is being split. Compare the migrated
/// semantic value, not the old version marker or fields that the current typed
/// schema no longer accepts. Legacy fields are transformed only by their
/// versioned migration step; mislabeled newer documents remain invalid. The
/// caller owns the eventual atomic write.
fn normalize_workspace_contents(
    raw: &str,
) -> Result<(String, Option<migrations::SchemaVersion>, bool), WorkspaceNormalizationError> {
    let mut doc: DocumentMut = raw.parse().map_err(|error| {
        WorkspaceNormalizationError::Other(ConfigError::Other(
            anyhow::Error::new(error).context("parsing workspace config"),
        ))
    })?;
    let old_version = migrations::doc_version(&doc, "workspace config")?;
    let current_version = migrations::parse_version(CURRENT_WORKSPACE_VERSION)?;
    if old_version > current_version {
        return Err(WorkspaceNormalizationError::UnsupportedVersion(
            ConfigError::msg(format!(
                "workspace config is at {old_version}, this binary only understands up to \
                 {CURRENT_WORKSPACE_VERSION}; upgrade jackin"
            )),
        ));
    }
    let migrated_from = migrations::migrate_document_if_needed(
        &mut doc,
        "workspace config",
        CURRENT_WORKSPACE_VERSION,
        migrations::WORKSPACE_MIGRATIONS,
    )?;
    let needs_write = migrated_from.is_some();
    Ok((doc.to_string(), migrated_from, needs_write))
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
    let _lock = acquire_config_write_lock(&paths.config_file)?;
    recover_pending_publication(&paths.config_file)?;
    let loaded = load_split_config_locked(paths, contents_opt)?;
    loaded.validate()?;
    loaded.commit()
}

pub(crate) fn load_split_config_locked(
    paths: &JackinPaths,
    contents_opt: Option<String>,
) -> crate::ConfigResult<LoadedConfig> {
    // Capture legacy per-workspace `op_account` from the raw TOML before
    // the typed parse below drops it: `WorkspaceConfig` no longer has that
    // field (it moved onto each op ref in v1alpha5), so a typed round-trip
    // would silently lose it for operators still on an embedded
    // `[workspaces.*]` config. See `plan_legacy_workspace_writes`.
    let legacy_op_accounts = match contents_opt.as_deref() {
        Some(c) => legacy_workspace_op_accounts(c)?,
        None => BTreeMap::new(),
    };

    let mut migrated_global_contents = None;
    let mut config: AppConfig = match contents_opt {
        Some(c) => {
            let mut doc: DocumentMut = c
                .parse()
                .context("parsing embedded workspace configuration")?;
            let migrated_from = migrations::migrate_document_if_needed(
                &mut doc,
                "config",
                CURRENT_CONFIG_VERSION,
                migrations::CONFIG_MIGRATIONS,
            );
            migrations::emit_migration_result(
                "global",
                CURRENT_CONFIG_VERSION,
                migrations::CONFIG_MIGRATIONS,
                &migrated_from,
            );
            let migrated = migrated_from?.is_some();
            migrate_embedded_op_accounts(&mut doc)?;
            migrate_embedded_workspaces(&mut doc).map_err(|issue| {
                ConfigError::msg(format!(
                    "migrating embedded workspace configuration: {issue:?}"
                ))
            })?;
            let serialized = doc.to_string();
            if migrated {
                migrated_global_contents = Some(serialized.clone());
            }
            toml::from_str(&serialized)?
        }
        None => AppConfig::default(),
    };

    let legacy_workspaces = std::mem::take(&mut config.workspaces);
    let (mut split_workspaces, split_writes) = load_workspace_files_locked(&paths.workspaces_dir)?;
    let mut pending_writes = split_writes;
    let mut global_write = None;
    if !legacy_workspaces.is_empty() {
        // Lossy: serde round-trip drops comments and blank lines from
        // `config.toml`. Acceptable here because this path runs once at
        // legacy migration; steady-state edits go through `ConfigEditor`.
        let global_contents = toml::to_string_pretty(&config).with_context(|| {
            format!(
                "serializing migrated global config for {}",
                paths.config_file.display()
            )
        })?;
        pending_writes.extend(plan_legacy_workspace_writes(
            paths,
            &legacy_workspaces,
            &legacy_op_accounts,
            &split_workspaces,
        )?);
        global_write = Some(PendingConfigWrite {
            path: paths.config_file.clone(),
            contents: global_contents,
        });
    } else if let Some(contents) = migrated_global_contents {
        global_write = Some(PendingConfigWrite {
            path: paths.config_file.clone(),
            contents,
        });
    }

    if let Some(global_write) = global_write {
        // Keep the global rewrite last: it remains the migration commit
        // marker, while the transaction restores earlier split files if a
        // later rename or directory sync fails.
        pending_writes.push(global_write);
    }
    for (name, workspace) in legacy_workspaces {
        split_workspaces.entry(name).or_insert(workspace);
    }
    config.workspaces = split_workspaces;
    Ok(LoadedConfig {
        config,
        pending_writes,
    })
}

/// Run the complete workspace migration chain before strict deserialization.
fn migrate_embedded_workspaces(doc: &mut DocumentMut) -> Result<(), ConfigSourceIssue> {
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
        let mut workspace = DocumentMut::new();
        *workspace.as_table_mut() = table.clone();
        let workspace = migrate_document_in_memory(
            &workspace.to_string(),
            "workspace config",
            CURRENT_WORKSPACE_VERSION,
            migrations::WORKSPACE_MIGRATIONS,
        )?;
        *table = workspace.as_table().clone();
    }
    Ok(())
}

/// Upgrade embedded legacy `op_account` fields before strict deserialization.
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
    let config_file = workspaces_dir
        .parent()
        .unwrap_or(workspaces_dir)
        .join("config.toml");
    let _lock = acquire_config_write_lock(&config_file)?;
    recover_pending_publication(&config_file)?;
    let (workspaces, pending_writes) = load_workspace_files_locked(workspaces_dir)?;
    commit_pending_config_writes(pending_writes)?;
    Ok(workspaces)
}

fn load_workspace_files_locked(
    workspaces_dir: &Path,
) -> crate::ConfigResult<(BTreeMap<String, WorkspaceConfig>, Vec<PendingConfigWrite>)> {
    let mut workspaces = BTreeMap::new();
    let entries = match std::fs::read_dir(workspaces_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((workspaces, Vec::new())),
        Err(e) => {
            return Err(anyhow::Error::new(e)
                .context(format!(
                    "reading workspaces directory {}",
                    workspaces_dir.display()
                ))
                .into());
        }
    };

    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| {
            format!("scanning workspaces directory {}", workspaces_dir.display())
        })?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        paths.push(path);
    }
    paths.sort();

    let mut pending_writes = Vec::new();
    for path in paths {
        let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
            anyhow::Error::from(ConfigError::msg(format!(
                "invalid workspace filename {}",
                path.display()
            )))
        })?;
        let name = WorkspaceName::parse(stem)
            .with_context(|| format!("invalid workspace filename {}", path.display()))?;
        let migration = (|| -> crate::ConfigResult<_> {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            normalize_workspace_contents(&raw)
                .map_err(WorkspaceNormalizationError::into_config_error)
        })();
        let (raw, needs_write) = match migration {
            Ok((raw, migrated_from, needs_write)) => {
                let event_result = Ok(migrated_from.clone());
                migrations::emit_migration_result(
                    "workspace",
                    CURRENT_WORKSPACE_VERSION,
                    migrations::WORKSPACE_MIGRATIONS,
                    &event_result,
                );
                (raw, needs_write)
            }
            Err(error) => {
                let event_result = Err(ConfigError::msg("workspace migration failed"));
                migrations::emit_migration_result(
                    "workspace",
                    CURRENT_WORKSPACE_VERSION,
                    migrations::WORKSPACE_MIGRATIONS,
                    &event_result,
                );
                return Err(error);
            }
        };
        if needs_write {
            pending_writes.push(PendingConfigWrite {
                path: path.clone(),
                contents: raw.clone(),
            });
        }
        let workspace = toml::from_str(&raw)
            .with_context(|| format!("parsing workspace config {}", path.display()))?;
        workspaces.insert(name.into_inner(), workspace);
    }

    Ok((workspaces, pending_writes))
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

/// Validate every embedded workspace and plan only the split files that need
/// to be created. This pass must stay read/compute-only: both
/// `AppConfig::load_or_init` and `ConfigEditor::open` depend on a conflict in
/// any later workspace leaving the complete config tree untouched.
fn plan_legacy_workspace_writes(
    paths: &JackinPaths,
    workspaces: &BTreeMap<String, WorkspaceConfig>,
    legacy_op_accounts: &BTreeMap<String, String>,
    existing_workspaces: &BTreeMap<String, WorkspaceConfig>,
) -> anyhow::Result<Vec<PendingConfigWrite>> {
    let mut writes = Vec::new();
    for (name, workspace) in workspaces {
        validate_workspace_file_stem(name)?;
        let path = workspace_file_path(paths, name);
        let contents = legacy_workspace_contents(
            name,
            workspace,
            legacy_op_accounts.get(name).map(String::as_str),
        )?;
        let desired: WorkspaceConfig = toml::from_str(&contents)
            .with_context(|| format!("parsing migrated workspace {name:?}"))?;
        if let Some(existing) = existing_workspaces.get(name) {
            // The split loader has already normalized versioned/legacy bytes
            // in memory and queued any required rewrite. Compare semantic
            // current-schema values, never raw on-disk versions or fields.
            if existing == &desired {
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
        writes.push(PendingConfigWrite { path, contents });
    }
    Ok(writes)
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

fn validate_config_semantics(config: &AppConfig) -> crate::ConfigResult<()> {
    validate_reserved_env_names(config)?;
    config.validate_accounts()?;
    config.validate_workspaces()
}

fn validate_editor_config_semantics(config: &AppConfig) -> crate::ConfigResult<()> {
    validate_reserved_env_names(config)?;
    config.validate_accounts()
}

/// `true` when `raw` still embeds non-empty `[workspaces]` tables.
///
/// Every embedded-workspace document must take the in-memory migration and
/// split path. A versioned document can still require splitting, and writing
/// its schema migration first would mutate the global file before a conflicting
/// split file is rejected.
pub fn config_needs_split_migration(raw: &str) -> crate::ConfigResult<bool> {
    let doc: DocumentMut = raw.parse().context("parsing config.toml")?;
    let has_legacy_workspaces = doc
        .get("workspaces")
        .and_then(toml_edit::Item::as_table)
        .is_some_and(|workspaces| !workspaces.is_empty());
    Ok(has_legacy_workspaces)
}

pub(crate) fn load_config_contents(paths: &JackinPaths) -> crate::ConfigResult<Option<String>> {
    // Keep migration read-only here. `load_split_config_locked` combines the
    // global and split plans, validates the complete config, then commits them
    // under the caller's write lock.
    match std::fs::read_to_string(&paths.config_file) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(anyhow::Error::new(error)
            .context(format!("reading {}", paths.config_file.display()))
            .into()),
    }
}

impl AppConfig {
    /// Load `config.toml` (migrate as needed), split workspaces, sync builtins, validate.
    pub fn load_or_init(paths: &JackinPaths) -> crate::ConfigResult<Self> {
        paths.ensure_base_dirs()?;
        let lock = acquire_config_write_lock(&paths.config_file)?;
        let loaded = (|| {
            recover_pending_publication(&paths.config_file)?;
            let contents_opt = load_config_contents(paths)?;
            let loaded = load_split_config_locked(paths, contents_opt)?;

            crate::telemetry::finish_operation(
                jackin_telemetry::schema::enums::ConfigScope::Global,
                jackin_telemetry::schema::enums::ConfigOperation::Validate,
                (|| {
                    validate_reserved_env_names(&loaded.config)?;
                    loaded.config.validate_accounts()
                })(),
            )?;
            crate::telemetry::finish_operation(
                jackin_telemetry::schema::enums::ConfigScope::Workspace,
                jackin_telemetry::schema::enums::ConfigOperation::Validate,
                loaded.config.validate_workspaces(),
            )?;

            loaded.commit()
        })();
        let mut config = crate::telemetry::finish_operation(
            jackin_telemetry::schema::enums::ConfigScope::Global,
            jackin_telemetry::schema::enums::ConfigOperation::Load,
            loaded,
        )?;

        // Keep the exclusive lock across migration, builtin repair, and all
        // validation. Passing it into the editor avoids recursive acquisition
        // while preserving one writer scope for the tree.
        let builtins_changed = config.sync_builtin_agents();
        if builtins_changed {
            let mut editor = ConfigEditor::open_with_lock(paths, lock)?;
            for &(name, git) in super::roles::BUILTIN_ROLES {
                editor.upsert_builtin_agent(name, git);
            }
            // Take save()'s post-write parse: it preserves [roles.X.env] that
            // sync_builtin_agents cleared in-memory.
            config = editor.save()?;
        } else {
            drop(lock);
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests;
