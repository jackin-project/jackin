use std::path::Path;

use anyhow::{Context, bail};
use toml_edit::DocumentMut;

pub const CURRENT_CONFIG_VERSION: &str = "v1alpha1";
pub const CURRENT_WORKSPACE_VERSION: &str = "v1alpha1";
pub const LEGACY_VERSION: &str = "legacy";

pub type Migration = fn(&mut DocumentMut) -> anyhow::Result<()>;

#[derive(Clone, Copy)]
pub struct MigrationStep {
    pub from: &'static str,
    pub to: &'static str,
    pub migrate: Migration,
}

const CONFIG_MIGRATIONS: &[MigrationStep] = &[MigrationStep {
    from: LEGACY_VERSION,
    to: "v1alpha1",
    migrate: migrate_config_legacy_to_v1alpha1,
}];
const WORKSPACE_MIGRATIONS: &[MigrationStep] = &[MigrationStep {
    from: LEGACY_VERSION,
    to: "v1alpha1",
    migrate: migrate_workspace_legacy_to_v1alpha1,
}];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaVersion {
    Legacy,
    Kubernetes {
        major: u32,
        stability: Stability,
        sequence: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stability {
    Alpha,
    Beta,
    Stable,
}

impl Ord for SchemaVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use SchemaVersion::{Kubernetes, Legacy};
        match (self, other) {
            (Legacy, Legacy) => std::cmp::Ordering::Equal,
            (Legacy, Kubernetes { .. }) => std::cmp::Ordering::Less,
            (Kubernetes { .. }, Legacy) => std::cmp::Ordering::Greater,
            (
                Kubernetes {
                    major,
                    stability,
                    sequence,
                },
                Kubernetes {
                    major: other_major,
                    stability: other_stability,
                    sequence: other_sequence,
                },
            ) => (*major, *stability, *sequence).cmp(&(
                *other_major,
                *other_stability,
                *other_sequence,
            )),
        }
    }
}

impl PartialOrd for SchemaVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Legacy => write!(f, "{LEGACY_VERSION}"),
            Self::Kubernetes {
                major,
                stability,
                sequence,
            } => match stability {
                Stability::Stable => write!(f, "v{major}"),
                Stability::Beta => write!(f, "v{major}beta{sequence}"),
                Stability::Alpha => write!(f, "v{major}alpha{sequence}"),
            },
        }
    }
}

pub fn current_config_version() -> String {
    CURRENT_CONFIG_VERSION.to_string()
}

pub fn current_workspace_version() -> String {
    CURRENT_WORKSPACE_VERSION.to_string()
}

pub fn migrate_config_file_if_needed(path: &Path) -> anyhow::Result<bool> {
    migrate_file_if_needed(path, "config", CURRENT_CONFIG_VERSION, CONFIG_MIGRATIONS)
}

pub fn migrate_workspace_file_if_needed(path: &Path) -> anyhow::Result<bool> {
    migrate_file_if_needed(
        path,
        "workspace config",
        CURRENT_WORKSPACE_VERSION,
        WORKSPACE_MIGRATIONS,
    )
}

fn migrate_file_if_needed(
    path: &Path,
    label: &str,
    current_raw: &str,
    migrations: &[MigrationStep],
) -> anyhow::Result<bool> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut doc: DocumentMut = raw
        .parse()
        .with_context(|| format!("parsing {}", path.display()))?;
    let old_version = doc_version(&doc, label)?;
    let current = parse_version(current_raw)?;

    if old_version > current {
        bail!(
            "{label} is at {old_version}, this binary only understands up to {current_raw}; upgrade jackin"
        );
    }
    if old_version == current {
        return Ok(false);
    }

    apply_migrations(&mut doc, &old_version, &current, migrations, label)?;
    set_doc_version(&mut doc, current_raw);
    crate::config::persist::atomic_write(path, &doc.to_string())?;
    eprintln!("[jackin] {label} migrated {old_version} -> {current_raw}");
    Ok(true)
}

