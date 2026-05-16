use std::num::NonZeroU32;
use std::path::Path;

use anyhow::{Context, bail};
use toml_edit::DocumentMut;

pub const CURRENT_CONFIG_VERSION: &str = "v1alpha5";
pub const CURRENT_WORKSPACE_VERSION: &str = "v1alpha4";
pub const LEGACY_VERSION: &str = "legacy";

pub type Migration = fn(&mut DocumentMut) -> anyhow::Result<()>;

#[derive(Clone, Copy)]
pub struct MigrationStep {
    pub from: &'static str,
    pub to: &'static str,
    pub migrate: Migration,
}

const CONFIG_MIGRATIONS: &[MigrationStep] = &[
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
        to: CURRENT_CONFIG_VERSION,
        migrate: noop_migration,
    },
];
const WORKSPACE_MIGRATIONS: &[MigrationStep] = &[
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
        to: CURRENT_WORKSPACE_VERSION,
        migrate: noop_migration,
    },
];

/// Schema version of a jackin-owned configuration file.
///
/// Variant order is load-bearing: `Legacy < Kubernetes(_)` lets the migration
/// walker treat a missing `version` field as the lowest possible value
/// without sprinkling `Option`-handling across call sites.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SchemaVersion {
    /// File predates versioning (no `version` key in the document).
    Legacy,
    Kubernetes(KubernetesVersion),
}

/// Field order is load-bearing: derived `Ord` compares `major` first, then
/// `channel`, which gives the expected `v1alpha1 < v1beta1 < v1 < v2alpha1`
/// ordering.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KubernetesVersion {
    major: NonZeroU32,
    channel: Channel,
}

/// Kubernetes channel maturity. Variant declaration order is load-bearing —
/// `Alpha < Beta < Stable` is required by the derived `Ord` and pinned by
/// `channel_order_is_alpha_beta_stable` in this module's tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Channel {
    Alpha(NonZeroU32),
    Beta(NonZeroU32),
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

pub fn current_config_version() -> String {
    CURRENT_CONFIG_VERSION.to_string()
}

pub fn current_workspace_version() -> String {
    CURRENT_WORKSPACE_VERSION.to_string()
}

pub fn migrate_config_file_if_needed(path: &Path) -> anyhow::Result<bool> {
    Ok(
        migrate_file_if_needed(path, "config", CURRENT_CONFIG_VERSION, CONFIG_MIGRATIONS)?
            .is_some(),
    )
}

pub fn migrate_workspace_file_if_needed(path: &Path) -> anyhow::Result<bool> {
    Ok(migrate_file_if_needed(
        path,
        "workspace config",
        CURRENT_WORKSPACE_VERSION,
        WORKSPACE_MIGRATIONS,
    )?
    .is_some())
}

/// Read `path`, run any pending migrations, write the result back atomically.
///
/// Returns `Some(old_version)` when a migration ran, `None` when the file
/// was already at `current_raw`. Also writes
/// `[jackin] {label} migrated {old} -> {current_raw}` to stderr. Wrappers
/// (e.g. `manifest::migrations::migrate_manifest_file`) project the
/// `SchemaVersion` into display strings for callers that need to print
/// both ends.
pub fn migrate_file_if_needed(
    path: &Path,
    label: &str,
    current_raw: &str,
    migrations: &[MigrationStep],
) -> anyhow::Result<Option<SchemaVersion>> {
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
        return Ok(None);
    }

    apply_migrations(&mut doc, &old_version, &current, migrations, label)?;
    crate::config::persist::atomic_write(path, &doc.to_string())
        .with_context(|| format!("writing migrated {label} to {}", path.display()))?;
    eprintln!("[jackin] {label} migrated {old_version} -> {current_raw}");
    Ok(Some(old_version))
}

/// Walk the registry from `old_version` up to `current_version`, mutating
/// `doc` in place. After each step, the framework stamps `step.to` into the
/// document — migration functions transform content; they must not write
/// `version` themselves.
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
        (step.migrate)(doc)
            .with_context(|| format!("running {label} migration {} -> {}", step.from, step.to))?;
        set_doc_version(doc, step.to);
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

