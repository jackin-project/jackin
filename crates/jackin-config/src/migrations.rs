// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Config and workspace file version migration registry.
//!
//! Defines `MigrationStep`, and the `CONFIG_MIGRATIONS` / `WORKSPACE_MIGRATIONS`
//! chains. Not responsible for manifest migrations — those live in the binary
//! crate's `manifest/migrations.rs`. One version bump per PR targeting the next
//! version after `main`.

use crate::ConfigError;
use std::num::NonZeroU32;
use std::path::Path;

use anyhow::Context;
use toml_edit::DocumentMut;

use crate::persist::atomic_write;
use crate::versions::{CURRENT_CONFIG_VERSION, CURRENT_WORKSPACE_VERSION, LEGACY_VERSION};

/// Transform applied to a TOML document for one version step.
pub type Migration = fn(&mut DocumentMut) -> crate::ConfigResult<()>;

/// One edge in a config or workspace migration registry.
#[expect(
    missing_debug_implementations,
    reason = "MigrationStep stores a function pointer; debug output would not add useful migration evidence."
)]
#[derive(Clone, Copy)]
pub struct MigrationStep {
    /// Source version string (`legacy` or `vN…`).
    pub from: &'static str,
    /// Destination version string after this step.
    pub to: &'static str,
    /// Document transform (may be [`noop_migration`]).
    pub migrate: Migration,
}

/// Ordered `config.toml` migration chain from [`LEGACY_VERSION`] to current.
pub const CONFIG_MIGRATIONS: &[MigrationStep] = &[
    MigrationStep {
        from: LEGACY_VERSION,
        to: "v1alpha1",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha1",
        to: "v1alpha2",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha2",
        to: "v1alpha3",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha3",
        to: "v1alpha4",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha4",
        to: "v1alpha5",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha5",
        to: "v1alpha6",
        migrate: noop_migration,
    },
    // v1alpha6 → v1alpha7: add optional Docker profile/grants config and
    // selectable container backends (`[runtime].default_backend`). Additive
    // with serde defaults; no transformation needed.
    MigrationStep {
        from: "v1alpha6",
        to: "v1alpha7",
        migrate: noop_migration,
    },
    // v1alpha7 → v1alpha8: add optional `dirty_exit_policy` to AppConfig and
    // WorkspaceConfig, and exec control-protocol types. Additive with serde
    // defaults; no transformation needed.
    MigrationStep {
        from: "v1alpha7",
        to: "v1alpha8",
        migrate: noop_migration,
    },
    // v1alpha8 -> v1alpha9: add optional `[telemetry]` settings to AppConfig.
    // Additive with serde defaults; no transformation needed.
    MigrationStep {
        from: "v1alpha8",
        to: CURRENT_CONFIG_VERSION,
        migrate: noop_migration,
    },
];
/// Ordered per-workspace file migration chain from [`LEGACY_VERSION`] to current.
pub const WORKSPACE_MIGRATIONS: &[MigrationStep] = &[
    MigrationStep {
        from: LEGACY_VERSION,
        to: "v1alpha1",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha1",
        to: "v1alpha2",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha2",
        to: "v1alpha3",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha3",
        to: "v1alpha4",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha4",
        to: "v1alpha5",
        migrate: migrate_workspace_op_account_to_refs,
    },
    MigrationStep {
        from: "v1alpha5",
        to: "v1alpha6",
        migrate: noop_migration,
    },
    // v1alpha6 → v1alpha7: add optional workspace Docker profile/grants config,
    // optional on-demand env metadata (`on_demand`), and optional
    // `[runtime].backend` per-workspace override. Additive with serde defaults;
    // no transformation needed.
    MigrationStep {
        from: "v1alpha6",
        to: "v1alpha7",
        migrate: noop_migration,
    },
    // v1alpha7 → v1alpha8: add optional `dirty_exit_policy` to WorkspaceConfig.
    // Additive with serde default; no transformation needed.
    MigrationStep {
        from: "v1alpha7",
        to: CURRENT_WORKSPACE_VERSION,
        migrate: noop_migration,
    },
];

