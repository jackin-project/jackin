// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    DockerGrants, DockerSecurityProfile, EffectiveGrants, GrantValidationError, NetworkGrant,
    apply_grants, profile_base_grants,
};

pub fn profile_meets_floor(resolved: DockerSecurityProfile, min: DockerSecurityProfile) -> bool {
    resolved >= min
}

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

pub fn validate_effective_grants(grants: &EffectiveGrants) -> Vec<GrantValidationError> {
    let mut errors = Vec::new();
    if grants.user == "root" && grants.sudo {
        errors.push(GrantValidationError::RootAndSudo);
    }
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
    apply_implicit_grants(merged)
}

pub(crate) fn apply_implicit_grants(mut grants: EffectiveGrants) -> EffectiveGrants {
    if grants.network == NetworkGrant::Allowlist {
        for cap in ["NET_ADMIN", "NET_RAW"] {
            if !grants.capabilities_add.iter().any(|c| c == cap) {
                grants.capabilities_add.push(cap.to_owned());
            }
        }
    }
    grants.no_new_privileges = !grants.sudo;
    grants
}
