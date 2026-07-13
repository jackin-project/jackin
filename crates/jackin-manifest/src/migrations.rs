//! Role manifest (`jackin.role.toml`) version migration registry.
//!
//! `CURRENT_MANIFEST_VERSION` is defined in `jackin-core` and re-exported
//! here for backward-compatibility. Migration steps delegate to shared
//! helpers in `jackin-config`. Not responsible for config or workspace
//! migrations — those live in `jackin-config`.

use std::path::Path;

use anyhow::bail;
use toml_edit::DocumentMut;

/// Current role-manifest schema version string (e.g. `"v1alpha6"`).
pub use jackin_core::constants::CURRENT_MANIFEST_VERSION;

const MANIFEST_MIGRATIONS: &[jackin_config::MigrationStep] = &[
    jackin_config::MigrationStep {
        from: jackin_config::LEGACY_VERSION,
        to: "v1alpha1",
        migrate: jackin_config::noop_migration,
    },
    jackin_config::MigrationStep {
        from: "v1alpha1",
        to: "v1alpha2",
        migrate: jackin_config::noop_migration,
    },
    jackin_config::MigrationStep {
        from: "v1alpha2",
        to: "v1alpha3",
        migrate: jackin_config::noop_migration,
    },
    jackin_config::MigrationStep {
        from: "v1alpha3",
        to: "v1alpha4",
        migrate: jackin_config::noop_migration,
    },
    // v1alpha4 -> v1alpha5: add optional `[<agent>.providers.<id>]` per-provider
    // model overrides. Additive with serde defaults; no transformation needed.
    jackin_config::MigrationStep {
        from: "v1alpha4",
        to: "v1alpha5",
        migrate: jackin_config::noop_migration,
    },
    // v1alpha5 -> v1alpha6: add optional role `[docker]` security settings.
    // Additive with serde defaults; no transformation needed.
    jackin_config::MigrationStep {
        from: "v1alpha5",
        to: CURRENT_MANIFEST_VERSION,
        migrate: jackin_config::noop_migration,
    },
];

/// Serde-default helper returning [`CURRENT_MANIFEST_VERSION`] as a `String`.
pub use jackin_core::constants::current_manifest_version;

/// Migrate `path` (typically `<repo>/jackin.role.toml`) to
/// `CURRENT_MANIFEST_VERSION`.
///
/// Returns `Some((old, new))` when a migration ran, `None` when the manifest
/// was already current. `old` and `new` are display strings (`"legacy"`,
/// `"v1alpha2"`) so CLI callers can print them as-is without needing the
/// structured `SchemaVersion`.
pub fn migrate_manifest_file(path: &Path) -> anyhow::Result<Option<(String, String)>> {
    let outcome = jackin_config::migrate_file_if_needed(
        path,
        "role manifest",
        CURRENT_MANIFEST_VERSION,
        MANIFEST_MIGRATIONS,
    )?;
    Ok(outcome.map(|old| (old.to_string(), CURRENT_MANIFEST_VERSION.to_owned())))
}

/// Parse and accept the role-manifest schema version in `doc`.
///
/// Accepts versions at or below [`CURRENT_MANIFEST_VERSION`]. Rejects
/// newer schemas that this binary cannot interpret.
///
/// # Errors
/// Returns an error when the version field is missing/invalid or newer than
/// this binary understands.
pub fn validate_manifest_version(
    doc: &DocumentMut,
) -> anyhow::Result<jackin_config::SchemaVersion> {
    let version = jackin_config::doc_version(doc, "role manifest")?;
    let current = jackin_config::parse_version(CURRENT_MANIFEST_VERSION)?;
    match version.cmp(&current) {
        std::cmp::Ordering::Greater => bail!(
            "role manifest is at {version}, this binary only understands up to {CURRENT_MANIFEST_VERSION}; upgrade jackin"
        ),
        std::cmp::Ordering::Less | std::cmp::Ordering::Equal => Ok(version),
    }
}

#[cfg(test)]
mod tests;
