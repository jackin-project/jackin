use super::{AppConfig, MountConfig};
use crate::selector::ClassSelector;
use crate::workspace::expand_tilde;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MountEntry {
    Mount(MountConfig),
    Scoped(BTreeMap<String, MountConfig>),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerMounts(BTreeMap<String, MountEntry>);

impl DockerMounts {
    pub fn get(&self, key: &str) -> Option<&MountEntry> {
        self.0.get(key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut MountEntry> {
        self.0.get_mut(key)
    }

    pub fn insert(&mut self, key: String, value: MountEntry) -> Option<MountEntry> {
        self.0.insert(key, value)
    }

    pub fn remove(&mut self, key: &str) -> Option<MountEntry> {
        self.0.remove(key)
    }

    pub fn entry(
        &mut self,
        key: String,
    ) -> std::collections::btree_map::Entry<'_, String, MountEntry> {
        self.0.entry(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &MountEntry)> {
        self.0.iter()
    }
}

impl AppConfig {
    pub fn resolve_mounts(&self, selector: &ClassSelector) -> Vec<(String, MountConfig)> {
        let mut by_name: BTreeMap<String, MountConfig> = BTreeMap::new();

        // Priority order: global < wildcard < exact (later inserts override earlier)
        let scopes = [
            None,                                                    // global
            selector.namespace.as_ref().map(|ns| format!("{ns}/*")), // wildcard
            Some(selector.key()),                                    // exact
        ];

        for scope in &scopes {
            let entries = match scope {
                None => {
                    let mut map = BTreeMap::new();
                    for (name, entry) in self.docker.mounts.iter() {
                        if let MountEntry::Mount(m) = entry {
                            map.insert(name.clone(), m.clone());
                        }
                    }
                    map
                }
                Some(scope_key) => match self.docker.mounts.get(scope_key) {
                    Some(MountEntry::Scoped(scope_map)) => scope_map.clone(),
                    _ => continue,
                },
            };

            for (name, mount) in entries {
                by_name.insert(name, mount);
            }
        }

        by_name.into_iter().collect()
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
            })
            .collect();
        crate::workspace::validate_mounts(&expanded)?;
        Ok(expanded)
    }

    // pub(crate): test-only affordance for in-memory AppConfig setup in tests
    // (launch/preview.rs, workspace/resolve.rs). Production callers use ConfigEditor.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn add_mount(&mut self, name: &str, mount: MountConfig, scope: Option<&str>) {
        let scope_key = scope.unwrap_or("");
        if scope_key.is_empty() {
            self.docker
                .mounts
                .insert(name.to_string(), MountEntry::Mount(mount));
        } else {
            match self.docker.mounts.entry(scope_key.to_string()) {
                Entry::Occupied(mut entry) => {
                    if let MountEntry::Scoped(map) = entry.get_mut() {
                        map.insert(name.to_string(), mount);
                    }
                }
                Entry::Vacant(entry) => {
                    let mut map = BTreeMap::new();
                    map.insert(name.to_string(), mount);
                    entry.insert(MountEntry::Scoped(map));
                }
            }
        }
    }

    pub fn list_mounts(&self) -> Vec<(String, String, &MountConfig)> {
        let mut result = Vec::new();
        for (key, entry) in self.docker.mounts.iter() {
            match entry {
                MountEntry::Mount(m) => {
                    result.push(("(global)".to_string(), key.clone(), m));
                }
                MountEntry::Scoped(map) => {
                    for (name, m) in map {
                        result.push((key.clone(), name.clone(), m));
                    }
                }
            }
        }
        result
    }

    pub fn global_mounts(&self) -> Vec<MountConfig> {
        self.docker
            .mounts
            .iter()
            .filter_map(|(_, entry)| match entry {
                MountEntry::Mount(m) => Some(m.clone()),
                MountEntry::Scoped(_) => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selector::ClassSelector;

    #[test]
    fn deserializes_global_docker_mounts() {
        let toml_str = r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts]
gradle-cache = { src = "~/.gradle/caches", dst = "/home/claude/.gradle/caches" }
gradle-wrapper = { src = "~/.gradle/wrapper", dst = "/home/claude/.gradle/wrapper", readonly = true }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let mounts = &config.docker.mounts;
        match mounts.get("gradle-cache").unwrap() {
            MountEntry::Mount(m) => {
                assert_eq!(m.src, "~/.gradle/caches");
                assert_eq!(m.dst, "/home/claude/.gradle/caches");
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
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts]
gradle-cache = { src = "/tmp/gradle-caches", dst = "/home/claude/.gradle/caches" }

[docker.mounts."chainargos/*"]
chainargos-secrets = { src = "/tmp/chainargos-secrets", dst = "/secrets", readonly = true }

[docker.mounts."chainargos/agent-brown"]
brown-config = { src = "/tmp/chainargos-brown", dst = "/config" }

[docker.mounts."other/*"]
other-data = { src = "/tmp/other", dst = "/other" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let selector = ClassSelector::new(Some("chainargos"), "agent-brown");
        let resolved = config.resolve_mounts(&selector);
        assert_eq!(resolved.len(), 3);
        assert!(
            resolved
                .iter()
                .any(|(_, m)| m.dst == "/home/claude/.gradle/caches")
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
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts]
shared = { src = "/tmp/global", dst = "/data" }

[docker.mounts."chainargos/agent-brown"]
shared = { src = "/tmp/specific", dst = "/data" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let selector = ClassSelector::new(Some("chainargos"), "agent-brown");
        let resolved = config.resolve_mounts(&selector);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].1.src, "/tmp/specific");
    }

    #[test]
    fn resolve_mounts_returns_empty_when_no_mounts_configured() {
        let config = AppConfig::default();
        let selector = ClassSelector::new(None, "agent-smith");
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
                },
            ),
            (
                "mount-b".to_string(),
                MountConfig {
                    src,
                    dst: "/data".to_string(),
                    readonly: true,
                },
            ),
        ];
        let err = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap_err();
        assert!(err.to_string().contains("duplicate"));
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
            },
        )];
        let validated = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap();
        assert_eq!(validated[0].src, home);
    }

    #[test]
    fn resolve_mounts_matches_exact_scope_for_unscoped_selector() {
        let toml_str = r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts]
global-data = { src = "/tmp/global", dst = "/global" }

[docker.mounts."agent-smith"]
smith-data = { src = "/tmp/smith", dst = "/smith" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let resolved = config.resolve_mounts(&selector);
        assert_eq!(resolved.len(), 2);
        assert!(resolved.iter().any(|(_, m)| m.dst == "/global"));
        assert!(resolved.iter().any(|(_, m)| m.dst == "/smith"));
    }
}