/// v1alpha4 → v1alpha5: the workspace-level `op_account` moves onto each
/// `op://` env ref as a per-ref `account` key, so a workspace holding
/// refs from several 1Password accounts resolves each correctly.
///
/// Walks every env table that can hold op refs (`[env]`,
/// `[roles.<role>.env]`, `[github.env]`, `[roles.<role>.github.env]`)
/// and stamps the old top-level `op_account` onto each inline-table
/// value carrying an `op` key that lacks an `account`. Plain string
/// values are skipped. Absent `op_account` is a no-op.
///
/// Exposed beyond the migration registry so the legacy-config split in
/// `persist.rs` can reuse this exact transform: the typed-struct round-trip
/// there drops the legacy `op_account` before the version-driven migration
/// would see it, so the split re-injects it and calls this directly.
pub fn migrate_workspace_op_account_to_refs(doc: &mut DocumentMut) -> crate::ConfigResult<()> {
    // Absent op_account is a legitimate no-op (single-account / never-set
    // workspace). A present-but-non-string value is operator data we must
    // not silently drop: bail loudly so the standard startup parser error
    // surfaces, rather than discarding the account and presenting a
    // downstream phantom "missing credential" at next launch.
    let acct = match doc.get("op_account") {
        None => return Ok(()),
        Some(item) => match item.as_str() {
            Some(s) => s.to_owned(),
            None => {
                return Err(ConfigError::msg(format!(
                    "workspace migration v1alpha4 → v1alpha5: `op_account` must be a string, \
                     found {item:?}"
                )));
            }
        },
    };

    stamp_account_in_env(doc.as_table_mut(), &acct);
    if let Some(roles) = doc.get_mut("roles").and_then(toml_edit::Item::as_table_mut) {
        for (_, role) in roles.iter_mut() {
            if let Some(role_tbl) = role.as_table_mut() {
                stamp_account_in_env(role_tbl, &acct);
            }
        }
    }

    doc.remove("op_account");
    Ok(())
}

/// Stamp `account` onto every op ref inside `table`'s `[env]` and
/// `[github.env]` sub-tables. An op ref is a table — inline (`KEY = { op
/// = … }`) or standard (`[env.KEY]`) — with an `op` key; refs already
/// carrying `account` are left untouched.
fn stamp_account_in_env(table: &mut toml_edit::Table, acct: &str) {
    stamp_account_in_env_table(table.get_mut("env"), acct);
    if let Some(github) = table
        .get_mut("github")
        .and_then(toml_edit::Item::as_table_mut)
    {
        stamp_account_in_env_table(github.get_mut("env"), acct);
    }
}

fn stamp_account_in_env_table(env: Option<&mut toml_edit::Item>, acct: &str) {
    let Some(env) = env.and_then(toml_edit::Item::as_table_like_mut) else {
        return;
    };
    for (_, value) in env.iter_mut() {
        // Match both the inline-table form (written by the editor) and the
        // standard-table form (`[env.KEY]`, as a serde round-trip emits) so
        // the legacy-config split is stamped the same as a normal migration.
        if let Some(tbl) = value.as_table_like_mut()
            && tbl.contains_key("op")
            && !tbl.contains_key("account")
        {
            tbl.insert("account", toml_edit::value(acct));
        }
    }
}

/// Schema version of a jackin-owned configuration file.
///
/// Variant order is load-bearing: `Legacy < Kubernetes(_)` lets the migration
/// walker treat a missing `version` field as the lowest possible value
/// without sprinkling `Option`-handling across call sites.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SchemaVersion {
    /// File predates versioning (no `version` key in the document).
    Legacy,
    /// Kubernetes-style `vN` / `vNalphaM` / `vNbetaM` version.
    Kubernetes(KubernetesVersion),
}

/// Field order is load-bearing: derived `Ord` compares `major` first, then
/// `channel`, which gives the expected `v1alpha1 < v1beta1 < v1 < v2alpha1`
/// ordering.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KubernetesVersion {
    /// Major API version number (non-zero).
    pub major: NonZeroU32,
    /// Stability channel and optional sequence.
    pub channel: Channel,
}

/// Kubernetes channel maturity.
///
/// Variant declaration order is load-bearing — `Alpha < Beta < Stable` is
/// required by the derived `Ord` and pinned by
/// `channel_order_is_alpha_beta_stable` in this module's tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Channel {
    /// `vNalphaM` channel with sequence `M`.
    Alpha(NonZeroU32),
    /// `vNbetaM` channel with sequence `M`.
    Beta(NonZeroU32),
    /// Stable `vN` (no channel suffix).
    Stable,
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Legacy => write!(f, "{LEGACY_VERSION}"),
            Self::Kubernetes(k) => write!(f, "{k}"),
        }
    }
}