// Hand-rolled parser for Kubernetes-style versions (`v1`, `v1betaN`,
// `v1alphaN`). No canonical Rust crate fits — `semver` parses `MAJOR.MINOR.PATCH`,
// `kube`/`k8s_openapi` are heavy and pull a runtime, and the grammar here is
// small enough that adding a dependency is overkill (per AGENTS.md
// "Prefer libraries over hand-rolled parsers" carve-out).
pub fn parse_version(version: &str) -> anyhow::Result<SchemaVersion> {
    let rest = version
        .strip_prefix('v')
        .ok_or_else(|| anyhow::anyhow!("version must start with `v`"))?;
    let (major_raw, suffix) =
        split_first_nondigit(rest).ok_or_else(|| anyhow::anyhow!("missing major version"))?;
    let major = parse_canonical_u32(major_raw, "major version")?;
    let major = NonZeroU32::new(major)
        .ok_or_else(|| anyhow::anyhow!("major version must be greater than zero"))?;

    let channel = if suffix.is_empty() {
        Channel::Stable
    } else if let Some(seq_raw) = suffix.strip_prefix("alpha") {
        Channel::Alpha(parse_sequence("alpha", seq_raw)?)
    } else if let Some(seq_raw) = suffix.strip_prefix("beta") {
        Channel::Beta(parse_sequence("beta", seq_raw)?)
    } else {
        bail!("version must look like v1, v1beta1, or v1alpha1");
    };

    Ok(SchemaVersion::Kubernetes(KubernetesVersion {
        major,
        channel,
    }))
}

fn parse_sequence(prefix: &str, raw: &str) -> anyhow::Result<NonZeroU32> {
    if raw.is_empty() {
        bail!("{prefix} version must include a sequence number");
    }
    let value = parse_canonical_u32(raw, &format!("{prefix} sequence"))?;
    NonZeroU32::new(value)
        .ok_or_else(|| anyhow::anyhow!("{prefix} sequence must be greater than zero"))
}

