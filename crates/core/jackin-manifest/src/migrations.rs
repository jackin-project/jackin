// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use anyhow::bail;
use toml_edit::DocumentMut;

pub use jackin_core::CURRENT_MANIFEST_VERSION;

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
    jackin_config::MigrationStep {
        from: "v1alpha4",
        to: "v1alpha5",
        migrate: jackin_config::noop_migration,
    },
    jackin_config::MigrationStep {
        from: "v1alpha5",
        to: "v1alpha6",
        migrate: jackin_config::noop_migration,
    },
    jackin_config::MigrationStep {
        from: "v1alpha6",
        to: CURRENT_MANIFEST_VERSION,
        migrate: reject_removed_provider_models,
    },
];

fn reject_removed_provider_models(doc: &mut DocumentMut) -> jackin_config::ConfigResult<()> {
    for agent in ["claude", "codex", "opencode"] {
        if doc
            .get(agent)
            .and_then(|table| table.get("providers"))
            .is_some()
        {
            return Err(jackin_config::ConfigError::Message(format!(
                "removed [{agent}.providers] role settings: move provider models into named API accounts, assign those accounts to workspace agents, then remove the providers table and rerun jackin role migrate"
            )));
        }
    }
    Ok(())
}

pub use jackin_core::current_manifest_version;

pub fn migrate_manifest_file(path: &Path) -> anyhow::Result<Option<(String, String)>> {
    let outcome = jackin_config::migrate_file_if_needed(
        path,
        "role manifest",
        CURRENT_MANIFEST_VERSION,
        MANIFEST_MIGRATIONS,
    )?;
    Ok(outcome.map(|old| (old.to_string(), CURRENT_MANIFEST_VERSION.to_owned())))
}

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
