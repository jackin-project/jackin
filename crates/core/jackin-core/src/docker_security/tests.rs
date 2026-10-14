// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{DindGrant, DockerSecurityProfile, NetworkGrant, ParseProfileError};
use std::str::FromStr;

#[test]
fn profile_ord_ascending_capability() {
    assert!(DockerSecurityProfile::Locked < DockerSecurityProfile::Hardened);
    assert!(DockerSecurityProfile::Hardened < DockerSecurityProfile::Standard);
    assert!(DockerSecurityProfile::Standard < DockerSecurityProfile::Compat);
}

#[test]
fn network_grant_ord_ascending() {
    assert!(NetworkGrant::None < NetworkGrant::Allowlist);
    assert!(NetworkGrant::Allowlist < NetworkGrant::Open);
}

#[test]
fn dind_grant_ord_ascending() {
    assert!(DindGrant::None < DindGrant::Rootless);
    assert!(DindGrant::Rootless < DindGrant::Privileged);
}

#[test]
fn as_str_matches_serde() {
    for grant in [
        NetworkGrant::None,
        NetworkGrant::Allowlist,
        NetworkGrant::Open,
    ] {
        let serde_form = serde_json::to_string(&grant).expect("serialize NetworkGrant");

        assert_eq!(serde_form, format!("{:?}", grant.as_str()));
    }
}

#[test]
fn parse_profile_error_message_parity() {
    let err = DockerSecurityProfile::from_str("bogus").unwrap_err();
    assert_eq!(
        err.to_string(),
        "unknown docker profile \"bogus\" - valid values: locked, hardened, standard, compat"
    );
    let _typed: ParseProfileError = err;
}