impl std::fmt::Display for KubernetesVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.channel {
            Channel::Stable => write!(f, "v{}", self.major),
            Channel::Alpha(seq) => write!(f, "v{}alpha{seq}", self.major),
            Channel::Beta(seq) => write!(f, "v{}beta{seq}", self.major),
        }
    }
}

/// Migrate a global `config.toml` to the current schema if needed.
pub fn migrate_config_file_if_needed(path: &Path) -> crate::ConfigResult<bool> {
    let result = migrate_file_if_needed(path, "config", CURRENT_CONFIG_VERSION, CONFIG_MIGRATIONS);
    emit_migration_result("global", CURRENT_CONFIG_VERSION, CONFIG_MIGRATIONS, &result);
    result
        .map(|version| version.is_some())
        .map_err(ConfigError::telemetry_owned)
}

/// Migrate a split workspace file to the current schema if needed.
pub fn migrate_workspace_file_if_needed(path: &Path) -> crate::ConfigResult<bool> {
    let result = migrate_file_if_needed(
        path,
        "workspace config",
        CURRENT_WORKSPACE_VERSION,
        WORKSPACE_MIGRATIONS,
    );
    emit_migration_result(
        "workspace",
        CURRENT_WORKSPACE_VERSION,
        WORKSPACE_MIGRATIONS,
        &result,
    );
    result
        .map(|version| version.is_some())
        .map_err(ConfigError::telemetry_owned)
}

fn emit_migration_result(
    scope: &'static str,
    current: &'static str,
    migrations: &[MigrationStep],
    result: &crate::ConfigResult<Option<SchemaVersion>>,
) {
    let from = result
        .as_ref()
        .ok()
        .and_then(Option::as_ref)
        .map(ToString::to_string);
    let outcome = match result {
        Ok(Some(_)) => "success",
        Ok(None) => "skip",
        Err(_) => "failure",
    };
    let mut attrs = vec![
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::CONFIG_SCOPE,
            value: jackin_telemetry::Value::Str(scope),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::CONFIG_OPERATION,
            value: jackin_telemetry::Value::Str("migrate"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str(outcome),
        },
    ];
    if let Some(from) = from.as_deref() {
        let step_count = u64::try_from(
            migrations
                .iter()
                .skip_while(|step| step.from != from)
                .take_while(|step| step.from != current)
                .count(),
        )
        .unwrap_or(u64::MAX);
        attrs.extend([
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_SCHEMA_VERSION_FROM,
                value: jackin_telemetry::Value::Str(from),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_SCHEMA_VERSION_TO,
                value: jackin_telemetry::Value::Str(current),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_MIGRATION_STEP_COUNT,
                value: jackin_telemetry::Value::U64(step_count),
            },
        ]);
    }
    if result.is_err() {
        attrs.push(jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
            value: jackin_telemetry::Value::Str("config_error"),
        });
    }
    let _result = jackin_telemetry::emit_event(
        &jackin_telemetry::event::CONFIG_OPERATION,
        jackin_telemetry::FieldSet::new(&attrs, None),
    );
}

/// Read `path`, run any pending migrations, write the result back atomically.
///
/// Returns `Some(old_version)` when a migration ran, `None` when the file
/// was already at `current_raw`. Emits a governed configuration-operation event
/// without configuration values or paths. Wrappers
/// (e.g. `manifest::migrations::migrate_manifest_file`) project the
/// `SchemaVersion` into display strings for callers that need to print
/// both ends.
pub fn migrate_file_if_needed(
    path: &Path,
    label: &str,
    current_raw: &str,
    migrations: &[MigrationStep],
) -> crate::ConfigResult<Option<SchemaVersion>> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut doc: DocumentMut = raw
        .parse()
        .with_context(|| format!("parsing {}", path.display()))?;
    let old_version = doc_version(&doc, label)?;
    let current = parse_version(current_raw)?;

    if old_version > current {
        return Err(ConfigError::msg(format!(
            "{label} is at {old_version}, this binary only understands up to {current_raw}; upgrade jackin"
        )));
    }
    if old_version == current {
        return Ok(None);
    }

    apply_migrations(&mut doc, &old_version, &current, migrations, label)?;
    atomic_write(path, &doc.to_string())
        .with_context(|| format!("writing migrated {label} to {}", path.display()))?;
    Ok(Some(old_version))
}

