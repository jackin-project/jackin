// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ProfileSource` and profile resolution.

use super::{
    DockerGrants, DockerSecurityProfile, EffectiveGrants, GrantValidationError, NetworkGrant,
    apply_grants, profile_base_grants,
};

/// Whether the resolved profile satisfies a role's `min_profile` floor: at least
/// as capable as `min` in the ascending-capability [`DockerSecurityProfile`] Ord.
///
/// Note the direction: a floor of `hardened` rejects `locked` (locked is *more*
/// restrictive, *less* capable) and accepts `standard`/`compat`.
pub fn profile_meets_floor(resolved: DockerSecurityProfile, min: DockerSecurityProfile) -> bool {
    resolved >= min
}

// ── Profile resolution ───────────────────────────────────────────────────────

/// Source that produced the active Docker security profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSource {
    Cli,
    Workspace,
    Config,
    Default,
}

impl std::fmt::Display for ProfileSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cli => write!(f, "cli"),
            Self::Workspace => write!(f, "workspace"),
            Self::Config => write!(f, "config"),
            Self::Default => write!(f, "default"),
        }
    }
}

/// Resolve the effective Docker security profile and its source.
///
/// Precedence (highest to lowest):
/// 1. CLI `--docker-profile` override
/// 2. Workspace `[docker] profile` override
/// 3. Global `[docker] profile` from `config.toml`
/// 4. Compiled-in default (`Compat` until the WP6 flip; WP-SUDO removed the
///    sudo-audit blocker)
pub fn resolve_profile(
    cli_override: Option<DockerSecurityProfile>,
    workspace_profile: Option<DockerSecurityProfile>,
    config_default: Option<DockerSecurityProfile>,
) -> (DockerSecurityProfile, ProfileSource) {
    if let Some(p) = cli_override {
        return (p, ProfileSource::Cli);
    }
    if let Some(p) = workspace_profile {
        return (p, ProfileSource::Workspace);
    }
    if let Some(p) = config_default {
        return (p, ProfileSource::Config);
    }
    (DockerSecurityProfile::default(), ProfileSource::Default)
}

/// Validate a fully-resolved [`EffectiveGrants`] for cross-source invariants
/// that per-source [`validate_grants`] cannot catch.
///
/// Returns a list of all violations. An empty list means the grants are valid.
pub fn validate_effective_grants(grants: &EffectiveGrants) -> Vec<GrantValidationError> {
    let mut errors = Vec::new();
    // user="root" + sudo=true can emerge from cross-source merging (e.g.
    // config sets sudo=true, workspace sets user="root") even though per-source
    // validation on each DockerGrants would not catch the combination.
    if grants.user == "root" && grants.sudo {
        errors.push(GrantValidationError::RootAndSudo);
    }
    // memory_reservation > memory can emerge from cross-source merging: each
    // source passes per-source validation independently, but after apply_grants
    // raises each field to its maximum the merged result may violate the constraint.
    if let (Some(res), Some(mem)) = (grants.memory_reservation_bytes, grants.memory_bytes)
        && res > mem
    {
        errors.push(GrantValidationError::MemoryReservationExceedsMemory {
            reservation: res,
            memory: mem,
        });
    }
    errors
}

/// Resolve the effective profile and apply grants, returning the merged
/// [`EffectiveGrants`] for a launch.
///
/// Precedence: config-level grants are applied first, then workspace-level
/// grants layer on top (workspace wins). Both use the same profile base.
pub fn resolve_effective_grants(
    profile: DockerSecurityProfile,
    config_grants: Option<&DockerGrants>,
    workspace_grants: Option<&DockerGrants>,
) -> EffectiveGrants {
    let base = profile_base_grants(profile);
    let after_config = match config_grants {
        Some(g) => apply_grants(base, g),
        None => base,
    };
    let merged = match workspace_grants {
        Some(g) => apply_grants(after_config, g),
        None => after_config,
    };
    // Apply implicit caps that depend on the MERGED network tier. This must
    // run after all source layers are applied so a workspace that raises the
    // network to Allowlist also picks up the required NET_ADMIN + NET_RAW.
    // (profile_base_grants starts with Allowlist for locked/hardened; when
    // no explicit grants are provided apply_grants is never called, so the
    // injection in apply_grants is never triggered — hence this finalization.)
    apply_implicit_grants(merged)
}

/// Apply grants that depend on the fully-resolved state rather than any
/// single source. Called once at the end of `resolve_effective_grants`.
pub(crate) fn apply_implicit_grants(mut grants: EffectiveGrants) -> EffectiveGrants {
    if grants.network == NetworkGrant::Allowlist {
        for cap in ["NET_ADMIN", "NET_RAW"] {
            if !grants.capabilities_add.iter().any(|c| c == cap) {
                grants.capabilities_add.push(cap.to_owned());
            }
        }
    }
    // WP-SUDO: no_new_privileges is exactly the negation of the resolved sudo
    // grant. Set it bidirectionally so an explicit `sudo = true` under a profile
    // whose base is `no_new_privileges: true` (hardened/locked) actually clears
    // it — otherwise sudo is provisioned but no-new-privileges blocks the setuid
    // escalation, the silent-sudo-failure trap. Resolved post-merge so the final
    // sudo value governs.
    grants.no_new_privileges = !grants.sudo;
    grants
}
