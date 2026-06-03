//! Role manifest (`jackin.role.toml`) version migration registry.
//!
//! Owns `CURRENT_MANIFEST_VERSION` and the `MANIFEST_MIGRATIONS` chain.
//! Migration steps delegate to shared helpers in `config::migrations`. Not
//! responsible for config or workspace migrations — those live in
//! `config/migrations.rs`.

use std::path::Path;

use anyhow::bail;
use toml_edit::DocumentMut;

pub const CURRENT_MANIFEST_VERSION: &str = "v1alpha4";

const MANIFEST_MIGRATIONS: &[crate::config::migrations::MigrationStep] = &[
    crate::config::migrations::MigrationStep {
        from: crate::config::migrations::LEGACY_VERSION,
        to: "v1alpha1",
        migrate: crate::config::migrations::noop_migration,
    },
    crate::config::migrations::MigrationStep {
        from: "v1alpha1",
        to: "v1alpha2",
        migrate: crate::config::migrations::noop_migration,
    },
    crate::config::migrations::MigrationStep {
        from: "v1alpha2",
        to: "v1alpha3",
        migrate: crate::config::migrations::noop_migration,
    },
    crate::config::migrations::MigrationStep {
        from: "v1alpha3",
        to: CURRENT_MANIFEST_VERSION,
        migrate: crate::config::migrations::noop_migration,
    },
];

pub fn current_manifest_version() -> String {
    CURRENT_MANIFEST_VERSION.to_string()
}

/// Migrate `path` (typically `<repo>/jackin.role.toml`) to
/// `CURRENT_MANIFEST_VERSION`.
///
/// Returns `Some((old, new))` when a migration ran, `None` when the manifest
/// was already current. `old` and `new` are display strings (`"legacy"`,
/// `"v1alpha2"`) so CLI callers can print them as-is without needing the
/// structured `SchemaVersion`.
pub fn migrate_manifest_file(path: &Path) -> anyhow::Result<Option<(String, String)>> {
    let outcome = crate::config::migrations::migrate_file_if_needed(
        path,
        "role manifest",
        CURRENT_MANIFEST_VERSION,
        MANIFEST_MIGRATIONS,
    )?;
    Ok(outcome.map(|old| (old.to_string(), CURRENT_MANIFEST_VERSION.to_string())))
}

pub(crate) fn validate_manifest_version(
    doc: &DocumentMut,
) -> anyhow::Result<crate::config::migrations::SchemaVersion> {
    let version = crate::config::migrations::doc_version(doc, "role manifest")?;
    let current = crate::config::migrations::parse_version(CURRENT_MANIFEST_VERSION)?;
    match version.cmp(&current) {
        std::cmp::Ordering::Greater => bail!(
            "role manifest is at {version}, this binary only understands up to {CURRENT_MANIFEST_VERSION}; upgrade jackin"
        ),
        std::cmp::Ordering::Less | std::cmp::Ordering::Equal => Ok(version),
    }
}

#[cfg(test)]
mod tests;
