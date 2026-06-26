//! Guard tests co-located with the schema enums whose invariants they protect.
//!
//! The runtime crate (`jackin-runtime::runtime::docker_profile`) relies on the
//! derived `Ord` of these enums for its floor / monotone-raise gates, but those
//! tests live a crate away. These local guards fire in `jackin-core` itself, so
//! a developer reordering a variant here sees the failure beside the change.

use super::{DindGrant, DockerSecurityProfile, NetworkGrant};

/// `DockerSecurityProfile` variants must ascend by capability/permissiveness:
/// the role `min_profile` floor relies on `resolved >= min`.
#[test]
fn profile_ord_ascending_capability() {
    assert!(DockerSecurityProfile::Locked < DockerSecurityProfile::Hardened);
    assert!(DockerSecurityProfile::Hardened < DockerSecurityProfile::Standard);
    assert!(DockerSecurityProfile::Standard < DockerSecurityProfile::Compat);
}

/// `NetworkGrant` variants must ascend by permissiveness: `apply_grants` only
/// ever raises (`network > base.network`), never lowers.
#[test]
fn network_grant_ord_ascending() {
    assert!(NetworkGrant::None < NetworkGrant::Allowlist);
    assert!(NetworkGrant::Allowlist < NetworkGrant::Open);
}

/// `DindGrant` variants must ascend by privilege: `apply_grants` only raises.
#[test]
fn dind_grant_ord_ascending() {
    assert!(DindGrant::None < DindGrant::Rootless);
    assert!(DindGrant::Rootless < DindGrant::Privileged);
}

/// `NetworkGrant::as_str` must equal the `#[serde(rename_all = "lowercase")]`
/// wire form, so the runtime contract label cannot drift from the serde
/// vocabulary (the guarantee `as_str`'s doc comment claims).
#[test]
fn as_str_matches_serde() {
    for grant in [NetworkGrant::None, NetworkGrant::Allowlist, NetworkGrant::Open] {
        let serde_form = serde_json::to_string(&grant).expect("serialize NetworkGrant");
        // serde_json wraps the unit variant in quotes, e.g. "\"none\"".
        assert_eq!(serde_form, format!("{:?}", grant.as_str()));
    }
}
