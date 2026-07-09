//! Docker security profile schema shared by config, manifests, and runtime.
//!
//! This crate owns the serde vocabulary only. Runtime behavior such as grant
//! validation, effective grant resolution, and Docker flag emission lives in
//! `jackin-runtime::runtime::docker_profile`.

use serde::{Deserialize, Serialize};

/// Named Docker isolation profile.
///
/// **ORDER IS SEMANTIC.** Variants ascend by capability/permissiveness
/// (`Locked` < `Hardened` < `Standard` < `Compat`); the derived `Ord` is relied
/// on by the role `min_profile` floor (`resolved < min` rejects an under-capable
/// profile). Reordering variants silently inverts that gate — guarded by
/// `ord_ascending_capability`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockerSecurityProfile {
    /// Minimal: allowlist network, no `DinD`, no sudo, read-only root, 4G memory.
    Locked,
    /// Restricted: allowlist network, no `DinD` by default, no sudo, read-only
    /// root, 16G memory.
    Hardened,
    /// Typical dev work: open network, no `DinD` by default, no sudo by default,
    /// writable root, 16G memory. (`DinD`/sudo can be raised by an explicit grant.)
    #[default]
    Standard,
    /// Maximum compatibility: privileged `DinD`, open network, sudo, no resource
    /// limits. Available as an explicit opt-back profile for legacy workflows.
    Compat,
}

impl std::fmt::Display for DockerSecurityProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Locked => write!(f, "locked"),
            Self::Hardened => write!(f, "hardened"),
            Self::Standard => write!(f, "standard"),
            Self::Compat => write!(f, "compat"),
        }
    }
}

impl std::str::FromStr for DockerSecurityProfile {
    type Err = ParseProfileError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "locked" => Ok(Self::Locked),
            "hardened" => Ok(Self::Hardened),
            "standard" => Ok(Self::Standard),
            "compat" => Ok(Self::Compat),
            other => Err(ParseProfileError(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseProfileError(String);

impl std::fmt::Display for ParseProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unknown docker profile {:?} - valid values: locked, hardened, standard, compat",
            self.0
        )
    }
}

impl std::error::Error for ParseProfileError {}

/// Network egress tier.
///
/// **ORDER IS SEMANTIC.** Variants ascend by permissiveness
/// (`None` < `Allowlist` < `Open`); the derived `Ord` is relied on by
/// `apply_grants` (`network > base.network` raises, never lowers). Reordering
/// silently breaks the monotone-raise guarantee — guarded by `network_grant_ord`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkGrant {
    /// No network at all (`--network none`).
    None,
    /// Routable network with an iptables OUTPUT egress allowlist.
    Allowlist,
    /// Unrestricted egress.
    Open,
}

impl NetworkGrant {
    /// Lowercase wire/label string for this tier. Single source for `Display`
    /// and the runtime `network_grant_label` / `JACKIN_NETWORK_MODE` value. The
    /// `as_str_matches_serde` guard test asserts it also equals the
    /// `#[serde(rename_all = "lowercase")]` wire form, so the contract label
    /// cannot drift from the serde vocabulary.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Allowlist => "allowlist",
            Self::Open => "open",
        }
    }
}

impl std::fmt::Display for NetworkGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Docker-in-Docker sidecar tier.
///
/// **ORDER IS SEMANTIC.** Variants ascend by privilege
/// (`None` < `Rootless` < `Privileged`); the derived `Ord` is relied on by
/// `apply_grants` (`dind > base.dind` raises, never lowers). Reordering silently
/// breaks the monotone-raise guarantee — guarded by `dind_grant_ord`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DindGrant {
    /// No `DinD` sidecar.
    None,
    /// `docker:dind-rootless` sidecar, no `--privileged`.
    Rootless,
    /// `docker:dind` sidecar with `--privileged`.
    Privileged,
}

impl std::fmt::Display for DindGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Rootless => write!(f, "rootless"),
            Self::Privileged => write!(f, "privileged"),
        }
    }
}

/// Per-dimension explicit overrides that layer on top of a profile's defaults.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DockerGrants {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkGrant>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_hosts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dind: Option<DindGrant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sudo: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_writes: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_reservation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpus: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pids: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nofile: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities_add: Vec<String>,
}

#[cfg(test)]
mod tests;
