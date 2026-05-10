use std::path::Path;

use anyhow::bail;
use toml_edit::DocumentMut;

pub const CURRENT_MANIFEST_VERSION: &str = "v1alpha1";

const MANIFEST_MIGRATIONS: &[crate::config::migrations::MigrationStep] =
    &[crate::config::migrations::MigrationStep {
        from: crate::config::migrations::LEGACY_VERSION,
        to: CURRENT_MANIFEST_VERSION,
        migrate: crate::config::migrations::noop_migration,
    }];

pub fn current_manifest_version() -> String {
    CURRENT_MANIFEST_VERSION.to_string()
}

/// Migrate `path` (typically `<repo>/jackin.role.toml`) to
/// `CURRENT_MANIFEST_VERSION`.
///
/// Returns `Some((old, new))` when a migration ran, `None` when the manifest
/// was already current. `old` and `new` are display strings (`"legacy"`,
/// `"v1alpha1"`) — `jackin-validate --migrate` prints them as-is and does
/// not need the structured `SchemaVersion`.
pub fn migrate_manifest_file(path: &Path) -> anyhow::Result<Option<(String, String)>> {
    let outcome = crate::config::migrations::migrate_file_if_needed(
        path,
        "role manifest",
        CURRENT_MANIFEST_VERSION,
        MANIFEST_MIGRATIONS,
    )?;
    Ok(outcome.map(|old| (old.to_string(), CURRENT_MANIFEST_VERSION.to_string())))
}

pub(crate) fn validate_manifest_version(doc: &DocumentMut, role_name: &str) -> anyhow::Result<()> {
    let version = crate::config::migrations::doc_version(doc, "role manifest")?;
    let current = crate::config::migrations::parse_version(CURRENT_MANIFEST_VERSION)?;
    match version.cmp(&current) {
        std::cmp::Ordering::Less => bail!(
            "role \"{role_name}\" manifest is at {version}, expected {CURRENT_MANIFEST_VERSION}; run \"jackin-validate --migrate <role-repo-path>\" to upgrade the local copy"
        ),
        std::cmp::Ordering::Greater => bail!(
            "role manifest is at {version}, this binary only understands up to {CURRENT_MANIFEST_VERSION}; upgrade jackin"
        ),
        std::cmp::Ordering::Equal => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn migrates_missing_manifest_version() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("jackin.role.toml");
        std::fs::write(&path, "# keep me\ndockerfile = \"Dockerfile\"\n").unwrap();

        let (old, new) = migrate_manifest_file(&path).unwrap().unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        let parsed: toml::Value = toml::from_str(&out).unwrap();

        assert_eq!(old, "legacy");
        assert_eq!(new, "v1alpha1");
        assert_eq!(parsed["version"].as_str().unwrap(), "v1alpha1");
        assert!(out.contains("# keep me"), "{out}");
    }

    #[test]
    fn current_manifest_migration_is_noop() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("jackin.role.toml");
        std::fs::write(
            &path,
            "version = \"v1alpha1\"\ndockerfile = \"Dockerfile\"\n",
        )
        .unwrap();

        assert!(migrate_manifest_file(&path).unwrap().is_none());
    }

    #[test]
    fn rejects_newer_manifest_version() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("jackin.role.toml");
        std::fs::write(
            &path,
            "version = \"v2alpha1\"\ndockerfile = \"Dockerfile\"\n",
        )
        .unwrap();

        let err = migrate_manifest_file(&path).unwrap_err();
        assert!(
            err.to_string().contains("only understands up to v1alpha1"),
            "{err}"
        );
    }

    #[test]
    fn validate_manifest_version_accepts_current() {
        let doc: DocumentMut = "version = \"v1alpha1\"\n".parse().unwrap();
        validate_manifest_version(&doc, "test-role").unwrap();
    }

    #[test]
    fn validate_manifest_version_rejects_legacy_with_migrate_hint() {
        let doc: DocumentMut = "dockerfile = \"Dockerfile\"\n".parse().unwrap();
        let err = validate_manifest_version(&doc, "the-architect").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("\"the-architect\""), "{msg}");
        assert!(msg.contains("at legacy"), "{msg}");
        assert!(msg.contains("jackin-validate --migrate"), "{msg}");
    }

    #[test]
    fn validate_manifest_version_rejects_newer() {
        let doc: DocumentMut = "version = \"v2alpha1\"\n".parse().unwrap();
        let err = validate_manifest_version(&doc, "test-role").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("only understands up to v1alpha1"), "{msg}");
    }

    #[test]
    fn manifest_migrations_chain_reaches_current() {
        // Production registry must form a contiguous chain from `legacy` to
        // CURRENT_MANIFEST_VERSION. A typo in `from` or `to` would only
        // surface when an operator actually triggered a walk; this test
        // catches it on every CI run.
        let current = crate::config::migrations::parse_version(CURRENT_MANIFEST_VERSION).unwrap();
        let mut cursor = crate::config::migrations::SchemaVersion::Legacy;
        let mut steps_taken = 0;
        while cursor < current {
            let step = MANIFEST_MIGRATIONS
                .iter()
                .find(|s| {
                    crate::config::migrations::parse_registry_version(s.from)
                        .map(|v| v == cursor)
                        .unwrap_or(false)
                })
                .unwrap_or_else(|| panic!("no manifest step from {cursor}"));
            let next = crate::config::migrations::parse_registry_version(step.to).unwrap();
            assert!(
                next > cursor,
                "step {} -> {} not forward",
                step.from,
                step.to
            );
            cursor = next;
            steps_taken += 1;
            assert!(steps_taken <= MANIFEST_MIGRATIONS.len(), "registry cycle");
        }
        assert_eq!(cursor, current);
    }
}
