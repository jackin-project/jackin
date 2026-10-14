// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockerSecurityProfile {
    Locked,

    Hardened,

    #[default]
    Standard,

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
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "unknown docker profile {:?} - valid values: locked, hardened, standard, compat",
            self.0
        )
    }
}

impl std::error::Error for ParseProfileError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkGrant {
    None,

    Allowlist,

    Open,
}

impl NetworkGrant {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DindGrant {
    None,

    Rootless,

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