pub fn apply_migrations(
    doc: &mut DocumentMut,
    old_version: &SchemaVersion,
    current_version: &SchemaVersion,
    migrations: &[MigrationStep],
    label: &str,
) -> anyhow::Result<()> {
    let mut cursor = old_version.clone();
    while &cursor < current_version {
        let Some(step) = find_step(&cursor, migrations)? else {
            bail!(
                "{label} is at {old_version}, but this binary no longer includes a migration path to {current_version}; upgrade through an older jackin first"
            );
        };
        let next = parse_registry_version(step.to)?;
        if next <= cursor {
            bail!(
                "{label} migration registry is invalid: step {} -> {} does not move forward",
                step.from,
                step.to
            );
        }
        (step.migrate)(doc)?;
        cursor = next;
    }
    if &cursor != current_version {
        bail!("{label} migration registry stopped at {cursor}, expected {current_version}");
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

pub fn parse_registry_version(version: &str) -> anyhow::Result<SchemaVersion> {
    if version == LEGACY_VERSION {
        return Ok(SchemaVersion::Legacy);
    }
    parse_version(version)
}

pub fn doc_version(doc: &DocumentMut, label: &str) -> anyhow::Result<SchemaVersion> {
    let Some(item) = doc.get("version") else {
        return Ok(SchemaVersion::Legacy);
    };
    let Some(version) = item.as_str() else {
        bail!("{label} version must be a string");
    };
    parse_version(version).with_context(|| format!("{label} version is invalid"))
}

pub fn parse_version(version: &str) -> anyhow::Result<SchemaVersion> {
    let rest = version
        .strip_prefix('v')
        .ok_or_else(|| anyhow::anyhow!("version must start with `v`"))?;
    let (major_raw, suffix) =
        split_first_nondigit(rest).ok_or_else(|| anyhow::anyhow!("missing major version"))?;
    let major: u32 = major_raw
        .parse()
        .with_context(|| format!("invalid major version {major_raw:?}"))?;
    if major == 0 {
        bail!("major version must be greater than zero");
    }
    if suffix.is_empty() {
        return Ok(SchemaVersion::Kubernetes {
            major,
            stability: Stability::Stable,
            sequence: 0,
        });
    }
    for (prefix, stability) in [("alpha", Stability::Alpha), ("beta", Stability::Beta)] {
        if let Some(sequence_raw) = suffix.strip_prefix(prefix) {
            if sequence_raw.is_empty() {
                bail!("{prefix} version must include a sequence number");
            }
            let sequence: u32 = sequence_raw
                .parse()
                .with_context(|| format!("invalid {prefix} sequence {sequence_raw:?}"))?;
            if sequence == 0 {
                bail!("{prefix} sequence must be greater than zero");
            }
            return Ok(SchemaVersion::Kubernetes {
                major,
                stability,
                sequence,
            });
        }
    }
    bail!("version must look like v1, v1beta1, or v1alpha1")
}

fn split_first_nondigit(s: &str) -> Option<(&str, &str)> {
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if split == 0 {
        return None;
    }
    Some(s.split_at(split))
}

pub fn set_doc_version(doc: &mut DocumentMut, version: &str) {
    doc["version"] = toml_edit::value(version);
}

#[allow(clippy::unnecessary_wraps)]
fn migrate_config_legacy_to_v1alpha1(doc: &mut DocumentMut) -> anyhow::Result<()> {
    set_doc_version(doc, CURRENT_CONFIG_VERSION);
    Ok(())
}

#[allow(clippy::unnecessary_wraps)]
fn migrate_workspace_legacy_to_v1alpha1(doc: &mut DocumentMut) -> anyhow::Result<()> {
    set_doc_version(doc, CURRENT_WORKSPACE_VERSION);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn migrates_missing_config_version_to_current() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("config.toml");
        std::fs::write(
            &path,
            "# keep me\n\n[roles.agent-smith]\ngit = \"https://example.test/role.git\"\n",
        )
        .unwrap();

        assert!(migrate_config_file_if_needed(&path).unwrap());
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(out.contains(r#"version = "v1alpha1""#), "{out}");
        assert!(out.contains("# keep me"), "{out}");
    }

    #[test]
    fn migrates_missing_workspace_version_to_current() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("prod.toml");
        std::fs::write(&path, "# keep me\nworkdir = \"/workspace/prod\"\n").unwrap();

        assert!(migrate_workspace_file_if_needed(&path).unwrap());
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(out.contains(r#"version = "v1alpha1""#), "{out}");
        assert!(out.contains("# keep me"), "{out}");
    }

    #[test]
    fn rejects_newer_config_version() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("config.toml");
        std::fs::write(&path, r#"version = "v2alpha1""#).unwrap();

        let err = migrate_config_file_if_needed(&path).unwrap_err();
        assert!(err.to_string().contains("only understands up to v1alpha1"));
    }

    #[test]
    fn rejects_invalid_version() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("config.toml");
        std::fs::write(&path, r#"version = "0.1.0""#).unwrap();

        let err = migrate_config_file_if_needed(&path).unwrap_err();
        assert!(err.to_string().contains("version is invalid"));
    }

    #[test]
    fn kubernetes_versions_sort_by_stability_and_sequence() {
        assert!(parse_version("v1alpha1").unwrap() < parse_version("v1alpha2").unwrap());
        assert!(parse_version("v1alpha2").unwrap() < parse_version("v1beta1").unwrap());
        assert!(parse_version("v1beta1").unwrap() < parse_version("v1").unwrap());
        assert!(parse_version("v1").unwrap() < parse_version("v2alpha1").unwrap());
    }

    #[test]
    fn rejects_when_migration_path_was_removed() {
        let old = SchemaVersion::Legacy;
        let current = parse_version("v1alpha1").unwrap();
        let mut doc = DocumentMut::new();

        let err = apply_migrations(&mut doc, &old, &current, &[], "config").unwrap_err();

        assert!(
            err.to_string()
                .contains("no longer includes a migration path")
        );
    }

    #[test]
    fn rejects_when_middle_migration_path_was_removed() {
        #[allow(clippy::unnecessary_wraps)]
        fn alpha2_to_alpha3(doc: &mut DocumentMut) -> anyhow::Result<()> {
            set_doc_version(doc, "v1alpha3");
            Ok(())
        }

        let old = parse_version("v1alpha1").unwrap();
        let current = parse_version("v1alpha3").unwrap();
        let migrations = [MigrationStep {
            from: "v1alpha2",
            to: "v1alpha3",
            migrate: alpha2_to_alpha3,
        }];
        let mut doc = DocumentMut::new();

        let err = apply_migrations(&mut doc, &old, &current, &migrations, "config").unwrap_err();

        assert!(
            err.to_string()
                .contains("no longer includes a migration path")
        );
    }
}
