//! `AppConfig` load/init behavior: TOML read, workspace-split migration,
//! reserved-env validation, and builtin-agent sync.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use jackin_core::JackinPaths;
use toml_edit::DocumentMut;

use super::AppConfig;
use crate::editor::ConfigEditor;
use crate::migrations;
use crate::persist::{atomic_write, validate_workspace_file_stem};
use crate::schema::WorkspaceConfig;

pub(crate) fn workspace_file_path(paths: &JackinPaths, name: &str) -> PathBuf {
    paths.workspaces_dir.join(format!("{name}.toml"))
}

pub fn load_split_config(
    paths: &JackinPaths,
    contents_opt: Option<String>,
) -> anyhow::Result<AppConfig> {
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
        Some(c) => toml::from_str(&c)?,
        None => AppConfig::default(),
    };

    let legacy_workspaces = std::mem::take(&mut config.workspaces);
    if !legacy_workspaces.is_empty() {
        migrate_legacy_workspaces(paths, &config, &legacy_workspaces, &legacy_op_accounts)?;
        // Silent automatic upgrade — record in the run diagnostics log only.
        jackin_diagnostics::debug_log!(
            "config",
            "migrated saved workspaces into {}",
            paths.workspaces_dir.display()
        );
    }

    config.workspaces = load_workspace_files(&paths.workspaces_dir)?;
    Ok(config)
}

pub fn load_workspace_files(
    workspaces_dir: &Path,
) -> anyhow::Result<BTreeMap<String, WorkspaceConfig>> {
    let mut workspaces = BTreeMap::new();
    let entries = match std::fs::read_dir(workspaces_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(workspaces),
        Err(e) => {
            return Err(e).with_context(|| {
                format!("reading workspaces directory {}", workspaces_dir.display())
            });
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
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid workspace filename {}", path.display()))?;
        validate_workspace_file_stem(stem)
            .with_context(|| format!("invalid workspace filename {}", path.display()))?;
        migrations::migrate_workspace_file_if_needed(&path)?;
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading workspace config {}", path.display()))?;
        let workspace = toml::from_str(&raw)
            .with_context(|| format!("parsing workspace config {}", path.display()))?;
        workspaces.insert(stem.to_owned(), workspace);
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
                anyhow::bail!("workspace {name:?}: `op_account` must be a string, found {item:?}")
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
        let contents = toml::to_string_pretty(workspace)
            .with_context(|| format!("serializing workspace {name:?}"))?;
        let contents = match legacy_op_accounts.get(name) {
            // Re-inject the legacy account and run the same v1alpha5
            // transform the version chain would have applied, so the
            // account lands on each op ref instead of being lost.
            Some(acct) => {
                let mut doc: DocumentMut = contents
                    .parse()
                    .with_context(|| format!("re-parsing serialized workspace {name:?}"))?;
                doc["op_account"] = toml_edit::value(acct.as_str());
                migrations::migrate_workspace_op_account_to_refs(&mut doc).with_context(|| {
                    format!("stamping legacy op_account onto refs for workspace {name:?}")
                })?;
                doc.to_string()
            }
            None => contents,
        };
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
            anyhow::bail!(
                "cannot migrate workspace {name:?}: {} already exists with different contents \
                 than the legacy config.toml. Reconcile the two copies (delete the split file to \
                 take the legacy version, or remove [workspaces.{name}] from config.toml to take \
                 the split file) and re-run.",
                path.display()
            );
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

/// Reject operator env maps that declare any reserved runtime name.
pub fn validate_reserved_env_names(config: &AppConfig) -> anyhow::Result<()> {
    let mut offenses: Vec<String> = Vec::new();
    let mut check = |layer: &str, env: &BTreeMap<String, jackin_core::EnvValue>| {
        for key in env.keys() {
            if jackin_core::env_model::is_reserved(key) {
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
    anyhow::bail!(
        "config contains reserved jackin runtime env vars:\n{}",
        offenses.join("\n")
    )
}

pub fn config_needs_split_migration(raw: &str) -> anyhow::Result<bool> {
    let doc: DocumentMut = raw.parse().context("parsing config.toml")?;
    let version = migrations::doc_version(&doc, "config")?;
    let has_legacy_workspaces = doc
        .get("workspaces")
        .and_then(toml_edit::Item::as_table)
        .is_some_and(|workspaces| !workspaces.is_empty());
    Ok(version == migrations::SchemaVersion::Legacy && has_legacy_workspaces)
}

impl AppConfig {
    pub fn load_or_init(paths: &JackinPaths) -> anyhow::Result<Self> {
        paths.ensure_base_dirs()?;

        let contents_opt = match std::fs::read_to_string(&paths.config_file) {
            Ok(raw) => {
                if config_needs_split_migration(&raw)? {
                    Some(raw)
                } else {
                    migrations::migrate_config_file_if_needed(&paths.config_file)?;
                    Some(
                        std::fs::read_to_string(&paths.config_file).with_context(|| {
                            format!("re-reading {} after migration", paths.config_file.display())
                        })?,
                    )
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                return Err(e).with_context(|| format!("reading {}", paths.config_file.display()));
            }
        };

        let mut config = load_split_config(paths, contents_opt)?;

        // Pre-sync validation: gives the operator a reserved-name error
        // rather than save()'s "rejecting candidate config" wrapper.
        // ConfigEditor::save runs the same check via validate_candidate;
        // this call covers the path where save() is never invoked because
        // builtins did not drift.
        validate_reserved_env_names(&config)?;
        config.validate_auth_modes()?;

        let builtins_changed = config.sync_builtin_agents();

        if builtins_changed {
            // ConfigEditor::open recurses into load_or_init when the file is
            // missing; bootstrap once here so the editor sees an existing
            // file and preserves operator comments rather than going through
            // the lossy serde rewrite.
            if !paths.config_file.exists() {
                let contents = toml::to_string_pretty(&config)?;
                atomic_write(&paths.config_file, &contents)?;
            }
            let mut editor = ConfigEditor::open(paths)?;
            for &(name, git) in super::roles::BUILTIN_ROLES {
                editor.upsert_builtin_agent(name, git);
            }
            // Take save()'s post-write parse: it preserves [roles.X.env] that
            // sync_builtin_agents cleared in-memory.
            config = editor.save()?;
        }

        config.validate_workspaces()?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests;
