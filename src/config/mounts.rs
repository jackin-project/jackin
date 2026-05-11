use super::{AppConfig, MountConfig};
use crate::selector::RoleSelector;
use crate::workspace::expand_tilde;
use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

/// Wire format for `[[mounts]]` / `[mounts.<scope>]` entries. Lacks
/// the `isolation` field (workspace-only) and rejects unknown fields so
/// setting `isolation` here fails deserialization with the generic
/// `MountEntry` untagged-enum error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GlobalMountConfig {
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub readonly: bool,
}

impl From<GlobalMountConfig> for MountConfig {
    fn from(g: GlobalMountConfig) -> Self {
        Self {
            src: g.src,
            dst: g.dst,
            readonly: g.readonly,
            isolation: crate::isolation::MountIsolation::Shared,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MountEntry {
    Mount(GlobalMountConfig),
    Scoped(BTreeMap<String, GlobalMountConfig>),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerMounts(BTreeMap<String, MountEntry>);

impl DockerMounts {
    pub(crate) fn get(&self, key: &str) -> Option<&MountEntry> {
        self.0.get(key)
    }

    pub(crate) fn insert(&mut self, key: String, value: MountEntry) -> Option<MountEntry> {
        self.0.insert(key, value)
    }

    pub(crate) fn entry(
        &mut self,
        key: String,
    ) -> std::collections::btree_map::Entry<'_, String, MountEntry> {
        self.0.entry(key)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&String, &MountEntry)> {
        self.0.iter()
    }
}

impl AppConfig {
    /// Determine which role drives role-scoped global mounts for this
    /// workspace. Returns `Applicable` (with the resolved role + merged
    /// rows) when role is determinable; `Ambiguous` (with candidates)
    /// otherwise. Role candidates merge `allowed_roles`, `default_role`,
    /// and `last_role`; if none is set and the config has exactly one
    /// role, that one is used.
    pub fn workspace_applicable_mount_rows(
        &self,
        workspace: &crate::workspace::WorkspaceConfig,
    ) -> WorkspaceGlobalMountRows {
        let mut candidates: Vec<String> = workspace.allowed_roles.clone();
        for extra in workspace
            .default_role
            .iter()
            .chain(workspace.last_role.iter())
        {
            if !candidates.iter().any(|role| role == extra) {
                candidates.push(extra.clone());
            }
        }
        candidates.sort();
        candidates.dedup();

        let resolved_role = if candidates.len() == 1 {
            Some(candidates.remove(0))
        } else if candidates.is_empty() && self.roles.len() == 1 {
            self.roles.keys().next().cloned()
        } else {
            None
        };

        if let Some(role) = resolved_role {
            return RoleSelector::parse(&role).map_or_else(
                |_| WorkspaceGlobalMountRows::Ambiguous {
                    candidates: vec![role],
                },
                |selector| WorkspaceGlobalMountRows::Applicable {
                    role: selector.key(),
                    rows: self.resolve_mount_rows(&selector),
                },
            );
        }

        if candidates.is_empty() {
            candidates = self.roles.keys().cloned().collect();
        }
        WorkspaceGlobalMountRows::Ambiguous { candidates }
    }

    pub fn resolve_mount_rows(&self, selector: &RoleSelector) -> Vec<GlobalMountRow> {
        let mut by_name: BTreeMap<String, GlobalMountRow> = BTreeMap::new();
        let scopes = [
            None,
            selector.namespace.as_ref().map(|ns| format!("{ns}/*")),
            Some(selector.key()),
        ];

        for scope in &scopes {
            match scope {
                None => {
                    for (name, entry) in self.docker.mounts.iter() {
                        if let MountEntry::Mount(m) = entry {
                            by_name.insert(
                                name.clone(),
                                GlobalMountRow {
                                    scope: None,
                                    name: name.clone(),
                                    mount: MountConfig::from(m.clone()),
                                },
                            );
                        }
                    }
                }
                Some(scope_key) => {
                    if let Some(MountEntry::Scoped(scope_map)) = self.docker.mounts.get(scope_key) {
                        for (name, m) in scope_map {
                            by_name.insert(
                                name.clone(),
                                GlobalMountRow {
                                    scope: Some(scope_key.clone()),
                                    name: name.clone(),
                                    mount: MountConfig::from(m.clone()),
                                },
                            );
                        }
                    }
                }
            }
        }

        by_name.into_values().collect()
    }

    pub fn resolve_mounts(&self, selector: &RoleSelector) -> Vec<(String, MountConfig)> {
        self.resolve_mount_rows(selector)
            .into_iter()
            .map(|row| (row.name, row.mount))
            .collect()
    }

    pub fn expand_and_validate_named_mounts(
        mounts: &[(String, MountConfig)],
    ) -> anyhow::Result<Vec<MountConfig>> {
        let expanded: Vec<MountConfig> = mounts
            .iter()
            .map(|(_, mount)| MountConfig {
                src: expand_tilde(&mount.src),
                dst: mount.dst.clone(),
                readonly: mount.readonly,
                isolation: mount.isolation,
            })
            .collect();
        crate::workspace::validate_mounts(&expanded)?;
        Ok(expanded)
    }

    // Test-only; production writes go through ConfigEditor.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn add_mount(&mut self, name: &str, mount: MountConfig, scope: Option<&str>) {
        debug_assert!(
            matches!(mount.isolation, crate::isolation::MountIsolation::Shared),
            "global mounts cannot carry isolation"
        );
        let global = GlobalMountConfig {
            src: mount.src,
            dst: mount.dst,
            readonly: mount.readonly,
        };
        let scope_key = scope.unwrap_or("");
        if scope_key.is_empty() {
            self.docker
                .mounts
                .insert(name.to_string(), MountEntry::Mount(global));
        } else {
            match self.docker.mounts.entry(scope_key.to_string()) {
                Entry::Occupied(mut entry) => {
                    if let MountEntry::Scoped(map) = entry.get_mut() {
                        map.insert(name.to_string(), global);
                    }
                }
                Entry::Vacant(entry) => {
                    let mut map = BTreeMap::new();
                    map.insert(name.to_string(), global);
                    entry.insert(MountEntry::Scoped(map));
                }
            }
        }
    }

    pub fn list_mount_rows(&self) -> Vec<GlobalMountRow> {
        let mut result = Vec::new();
        for (key, entry) in self.docker.mounts.iter() {
            match entry {
                MountEntry::Mount(m) => result.push(GlobalMountRow {
                    scope: None,
                    name: key.clone(),
                    mount: MountConfig::from(m.clone()),
                }),
                MountEntry::Scoped(map) => {
                    for (name, m) in map {
                        result.push(GlobalMountRow {
                            scope: Some(key.clone()),
                            name: name.clone(),
                            mount: MountConfig::from(m.clone()),
                        });
                    }
                }
            }
        }
        result
    }

    pub fn validate_effective_mount_destinations(
        workspace: &crate::workspace::WorkspaceConfig,
        rows: &[GlobalMountRow],
    ) -> anyhow::Result<()> {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for mount in &workspace.mounts {
            if !seen.insert(mount.dst.as_str()) {
                anyhow::bail!("duplicate mount destination: {}", mount.dst);
            }
        }
        for row in rows {
            if !seen.insert(row.mount.dst.as_str()) {
                let scope = row.scope.as_deref().unwrap_or("global");
                anyhow::bail!(
                    "global mount destination conflicts with workspace destination: {} (from global mount {} [{}])",
                    row.mount.dst,
                    row.name,
                    scope
                );
            }
        }
        Ok(())
    }

    pub fn validate_global_mount_rows(rows: &[GlobalMountRow]) -> anyhow::Result<()> {
        let mut seen_keys: BTreeSet<(Option<&str>, &str)> = BTreeSet::new();
        for row in rows {
            if row.name.trim().is_empty() {
                anyhow::bail!("global mount name cannot be empty");
            }
            // Two rows with the same (scope, name) silently collapse on
            // wire-write because `add_mount` keys the BTreeMap by name —
            // catch it here before the editor loses one row's data.
            if !seen_keys.insert((row.scope.as_deref(), row.name.as_str())) {
                let scope = row.scope.as_deref().unwrap_or("global");
                anyhow::bail!("duplicate global mount entry: {} [{}]", row.name, scope);
            }
            let expanded = MountConfig {
                src: expand_tilde(&row.mount.src),
                dst: row.mount.dst.clone(),
                readonly: row.mount.readonly,
                isolation: row.mount.isolation,
            };
            crate::workspace::validate_mounts(std::slice::from_ref(&expanded))
                .with_context(|| format!("validating global mount {}", row.name))?;
        }
        for (idx, left) in rows.iter().enumerate() {
            for right in rows.iter().skip(idx + 1) {
                if left.name != right.name
                    && left.mount.dst == right.mount.dst
                    && scopes_overlap(left.scope.as_ref(), right.scope.as_ref())
                {
                    anyhow::bail!(
                        "duplicate global mount destination in overlapping scope: {}",
                        left.mount.dst
                    );
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalMountRow {
    pub scope: Option<String>,
    pub name: String,
    pub mount: MountConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceGlobalMountRows {
    Applicable {
        role: String,
        rows: Vec<GlobalMountRow>,
    },
    Ambiguous {
        candidates: Vec<String>,
    },
}

fn scopes_overlap(left: Option<&String>, right: Option<&String>) -> bool {
    match (left.map(String::as_str), right.map(String::as_str)) {
        (None, _) | (_, None) => true,
        (Some(a), Some(b)) if a == b => true,
        (Some(a), Some(b)) => wildcard_scope_matches(a, b) || wildcard_scope_matches(b, a),
    }
}

fn wildcard_scope_matches(wildcard: &str, concrete: &str) -> bool {
    let Some(prefix) = wildcard.strip_suffix("/*") else {
        return false;
    };
    concrete
        .strip_prefix(prefix)
        .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selector::RoleSelector;

    #[test]
    fn deserializes_global_docker_mounts() {
        let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts]
gradle-cache = { src = "~/.gradle/caches", dst = "/home/agent/.gradle/caches" }
gradle-wrapper = { src = "~/.gradle/wrapper", dst = "/home/agent/.gradle/wrapper", readonly = true }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let mounts = &config.docker.mounts;
        match mounts.get("gradle-cache").unwrap() {
            MountEntry::Mount(m) => {
                assert_eq!(m.src, "~/.gradle/caches");
                assert_eq!(m.dst, "/home/agent/.gradle/caches");
                assert!(!m.readonly);
            }
            MountEntry::Scoped(_) => panic!("expected MountEntry::Mount"),
        }
        match mounts.get("gradle-wrapper").unwrap() {
            MountEntry::Mount(m) => assert!(m.readonly),
            MountEntry::Scoped(_) => panic!("expected MountEntry::Mount"),
        }
    }

    #[test]
    fn resolve_mounts_collects_global_and_matching_scopes() {
        let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts]
gradle-cache = { src = "/tmp/gradle-caches", dst = "/home/agent/.gradle/caches" }

[docker.mounts."chainargos/*"]
chainargos-secrets = { src = "/tmp/chainargos-secrets", dst = "/secrets", readonly = true }

[docker.mounts."chainargos/agent-brown"]
brown-config = { src = "/tmp/chainargos-brown", dst = "/config" }

[docker.mounts."other/*"]
other-data = { src = "/tmp/other", dst = "/other" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let selector = RoleSelector::new(Some("chainargos"), "agent-brown");
        let resolved = config.resolve_mounts(&selector);
        assert_eq!(resolved.len(), 3);
        assert!(
            resolved
                .iter()
                .any(|(_, m)| m.dst == "/home/agent/.gradle/caches")
        );
        assert!(
            resolved
                .iter()
                .any(|(_, m)| m.dst == "/secrets" && m.readonly)
        );
        assert!(
            resolved
                .iter()
                .any(|(_, m)| m.dst == "/config" && !m.readonly)
        );
    }

    #[test]
    fn resolve_mounts_exact_overrides_global_with_same_name() {
        let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts]
shared = { src = "/tmp/global", dst = "/data" }

[docker.mounts."chainargos/agent-brown"]
shared = { src = "/tmp/specific", dst = "/data" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let selector = RoleSelector::new(Some("chainargos"), "agent-brown");
        let resolved = config.resolve_mounts(&selector);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].1.src, "/tmp/specific");
    }

    #[test]
    fn resolve_mounts_returns_empty_when_no_mounts_configured() {
        let config = AppConfig::default();
        let selector = RoleSelector::new(None, "agent-smith");
        let resolved = config.resolve_mounts(&selector);
        assert!(resolved.is_empty());
    }

    #[test]
    fn validate_mounts_rejects_missing_src() {
        let mounts = vec![(
            "test-mount".to_string(),
            MountConfig {
                src: "/nonexistent/path/that/does/not/exist".to_string(),
                dst: "/data".to_string(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        )];
        let err = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap_err();
        assert!(
            err.to_string()
                .contains("/nonexistent/path/that/does/not/exist")
        );
    }

    #[test]
    fn validate_mounts_rejects_relative_src() {
        let mounts = vec![(
            "test-mount".to_string(),
            MountConfig {
                src: ".".to_string(),
                dst: "/data".to_string(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        )];

        let err = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap_err();

        assert!(err.to_string().contains("mount source must be absolute"));
    }

    #[test]
    fn validate_mounts_rejects_relative_dst() {
        let temp = tempfile::tempdir().unwrap();
        let mounts = vec![(
            "test-mount".to_string(),
            MountConfig {
                src: temp.path().display().to_string(),
                dst: "relative/path".to_string(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        )];
        let err = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap_err();
        assert!(err.to_string().contains("absolute"));
    }

    #[test]
    fn validate_mounts_rejects_duplicate_dst() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().display().to_string();
        let mounts = vec![
            (
                "mount-a".to_string(),
                MountConfig {
                    src: src.clone(),
                    dst: "/data".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            ),
            (
                "mount-b".to_string(),
                MountConfig {
                    src,
                    dst: "/data".to_string(),
                    readonly: true,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            ),
        ];
        let err = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap_err();
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn validate_global_mount_rows_rejects_duplicate_scope_name() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().display().to_string();
        let rows = vec![
            GlobalMountRow {
                scope: None,
                name: "cache".into(),
                mount: MountConfig {
                    src: src.clone(),
                    dst: "/a".into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            },
            GlobalMountRow {
                scope: None,
                name: "cache".into(),
                mount: MountConfig {
                    src,
                    dst: "/b".into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            },
        ];

        let err = AppConfig::validate_global_mount_rows(&rows).unwrap_err();

        assert!(
            err.to_string().contains("duplicate global mount entry"),
            "expected duplicate-entry error, got: {err}"
        );
    }

    #[test]
    fn validate_global_mount_rows_rejects_empty_name() {
        let temp = tempfile::tempdir().unwrap();
        let rows = vec![GlobalMountRow {
            scope: None,
            name: "  ".into(),
            mount: MountConfig {
                src: temp.path().display().to_string(),
                dst: "/x".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        }];

        let err = AppConfig::validate_global_mount_rows(&rows).unwrap_err();

        assert!(err.to_string().contains("name cannot be empty"));
    }

    #[test]
    fn validate_global_mount_rows_rejects_overlapping_scope_duplicate_dst() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().display().to_string();
        let rows = vec![
            GlobalMountRow {
                scope: Some("chainargos/*".into()),
                name: "a".into(),
                mount: MountConfig {
                    src: src.clone(),
                    dst: "/cache".into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            },
            GlobalMountRow {
                scope: Some("chainargos/the-architect".into()),
                name: "b".into(),
                mount: MountConfig {
                    src,
                    dst: "/cache".into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            },
        ];

        let err = AppConfig::validate_global_mount_rows(&rows).unwrap_err();

        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn validate_global_mount_rows_allows_disjoint_scope_duplicate_dst() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().display().to_string();
        let rows = vec![
            GlobalMountRow {
                scope: Some("chainargos/*".into()),
                name: "a".into(),
                mount: MountConfig {
                    src: src.clone(),
                    dst: "/cache".into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            },
            GlobalMountRow {
                scope: Some("scentbird/*".into()),
                name: "b".into(),
                mount: MountConfig {
                    src,
                    dst: "/cache".into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            },
        ];

        AppConfig::validate_global_mount_rows(&rows).unwrap();
    }

    #[test]
    fn validate_global_mount_rows_allows_same_name_override_duplicate_dst() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().display().to_string();
        let rows = vec![
            GlobalMountRow {
                scope: None,
                name: "cache".into(),
                mount: MountConfig {
                    src: src.clone(),
                    dst: "/cache".into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            },
            GlobalMountRow {
                scope: Some("chainargos/*".into()),
                name: "cache".into(),
                mount: MountConfig {
                    src,
                    dst: "/cache".into(),
                    readonly: true,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            },
        ];

        AppConfig::validate_global_mount_rows(&rows).unwrap();
    }

    #[test]
    fn validate_mounts_expands_tilde_in_src() {
        let home = std::env::var("HOME").unwrap();
        let mounts = vec![(
            "home-mount".to_string(),
            MountConfig {
                src: "~".to_string(),
                dst: "/home-mount".to_string(),
                readonly: true,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        )];
        let validated = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap();
        assert_eq!(validated[0].src, home);
    }

    #[test]
    fn resolve_mounts_matches_exact_scope_for_unscoped_selector() {
        let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts]
global-data = { src = "/tmp/global", dst = "/global" }

[docker.mounts."agent-smith"]
smith-data = { src = "/tmp/smith", dst = "/smith" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let resolved = config.resolve_mounts(&selector);
        assert_eq!(resolved.len(), 2);
        assert!(resolved.iter().any(|(_, m)| m.dst == "/global"));
        assert!(resolved.iter().any(|(_, m)| m.dst == "/smith"));
    }

    #[test]
    fn global_mount_rejects_isolation_field() {
        let toml = r#"src = "/tmp/x"
dst = "/workspace/x"
isolation = "worktree"
"#;
        let err = toml::from_str::<GlobalMountConfig>(toml).unwrap_err();
        assert!(
            err.to_string().contains("isolation") || err.to_string().contains("unknown field"),
            "expected unknown-field error, got: {err}"
        );
    }

    #[test]
    fn global_mount_accepts_minimal_fields() {
        let toml = r#"src = "/tmp/x"
dst = "/workspace/x"
"#;
        let m: GlobalMountConfig = toml::from_str(toml).unwrap();
        assert_eq!(m.src, "/tmp/x");
        assert_eq!(m.dst, "/workspace/x");
        assert!(!m.readonly);
    }

    #[test]
    fn global_mount_accepts_readonly() {
        let toml = r#"src = "/tmp/x"
dst = "/workspace/x"
readonly = true
"#;
        let m: GlobalMountConfig = toml::from_str(toml).unwrap();
        assert!(m.readonly);
    }

    #[test]
    fn wire_path_rejects_isolation_on_global_mount() {
        // Production wire path: AppConfig → DockerMounts → MountEntry
        // (untagged enum) → GlobalMountConfig. Setting `isolation` on
        // a top-level `[docker.mounts]` entry must fail to deserialize.
        // Because `MountEntry` is `#[serde(untagged)]`, the message is
        // the generic "data did not match any variant" rather than
        // the cleaner "unknown field `isolation`" — see the doc
        // comment on `GlobalMountConfig` for the rationale.
        let toml = r#"
[docker.mounts]
gradle-cache = { src = "/tmp/x", dst = "/workspace/x", isolation = "worktree" }
"#;
        let err = toml::from_str::<AppConfig>(toml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("did not match any variant of untagged enum MountEntry"),
            "expected untagged-enum mismatch error, got: {msg}"
        );
    }
}