/// Walk the registry from `old_version` to `current_version`, mutating `doc` in place.
///
/// After each step the framework stamps `step.to` into the document —
/// migration functions transform content; they must not write `version` themselves.
pub fn apply_migrations(
    doc: &mut DocumentMut,
    old_version: &SchemaVersion,
    current_version: &SchemaVersion,
    migrations: &[MigrationStep],
    label: &str,
) -> crate::ConfigResult<()> {
    let mut cursor = old_version.clone();
    while &cursor < current_version {
        let Some(step) = find_step(&cursor, migrations)? else {
            return Err(ConfigError::msg(format!(
                "{label} is at {old_version}, but this binary no longer includes a migration path to {current_version}; upgrade through an older jackin first"
            )));
        };
        let next = parse_registry_version(step.to)?;
        if next <= cursor {
            return Err(ConfigError::msg(format!(
                "{label} migration registry is invalid: step {} -> {} does not move forward",
                step.from, step.to
            )));
        }
        (step.migrate)(doc)
            .with_context(|| format!("running {label} migration {} -> {}", step.from, step.to))?;
        set_doc_version(doc, step.to);
        cursor = next;
    }
    if &cursor != current_version {
        return Err(ConfigError::msg(format!(
            "{label} migration registry stopped at {cursor}, expected {current_version}"
        )));
    }
    Ok(())
}

fn find_step<'a>(
    from: &SchemaVersion,
    migrations: &'a [MigrationStep],
) -> anyhow::Result<Option<&'a MigrationStep>> {
    for step in migrations {
        if parse_registry_version(step.from)? == *from {
            return Ok(Some(step));
        }
    }
    Ok(None)
}

/// Parse a registry `from`/`to` string (`legacy` or Kubernetes-style).
pub fn parse_registry_version(version: &str) -> crate::ConfigResult<SchemaVersion> {
    if version == LEGACY_VERSION {
        return Ok(SchemaVersion::Legacy);
    }
    parse_version(version)
}

/// Read and parse the document's `version` field (`Legacy` when absent).
pub fn doc_version(doc: &DocumentMut, label: &str) -> crate::ConfigResult<SchemaVersion> {
    let Some(item) = doc.get("version") else {
        return Ok(SchemaVersion::Legacy);
    };
    let Some(version) = item.as_str() else {
        return Err(ConfigError::msg(format!(
            "{label} version must be a string"
        )));
    };
    Ok(parse_version(version).with_context(|| format!("{label} version is invalid"))?)
}

// Hand-rolled parser for Kubernetes-style versions (`v1`, `v1betaN`,
// `v1alphaN`). No canonical Rust crate fits — `semver` parses `MAJOR.MINOR.PATCH`,
// `kube`/`k8s_openapi` are heavy and pull a runtime, and the grammar here is
// small enough that adding a dependency is overkill (per AGENTS.md
// "Prefer libraries over hand-rolled parsers" carve-out).
/// Parse a Kubernetes-style schema version string (`v1`, `v1alpha2`, …).
pub fn parse_version(version: &str) -> crate::ConfigResult<SchemaVersion> {
    let rest = version
        .strip_prefix('v')
        .ok_or_else(|| anyhow::Error::from(ConfigError::msg("version must start with `v`")))?;
    let (major_raw, suffix) = split_first_nondigit(rest)
        .ok_or_else(|| anyhow::Error::from(ConfigError::msg("missing major version")))?;
    let major = parse_canonical_u32(major_raw, "major version")?;
    let major = NonZeroU32::new(major).ok_or_else(|| {
        anyhow::Error::from(ConfigError::msg("major version must be greater than zero"))
    })?;

    let channel = if suffix.is_empty() {
        Channel::Stable
    } else if let Some(seq_raw) = suffix.strip_prefix("alpha") {
        Channel::Alpha(parse_sequence("alpha", seq_raw)?)
    } else if let Some(seq_raw) = suffix.strip_prefix("beta") {
        Channel::Beta(parse_sequence("beta", seq_raw)?)
    } else {
        return Err(ConfigError::msg(
            "version must look like v1, v1beta1, or v1alpha1",
        ));
    };

    Ok(SchemaVersion::Kubernetes(KubernetesVersion {
        major,
        channel,
    }))
}

