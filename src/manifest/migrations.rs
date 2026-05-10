use std::path::Path;

use anyhow::{Context, bail};
use toml_edit::DocumentMut;

pub const CURRENT_MANIFEST_VERSION: &str = "v1alpha1";

const MANIFEST_MIGRATIONS: &[crate::config::migrations::MigrationStep] =
    &[crate::config::migrations::MigrationStep {
        from: crate::config::migrations::LEGACY_VERSION,
        to: "v1alpha1",
        migrate: migrate_manifest_legacy_to_v1alpha1,
    }];

pub fn current_manifest_version() -> String {
    CURRENT_MANIFEST_VERSION.to_string()
}

pub fn migrate_manifest_file(
    path: &Path,
) -> anyhow::Result<Option<(crate::config::migrations::SchemaVersion, String)>> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut doc: DocumentMut = raw
        .parse()
        .with_context(|| format!("parsing {}", path.display()))?;
    let old_version = crate::config::migrations::doc_version(&doc, "role manifest")?;
    let current = crate::config::migrations::parse_version(CURRENT_MANIFEST_VERSION)?;

    if old_version > current {
        bail!(
            "role manifest is at {old_version}, this binary only understands up to {CURRENT_MANIFEST_VERSION}; upgrade jackin"
        );
    }
    if old_version == current {
        return Ok(None);
    }

    crate::config::migrations::apply_migrations(
        &mut doc,
        &old_version,
        &current,
        MANIFEST_MIGRATIONS,
        "role manifest",
    )?;
    crate::config::migrations::set_doc_version(&mut doc, CURRENT_MANIFEST_VERSION);
    crate::config::persist::atomic_write(path, &doc.to_string())?;
    Ok(Some((old_version, CURRENT_MANIFEST_VERSION.to_string())))
}

pub fn validate_manifest_version(doc: &DocumentMut, role_name: &str) -> anyhow::Result<()> {
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

#[allow(clippy::unnecessary_wraps)]
fn migrate_manifest_legacy_to_v1alpha1(doc: &mut DocumentMut) -> anyhow::Result<()> {
    crate::config::migrations::set_doc_version(doc, CURRENT_MANIFEST_VERSION);
    Ok(())
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

        assert_eq!(old.to_string(), "legacy");
        assert_eq!(new, "v1alpha1");
        assert!(out.contains(r#"version = "v1alpha1""#), "{out}");
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
}