// Reject leading zeros so version strings round-trip canonically. Without
// this, `v01alpha01` would parse equal to `v1alpha1` but live on disk as the
// non-canonical form forever (the file is only rewritten when migrating).
fn parse_canonical_u32(raw: &str, label: &str) -> anyhow::Result<u32> {
    if raw.len() > 1 && raw.starts_with('0') {
        bail!("{label} must not have leading zeros");
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
#[allow(clippy::unnecessary_wraps)]
pub const fn noop_migration(_doc: &mut DocumentMut) -> anyhow::Result<()> {
    Ok(())
}

/// Walk a `MigrationStep` slice from `Legacy` to `current_raw` and assert
/// the chain is shape-correct. Catches typos in `from` / `to`, missing
/// middle steps, backward steps, cycles, and duplicate `from` values
/// (which would silently fork the walker). Production registries call
/// this from a `#[test]` so a registry mistake fails CI rather than
/// surfacing on an operator's machine.
#[cfg(test)]
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
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn nz(n: u32) -> NonZeroU32 {
        NonZeroU32::new(n).expect("non-zero literal in test")
    }

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
        let parsed: toml::Value = toml::from_str(&out).unwrap();
        assert_eq!(parsed["version"].as_str().unwrap(), "v1alpha5");
        assert!(out.contains("# keep me"), "{out}");
    }

    #[test]
    fn migrates_missing_workspace_version_to_current() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("prod.toml");
        std::fs::write(&path, "# keep me\nworkdir = \"/workspace/prod\"\n").unwrap();

        assert!(migrate_workspace_file_if_needed(&path).unwrap());
        let out = std::fs::read_to_string(&path).unwrap();
        let parsed: toml::Value = toml::from_str(&out).unwrap();
        assert_eq!(parsed["version"].as_str().unwrap(), "v1alpha4");
        assert!(out.contains("# keep me"), "{out}");
    }

    #[test]
    fn already_current_workspace_is_a_no_op() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("prod.toml");
        std::fs::write(
            &path,
            "version = \"v1alpha4\"\nworkdir = \"/workspace/prod\"\n",
        )
        .unwrap();

        assert!(!migrate_workspace_file_if_needed(&path).unwrap());
    }

    #[test]
    fn rejects_newer_config_version() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("config.toml");
        std::fs::write(&path, r#"version = "v2alpha1""#).unwrap();

        let err = migrate_config_file_if_needed(&path).unwrap_err();
        assert!(err.to_string().contains("only understands up to v1alpha5"));
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
    fn rejects_non_string_version_field() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("config.toml");
        std::fs::write(&path, "version = 42\n").unwrap();

        let err = migrate_config_file_if_needed(&path).unwrap_err();
        assert!(
            err.to_string().contains("version must be a string"),
            "{err}"
        );
    }

    #[test]
    fn parse_version_rejects_zero_major() {
        let err = parse_version("v0").unwrap_err();
        assert!(
            err.to_string()
                .contains("major version must be greater than zero"),
            "{err}"
        );
    }

    #[test]
    fn parse_version_rejects_zero_alpha_sequence() {
        let err = parse_version("v1alpha0").unwrap_err();
        assert!(
            err.to_string()
                .contains("alpha sequence must be greater than zero"),
            "{err}"
        );
    }

    #[test]
    fn parse_version_rejects_alpha_without_sequence() {
        let err = parse_version("v1alpha").unwrap_err();
        assert!(
            err.to_string()
                .contains("alpha version must include a sequence number"),
            "{err}"
        );
    }

    #[test]
    fn parse_version_rejects_beta_without_sequence() {
        let err = parse_version("v1beta").unwrap_err();
        assert!(
            err.to_string()
                .contains("beta version must include a sequence number"),
            "{err}"
        );
    }

    #[test]
    fn parse_version_rejects_unknown_channel() {
        let err = parse_version("v1gamma1").unwrap_err();
        assert!(
            err.to_string()
                .contains("must look like v1, v1beta1, or v1alpha1"),
            "{err}"
        );
    }

    #[test]
    fn parse_version_rejects_missing_v_prefix() {
        let err = parse_version("1alpha1").unwrap_err();
        assert!(err.to_string().contains("must start with `v`"), "{err}");
    }

    #[test]
    fn parse_version_rejects_no_digits() {
        let err = parse_version("vabc").unwrap_err();
        assert!(err.to_string().contains("missing major version"), "{err}");
    }

    #[test]
    fn parse_version_rejects_leading_zero_major() {
        let err = parse_version("v01alpha1").unwrap_err();
        assert!(
            err.to_string()
                .contains("major version must not have leading zeros"),
            "{err}"
        );
    }

    #[test]
    fn parse_version_rejects_leading_zero_alpha_sequence() {
        let err = parse_version("v1alpha01").unwrap_err();
        assert!(
            err.to_string()
                .contains("alpha sequence must not have leading zeros"),
            "{err}"
        );
    }

    #[test]
    fn parse_version_rejects_u32_overflow() {
        let err = parse_version("v9999999999").unwrap_err();
        assert!(err.to_string().contains("invalid major version"), "{err}");
    }

    #[test]
    fn parse_version_accepts_canonical_forms() {
        assert_eq!(
            parse_version("v1").unwrap(),
            SchemaVersion::Kubernetes(KubernetesVersion {
                major: nz(1),
                channel: Channel::Stable,
            })
        );
        assert_eq!(
            parse_version("v1alpha1").unwrap(),
            SchemaVersion::Kubernetes(KubernetesVersion {
                major: nz(1),
                channel: Channel::Alpha(nz(1)),
            })
        );
        assert_eq!(
            parse_version("v2beta3").unwrap(),
            SchemaVersion::Kubernetes(KubernetesVersion {
                major: nz(2),
                channel: Channel::Beta(nz(3)),
            })
        );
    }

    #[test]
    fn channel_order_is_alpha_beta_stable() {
        assert!(Channel::Alpha(nz(1)) < Channel::Beta(nz(1)));
        assert!(Channel::Beta(nz(1)) < Channel::Stable);
        // Within a channel, sequence orders.
        assert!(Channel::Alpha(nz(1)) < Channel::Alpha(nz(2)));
        // Cross-channel beats sequence: a high alpha is still less than any
        // beta.
        assert!(Channel::Alpha(nz(99)) < Channel::Beta(nz(1)));
    }

    fn assert_registry_reaches(migrations: &[MigrationStep], current_raw: &str) {
        super::assert_registry_chain(migrations, current_raw);
    }

    #[test]
    fn config_migrations_chain_reaches_current() {
        assert_registry_reaches(CONFIG_MIGRATIONS, CURRENT_CONFIG_VERSION);
    }

    #[test]
    fn parse_registry_version_handles_legacy_sentinel() {
        assert_eq!(
            parse_registry_version("legacy").unwrap(),
            SchemaVersion::Legacy
        );
        // Non-sentinel strings delegate to parse_version.
        assert!(parse_registry_version("legacyfoo").is_err());
        assert_eq!(
            parse_registry_version("v1alpha1").unwrap(),
            parse_version("v1alpha1").unwrap()
        );
    }

    #[test]
    fn workspace_migrations_chain_reaches_current() {
        assert_registry_reaches(WORKSPACE_MIGRATIONS, CURRENT_WORKSPACE_VERSION);
    }

    #[test]
    fn kubernetes_versions_sort_by_stability_and_sequence() {
        assert!(parse_version("v1alpha1").unwrap() < parse_version("v1alpha2").unwrap());
        assert!(parse_version("v1alpha2").unwrap() < parse_version("v1beta1").unwrap());
        assert!(parse_version("v1beta1").unwrap() < parse_version("v1").unwrap());
        assert!(parse_version("v1").unwrap() < parse_version("v2alpha1").unwrap());
    }

    #[test]
    fn legacy_orders_below_every_kubernetes_version() {
        assert!(SchemaVersion::Legacy < parse_version("v1alpha1").unwrap());
        assert!(SchemaVersion::Legacy < parse_version("v1").unwrap());
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
        let old = parse_version("v1alpha1").unwrap();
        let current = parse_version("v1alpha4").unwrap();
        // No content mutation: framework stamps `step.to` after each step.
        let migrations = [MigrationStep {
            from: "v1alpha2",
            to: "v1alpha3",
            migrate: noop_migration,
        }];
        let mut doc = DocumentMut::new();

        let err = apply_migrations(&mut doc, &old, &current, &migrations, "config").unwrap_err();

        assert!(
            err.to_string()
                .contains("no longer includes a migration path")
        );
    }

    #[test]
    fn rejects_backward_step_in_registry() {
        let old = SchemaVersion::Legacy;
        let current = parse_version("v1alpha1").unwrap();
        let migrations = [MigrationStep {
            from: LEGACY_VERSION,
            to: LEGACY_VERSION,
            migrate: noop_migration,
        }];
        let mut doc = DocumentMut::new();

        let err = apply_migrations(&mut doc, &old, &current, &migrations, "config").unwrap_err();

        assert!(
            err.to_string()
                .contains("registry is invalid: step legacy -> legacy does not move forward"),
            "{err}"
        );
    }

    #[test]
    fn rejects_when_chain_overshoots_current() {
        let old = SchemaVersion::Legacy;
        let current = parse_version("v1alpha1").unwrap();
        let migrations = [MigrationStep {
            from: LEGACY_VERSION,
            to: "v1alpha2",
            migrate: noop_migration,
        }];
        let mut doc = DocumentMut::new();

        let err = apply_migrations(&mut doc, &old, &current, &migrations, "config").unwrap_err();

        assert!(
            err.to_string()
                .contains("registry stopped at v1alpha2, expected v1alpha1"),
            "{err}"
        );
    }

    // Migration fn pointers must return Result to match the
    // `Migration` type alias even when the test bodies always succeed.
    #[allow(clippy::unnecessary_wraps)]
    fn alpha1_to_alpha2(doc: &mut DocumentMut) -> anyhow::Result<()> {
        doc["alpha1_to_alpha2"] = toml_edit::value(true);
        Ok(())
    }
    #[allow(clippy::unnecessary_wraps)]
    fn alpha2_to_alpha3(doc: &mut DocumentMut) -> anyhow::Result<()> {
        doc["alpha2_to_alpha3"] = toml_edit::value(true);
        Ok(())
    }

    #[test]
    fn applies_multi_step_chain_in_order() {
        // Each step appends a marker key so the final doc captures the
        // execution order — a regression that double-applies, skips, or
        // reorders steps changes the marker.

        let old = parse_version("v1alpha1").unwrap();
        let current = parse_version("v1alpha4").unwrap();
        let migrations = [
            MigrationStep {
                from: "v1alpha1",
                to: "v1alpha2",
                migrate: alpha1_to_alpha2,
            },
            MigrationStep {
                from: "v1alpha2",
                to: "v1alpha3",
                migrate: alpha2_to_alpha3,
            },
            MigrationStep {
                from: "v1alpha3",
                to: "v1alpha4",
                migrate: noop_migration,
            },
        ];
        let mut doc = DocumentMut::new();

        apply_migrations(&mut doc, &old, &current, &migrations, "config").unwrap();

        assert_eq!(doc["alpha1_to_alpha2"].as_bool(), Some(true));
        assert_eq!(doc["alpha2_to_alpha3"].as_bool(), Some(true));
        assert_eq!(doc["version"].as_str(), Some("v1alpha4"));
    }

    #[test]
    fn applies_multi_step_chain_in_order_to_alpha3() {
        let old = parse_version("v1alpha1").unwrap();
        let current = parse_version("v1alpha3").unwrap();
        let migrations = [
            MigrationStep {
                from: "v1alpha1",
                to: "v1alpha2",
                migrate: alpha1_to_alpha2,
            },
            MigrationStep {
                from: "v1alpha2",
                to: "v1alpha3",
                migrate: alpha2_to_alpha3,
            },
        ];
        let mut doc = DocumentMut::new();

        apply_migrations(&mut doc, &old, &current, &migrations, "config").unwrap();

        assert_eq!(doc["alpha1_to_alpha2"].as_bool(), Some(true));
        assert_eq!(doc["alpha2_to_alpha3"].as_bool(), Some(true));
        assert_eq!(doc["version"].as_str(), Some("v1alpha3"));
    }

    #[test]
    fn version_field_is_migrated_to_first_line() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("prod.toml");
        std::fs::write(&path, "workdir = \"/workspace/prod\"\n# trailing comment\n").unwrap();

        assert!(migrate_workspace_file_if_needed(&path).unwrap());
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(out.starts_with("version = \"v1alpha4\""), "{out}");
        assert!(out.contains("workdir = \"/workspace/prod\""), "{out}");
        assert!(out.contains("# trailing comment"), "{out}");
    }
}
