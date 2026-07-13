//! `EnvLayer` enum and `merge_layers` function for the four-layer env merge.

use std::collections::BTreeMap;

use jackin_core::EnvValue;

/// Source layer of an env value.
///
/// Attached to error messages and launch diagnostics so the operator can
/// locate the offending entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EnvLayer {
    Global,
    Role(String),
    Workspace(String),
    WorkspaceRole { workspace: String, role: String },
}

impl std::fmt::Display for EnvLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Global => write!(f, "global [env]"),
            Self::Role(name) => write!(f, "role {name:?} [env]"),
            Self::Workspace(name) => write!(f, "workspace {name:?} [env]"),
            Self::WorkspaceRole { workspace, role } => {
                write!(f, "workspace {workspace:?} → role {role:?} [env]")
            }
        }
    }
}

/// Later-wins merge: global → role → workspace → workspace-role.
#[allow(dead_code, reason = "demoted with env_layer module (plan 039)")]
pub(crate) fn merge_layers(
    global: &BTreeMap<String, EnvValue>,
    role: &BTreeMap<String, EnvValue>,
    workspace: &BTreeMap<String, EnvValue>,
    workspace_role: &BTreeMap<String, EnvValue>,
) -> BTreeMap<String, EnvValue> {
    let mut merged = BTreeMap::new();
    for layer in [global, role, workspace, workspace_role] {
        for (k, v) in layer {
            merged.insert(k.clone(), v.clone());
        }
    }
    merged
}