fn parse_sequence(prefix: &str, raw: &str) -> anyhow::Result<NonZeroU32> {
    if raw.is_empty() {
        return Err(
            ConfigError::msg(format!("{prefix} version must include a sequence number")).into(),
        );
    }
    let value = parse_canonical_u32(raw, &format!("{prefix} sequence"))?;
    NonZeroU32::new(value).ok_or_else(|| {
        anyhow::Error::from(ConfigError::msg(format!(
            "{prefix} sequence must be greater than zero"
        )))
    })
}

// Reject leading zeros so version strings round-trip canonically. Without
// this, `v01alpha01` would parse equal to `v1alpha1` but live on disk as the
// non-canonical form forever (the file is only rewritten when migrating).
fn parse_canonical_u32(raw: &str, label: &str) -> anyhow::Result<u32> {
    if raw.len() > 1 && raw.starts_with('0') {
        return Err(ConfigError::msg(format!("{label} must not have leading zeros")).into());
    }
    raw.parse::<u32>()
        .with_context(|| format!("invalid {label} {raw:?}"))
}

fn split_first_nondigit(s: &str) -> Option<(&str, &str)> {
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if split == 0 {
        return None;
    }
    Some(s.split_at(split))
}

/// Write `version` and sort the key so `version` appears first.
pub fn set_doc_version(doc: &mut DocumentMut, version: &str) {
    doc["version"] = toml_edit::value(version);
    doc.as_table_mut().sort_values_by(|left, _, right, _| {
        match (left.get() == "version", right.get() == "version") {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    });
}

// Stamp-only registry slot for transitions whose only delta is `version`.
// `apply_migrations` writes `step.to` to the document after each step, so
// these migrations are pure no-ops; content-changing migrations replace
// this with their own fn.
/// No-op content transform; the framework still stamps `step.to` as `version`.
pub const fn noop_migration(_doc: &mut DocumentMut) -> crate::ConfigResult<()> {
    Ok(())
}

/// Walk a `MigrationStep` slice from `Legacy` to `current_raw` and assert the chain is correct.
///
/// Catches typos in `from` / `to`, missing middle steps, backward steps,
/// cycles, and duplicate `from` values (which would silently fork the walker).
/// Production registries call this from a unit test so a registry mistake
/// fails CI rather than surfacing on an operator's machine.
#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "registry verifier is test-only assertion logic; invariant failures must fail the test with exact context"
)]
pub fn assert_registry_chain(migrations: &[MigrationStep], current_raw: &str) {
    let mut seen_froms = std::collections::BTreeSet::new();
    for step in migrations {
        let from = parse_registry_version(step.from)
            .unwrap_or_else(|_| panic!("step.from {:?} does not parse", step.from));
        assert!(
            seen_froms.insert(from),
            "duplicate `from` {} in registry",
            step.from
        );
    }

    let current = parse_version(current_raw).expect("current version parses");
    let mut cursor = SchemaVersion::Legacy;
    let mut steps_taken = 0;
    while cursor < current {
        let step = migrations
            .iter()
            .find(|s| parse_registry_version(s.from).is_ok_and(|v| v == cursor))
            .unwrap_or_else(|| panic!("no step from {cursor} in registry"));
        let next = parse_registry_version(step.to)
            .unwrap_or_else(|_| panic!("step.to {:?} does not parse", step.to));
        assert!(
            next > cursor,
            "step {} -> {} is not strictly forward",
            step.from,
            step.to
        );
        cursor = next;
        steps_taken += 1;
        assert!(steps_taken <= migrations.len(), "registry has a cycle");
    }
    assert_eq!(cursor, current, "registry does not reach {current_raw}");
    // Catches orphaned entries past `current` — e.g. a step left behind
    // after `CURRENT_*_VERSION` was rolled back, which would silently extend
    // the chain when current is bumped again.
    assert_eq!(
        steps_taken,
        migrations.len(),
        "registry has {} unreachable step(s) past {current_raw}",
        migrations.len() - steps_taken
    );
}

#[cfg(test)]
mod tests;
