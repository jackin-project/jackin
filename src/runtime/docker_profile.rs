/// Docker security profiles — named bundles of capability grants.
///
/// Profiles are ordered ascending by capability: `Locked` is the tightest,
/// `Compat` grants everything. An operator grants up from a locked baseline
/// rather than restricting down from a permissive one.
///
/// Phase 1: the enum and type infrastructure exist; all profiles resolve to
/// `Compat`-equivalent Docker flags until Phase 2–5 wire each dimension.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockerSecurityProfile {
    /// Minimal — model API access only, no DinD, no sudo, read-only root.
    /// Purpose-built read-only analysis roles. Highest confidence in
    /// container boundary.
    Locked,
    /// Restricted — no DinD by default, api_only network, no sudo, read-only
    /// root. For untrusted repos or long autonomous runs where inner Docker
    /// is not needed.
    Hardened,
    /// Typical dev work — open network, DinD, sudo, writable root, resource
    /// limits required. Intended eventual default after the sudo audit.
    Standard,
    /// Maximum compatibility — today's behavior. Privileged DinD, open
    /// network, NOPASSWD:ALL sudo, no resource limits. Explicit opt-in for
    /// roles that need everything.
    Compat,
}

impl Default for DockerSecurityProfile {
    fn default() -> Self {
        // `Compat` is the initial default while `Standard` is not yet fully
        // validated (sudo audit pending). This will flip to `Standard` in a
        // later phase once the base-image sudo audit is resolved.
        Self::Compat
    }
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
            other => Err(ParseProfileError(other.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseProfileError(String);

impl std::fmt::Display for ParseProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unknown docker profile {:?} — valid values: locked, hardened, standard, compat",
            self.0
        )
    }
}

impl std::error::Error for ParseProfileError {}

/// Resolve the effective profile for a launch from the available sources.
///
/// Precedence (highest to lowest):
/// 1. CLI `--docker-profile` override
/// 2. Workspace `[docker] profile` (not yet wired — Phase 2)
/// 3. Role manifest `[docker] min_profile` (not yet wired — Phase 2)
/// 4. Global `[docker] default_profile` from config.toml (not yet wired — Phase 2)
/// 5. Compiled-in default (`Compat` until sudo audit resolves)
///
/// Phase 1: only the CLI override and the compiled-in default are active.
/// All profiles produce identical Docker flags (`Compat` behavior).
pub fn resolve_profile(cli_override: Option<DockerSecurityProfile>) -> DockerSecurityProfile {
    cli_override.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        for profile in [
            DockerSecurityProfile::Locked,
            DockerSecurityProfile::Hardened,
            DockerSecurityProfile::Standard,
            DockerSecurityProfile::Compat,
        ] {
            let s = profile.to_string();
            let parsed: DockerSecurityProfile = s.parse().unwrap();
            assert_eq!(parsed, profile);
        }
    }

    #[test]
    fn ord_ascending_capability() {
        assert!(DockerSecurityProfile::Locked < DockerSecurityProfile::Hardened);
        assert!(DockerSecurityProfile::Hardened < DockerSecurityProfile::Standard);
        assert!(DockerSecurityProfile::Standard < DockerSecurityProfile::Compat);
    }

    #[test]
    fn default_is_compat() {
        assert_eq!(DockerSecurityProfile::default(), DockerSecurityProfile::Compat);
    }

    #[test]
    fn unknown_profile_is_error() {
        assert!("ultra".parse::<DockerSecurityProfile>().is_err());
    }

    #[test]
    fn resolve_cli_override_wins() {
        assert_eq!(
            resolve_profile(Some(DockerSecurityProfile::Locked)),
            DockerSecurityProfile::Locked,
        );
    }

    #[test]
    fn resolve_no_override_returns_default() {
        assert_eq!(resolve_profile(None), DockerSecurityProfile::default());
    }
}
