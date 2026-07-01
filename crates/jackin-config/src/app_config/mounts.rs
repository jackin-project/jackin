//! `AppConfig` mount resolution impl blocks and display helpers.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context as _;
use jackin_core::{MountIsolation, RoleSelector};

use super::AppConfig;
use crate::paths::expand_tilde;
use crate::schema::validate_mounts;
use crate::schema::{GlobalMountConfig, MountConfig, MountEntry};

/// A resolved global mount entry for display and validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalMountRow {
    pub scope: Option<String>,
    pub name: String,
    pub mount: MountConfig,
}

/// Result of resolving applicable global mounts for a workspace.
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

impl AppConfig {
    /// Determine which role drives role-scoped global mounts for this
    /// workspace. Returns `Applicable` (with the resolved role + merged
    /// rows) when role is determinable; `Ambiguous` (with candidates)
    /// otherwise. Role candidates merge `allowed_roles`, `default_role`,
    /// and `last_role`; if none is set and the config has exactly one
    /// role, that one is used.
    pub fn workspace_applicable_mount_rows(
        &self,
        workspace: &crate::schema::WorkspaceConfig,
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

    #[allow(
        clippy::excessive_nesting,
        reason = "Mount-row resolution: per-mount, per-scope (None / Scope / \
                  Multi-Scope), and per-mount-type (Mount / WorkspaceRef) branches \
                  nested to apply union-merge semantics. Extracting per-scope \
                  helpers would re-pass mutable by_name + selector borrows across \
                  fn boundaries and obscure the per-scope merge logic."
    )]
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
        validate_mounts(&expanded)?;
        Ok(expanded)
    }

    // Test-only; production writes go through ConfigEditor.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn add_mount(&mut self, name: &str, mount: MountConfig, scope: Option<&str>) {
        debug_assert!(
            matches!(mount.isolation, MountIsolation::Shared),
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
                .insert(name.to_owned(), MountEntry::Mount(global));
        } else {
            match self.docker.mounts.entry(scope_key.to_owned()) {
                Entry::Occupied(mut entry) => {
                    if let MountEntry::Scoped(map) = entry.get_mut() {
                        map.insert(name.to_owned(), global);
                    }
                }
                Entry::Vacant(entry) => {
                    let mut map = BTreeMap::new();
                    map.insert(name.to_owned(), global);
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
        workspace: &crate::schema::WorkspaceConfig,
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
            if !matches!(row.mount.isolation, MountIsolation::Shared) {
                anyhow::bail!(
                    "global mount {} cannot use isolation {}; global mounts are always shared",
                    row.name,
                    row.mount.isolation.as_str()
                );
            }
            let expanded = MountConfig {
                src: expand_tilde(&row.mount.src),
                dst: row.mount.dst.clone(),
                readonly: row.mount.readonly,
                isolation: row.mount.isolation,
            };
            validate_mounts(std::slice::from_ref(&expanded))
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
mod tests;
