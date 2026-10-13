// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
fn default_is_standard() {
    assert_eq!(
        DockerSecurityProfile::default(),
        DockerSecurityProfile::Standard
    );
}

#[test]
fn unknown_profile_is_error() {
    "ultra".parse::<DockerSecurityProfile>().unwrap_err();
}

#[test]
fn resolve_cli_override_wins() {
    let (profile, source) = resolve_profile(
        Some(DockerSecurityProfile::Locked),
        Some(DockerSecurityProfile::Standard),
        Some(DockerSecurityProfile::Compat),
    );
    assert_eq!(profile, DockerSecurityProfile::Locked);
    assert_eq!(source, ProfileSource::Cli);
}

#[test]
fn resolve_workspace_beats_config() {
    let (profile, source) = resolve_profile(
        None,
        Some(DockerSecurityProfile::Hardened),
        Some(DockerSecurityProfile::Compat),
    );
    assert_eq!(profile, DockerSecurityProfile::Hardened);
    assert_eq!(source, ProfileSource::Workspace);
}

#[test]
fn resolve_no_override_returns_default() {
    let (profile, source) = resolve_profile(None, None, None);
    assert_eq!(profile, DockerSecurityProfile::default());
    assert_eq!(source, ProfileSource::Default);
}

#[test]
fn resolve_config_source_tracked() {
    let (profile, source) = resolve_profile(None, None, Some(DockerSecurityProfile::Standard));
    assert_eq!(profile, DockerSecurityProfile::Standard);
    assert_eq!(source, ProfileSource::Config);
}

#[test]
fn parse_memory_bytes_units() {
    assert_eq!(parse_memory_bytes("512M"), Some(512 * 1024 * 1024));
    assert_eq!(parse_memory_bytes("4G"), Some(4 * 1024 * 1024 * 1024));
    assert_eq!(parse_memory_bytes("2048K"), Some(2048 * 1024));
    assert_eq!(parse_memory_bytes("1024"), Some(1024));
    assert_eq!(parse_memory_bytes("4g"), Some(4 * 1024 * 1024 * 1024));
    assert_eq!(parse_memory_bytes("bad"), None);
    assert_eq!(parse_memory_bytes(""), None);
}

#[test]
fn validate_grants_root_and_sudo_error() {
    let grants = DockerGrants {
        user: Some("root".to_owned()),
        sudo: Some(true),
        ..Default::default()
    };
    let errors = validate_grants(&grants);
    assert!(!errors.is_empty());
    assert!(matches!(errors[0], GrantValidationError::RootAndSudo));
}

#[test]
fn validate_grants_unknown_cap_error() {
    let grants = DockerGrants {
        capabilities_add: vec!["MAGIC_CAP".to_owned()],
        ..Default::default()
    };
    let errors = validate_grants(&grants);
    assert!(!errors.is_empty());
    assert!(matches!(&errors[0], GrantValidationError::UnknownCapability(s) if s == "MAGIC_CAP"));
}

#[test]
fn validate_grants_cap_prefix_stripped() {
    let grants = DockerGrants {
        capabilities_add: vec!["CAP_NET_RAW".to_owned()],
        ..Default::default()
    };
    let errors = validate_grants(&grants);
    assert!(
        errors.is_empty(),
        "CAP_NET_RAW should be valid after stripping prefix"
    );
}

#[test]
fn validate_grants_memory_reservation_exceeds_memory() {
    let grants = DockerGrants {
        memory: Some("4G".to_owned()),
        memory_reservation: Some("8G".to_owned()),
        ..Default::default()
    };
    let errors = validate_grants(&grants);
    assert!(!errors.is_empty());
    assert!(matches!(
        errors[0],
        GrantValidationError::MemoryReservationExceedsMemory { .. }
    ));
}

#[test]
fn validate_grants_valid_passes() {
    let grants = DockerGrants {
        memory: Some("16G".to_owned()),
        memory_reservation: Some("12G".to_owned()),
        cpus: Some(4.0),
        pids: Some(2048),
        nofile: Some(8192),
        capabilities_add: vec!["NET_RAW".to_owned(), "SYS_PTRACE".to_owned()],
        ..Default::default()
    };
    let errors = validate_grants(&grants);
    assert!(errors.is_empty());
}

#[test]
fn resource_flags_full() {
    let grants = EffectiveGrants {
        network: NetworkGrant::Open,
        allowed_hosts: vec![],
        dind: DindGrant::Privileged,
        user: "agent".to_owned(),
        sudo: true,
        system_writes: true,
        memory_bytes: Some(4 * GB),
        memory_reservation_bytes: Some(3 * GB),
        cpus: Some(2.0),
        pids: Some(512),
        nofile: Some(2048),
        capabilities_add: vec![],
        no_new_privileges: false,
    };
    let flags = resource_flags(&grants);
    assert!(flags.contains(&"--memory".to_owned()));
    assert!(flags.contains(&"--memory-reservation".to_owned()));
    assert!(flags.contains(&"--cpus".to_owned()));
    assert!(flags.contains(&"--pids-limit".to_owned()));
    assert!(flags.contains(&"--ulimit".to_owned()));
}

#[test]
fn resource_flags_empty_for_compat() {
    let grants = profile_base_grants(DockerSecurityProfile::Compat);
    let flags = resource_flags(&grants);
    assert!(flags.is_empty());
}

#[test]
fn capability_flags_hardened_drops_all() {
    let flags = capability_flags(DockerSecurityProfile::Hardened, &[]);
    assert!(flags.contains(&"--cap-drop=ALL".to_owned()));
    for cap in MINIMUM_CAPABILITIES {
        assert!(
            flags.contains(&"--cap-add".to_owned()),
            "missing --cap-add for {cap}"
        );
        assert!(flags.contains(&(*cap).to_owned()));
    }
}

#[test]
fn capability_flags_compat_empty() {
    let flags = capability_flags(DockerSecurityProfile::Compat, &[]);
    assert!(flags.is_empty());
}

#[test]
fn capability_flags_compat_adds_extra_without_drop_all() {
    // Non-drop-all profile: extra caps are added, but no --cap-drop=ALL.
    let flags = capability_flags(DockerSecurityProfile::Compat, &["NET_ADMIN".to_owned()]);
    assert!(!flags.iter().any(|f| f == "--cap-drop=ALL"));
    assert!(flags.windows(2).any(|w| w == ["--cap-add", "NET_ADMIN"]));
}

#[test]
fn capability_flags_hardened_adds_extra_on_top_of_minimum() {
    // Drop-all profile: --cap-drop=ALL + the minimum set + the extra cap.
    let flags = capability_flags(
        DockerSecurityProfile::Hardened,
        &["CAP_NET_ADMIN".to_owned()],
    );
    assert!(flags.contains(&"--cap-drop=ALL".to_owned()));
    // Extra cap is normalized (CAP_ prefix stripped) and added.
    assert!(flags.windows(2).any(|w| w == ["--cap-add", "NET_ADMIN"]));
    // The minimum set is still present alongside the extra.
    assert!(flags.windows(2).any(|w| w == ["--cap-add", "SETUID"]));
}

#[test]
fn readonly_root_flags_for_locked() {
    let grants = profile_base_grants(DockerSecurityProfile::Locked);
    let flags = readonly_root_flags(DockerSecurityProfile::Locked, &grants);
    assert!(flags.contains(&"--read-only".to_owned()));
    assert!(flags.iter().any(|f| f.starts_with("/tmp")));
}

#[test]
fn readonly_root_flags_empty_for_compat() {
    let grants = profile_base_grants(DockerSecurityProfile::Compat);
    let flags = readonly_root_flags(DockerSecurityProfile::Compat, &grants);
    assert!(flags.is_empty());
}

#[test]
fn readonly_root_flags_mount_sudoers_tmpfs_only_when_sudo_granted() {
    let no_sudo = profile_base_grants(DockerSecurityProfile::Hardened);
    assert!(!no_sudo.sudo);
    let flags = readonly_root_flags(DockerSecurityProfile::Hardened, &no_sudo);
    assert!(
        !flags.iter().any(|f| f.starts_with("/etc/sudoers.d")),
        "no sudo grant must not mount the sudoers tmpfs"
    );

    let with_sudo = EffectiveGrants {
        sudo: true,
        no_new_privileges: false,
        ..profile_base_grants(DockerSecurityProfile::Hardened)
    };
    let flags = readonly_root_flags(DockerSecurityProfile::Hardened, &with_sudo);
    assert!(
        flags
            .iter()
            .any(|f| f == "/etc/sudoers.d:rw,nosuid,nodev,mode=0755"),
        "sudo on read-only root must mount a root-owned /etc/sudoers.d tmpfs, got {flags:?}"
    );
}

#[test]
fn apply_grants_raises_network() {
    let base = profile_base_grants(DockerSecurityProfile::Locked);
    assert_eq!(base.network, NetworkGrant::Allowlist);
    let grants = DockerGrants {
        network: Some(NetworkGrant::Open),
        ..Default::default()
    };
    let effective = apply_grants(base, &grants);
    assert_eq!(effective.network, NetworkGrant::Open);
}

#[test]
fn apply_grants_cannot_lower_network() {
    let base = profile_base_grants(DockerSecurityProfile::Standard);
    assert_eq!(base.network, NetworkGrant::Open);
    let grants = DockerGrants {
        network: Some(NetworkGrant::None),
        ..Default::default()
    };
    // Grant is lower than profile default — profile wins.
    let effective = apply_grants(base, &grants);
    assert_eq!(effective.network, NetworkGrant::Open);
}

#[test]
fn network_grant_ord() {
    assert!(NetworkGrant::None < NetworkGrant::Allowlist);
    assert!(NetworkGrant::Allowlist < NetworkGrant::Open);
}

#[test]
fn dind_grant_ord() {
    assert!(DindGrant::None < DindGrant::Rootless);
    assert!(DindGrant::Rootless < DindGrant::Privileged);
}

#[test]
fn apply_grants_raises_all_resource_ceilings() {
    let base = profile_base_grants(DockerSecurityProfile::Locked);
    let grants = DockerGrants {
        memory: Some("64G".to_owned()),
        cpus: Some(16.0),
        pids: Some(99_999),
        nofile: Some(1_048_576),
        dind: Some(DindGrant::Privileged),
        ..Default::default()
    };
    let e = apply_grants(base, &grants);
    assert_eq!(e.memory_bytes, Some(64 * GB));
    assert_eq!(e.cpus, Some(16.0));
    assert_eq!(e.pids, Some(99_999));
    assert_eq!(e.nofile, Some(1_048_576));
    assert_eq!(e.dind, DindGrant::Privileged);
}

#[test]
fn apply_grants_never_lowers_resource_ceilings() {
    let base = EffectiveGrants {
        memory_bytes: Some(8 * GB),
        cpus: Some(4.0),
        pids: Some(4096),
        nofile: Some(65536),
        ..profile_base_grants(DockerSecurityProfile::Standard)
    };
    let grants = DockerGrants {
        memory: Some("1G".to_owned()),
        cpus: Some(0.5),
        pids: Some(1),
        nofile: Some(8),
        ..Default::default()
    };
    let e = apply_grants(base, &grants);
    assert_eq!(e.memory_bytes, Some(8 * GB));
    assert_eq!(e.cpus, Some(4.0));
    assert_eq!(e.pids, Some(4096));
    assert_eq!(e.nofile, Some(65536));
}

#[test]
fn apply_grants_cannot_lower_dind() {
    let base = profile_base_grants(DockerSecurityProfile::Compat);
    assert_eq!(base.dind, DindGrant::Privileged);
    let grants = DockerGrants {
        dind: Some(DindGrant::Rootless),
        ..Default::default()
    };
    assert_eq!(apply_grants(base, &grants).dind, DindGrant::Privileged);
}

#[test]
fn fold_role_grants_pins_dind_off_over_capable_profile() {
    // The only down-force in the grant system: a role can pin DinD OFF even
    // after a more capable profile raised it.
    let base = profile_base_grants(DockerSecurityProfile::Compat);
    assert_eq!(base.dind, DindGrant::Privileged);
    let role = DockerGrants {
        dind: Some(DindGrant::None),
        ..Default::default()
    };
    assert_eq!(fold_role_grants(base, &role).dind, DindGrant::None);
}

#[test]
fn fold_role_grants_pin_off_does_not_strip_other_raises() {
    let base = profile_base_grants(DockerSecurityProfile::Standard);
    let role = DockerGrants {
        dind: Some(DindGrant::None),
        capabilities_add: vec!["NET_ADMIN".to_owned()],
        ..Default::default()
    };
    let folded = fold_role_grants(base, &role);
    assert_eq!(folded.dind, DindGrant::None);
    assert!(folded.capabilities_add.iter().any(|c| c == "NET_ADMIN"));
}

#[test]
fn fold_role_grants_raises_dind_when_not_pinned() {
    let base = profile_base_grants(DockerSecurityProfile::Standard);
    assert_eq!(base.dind, DindGrant::None);
    let role = DockerGrants {
        dind: Some(DindGrant::Rootless),
        ..Default::default()
    };
    assert_eq!(fold_role_grants(base, &role).dind, DindGrant::Rootless);
}

#[test]
fn profile_meets_floor_respects_ascending_capability() {
    use DockerSecurityProfile::{Compat, Hardened, Locked};
    // Floor `hardened`: `locked` is more restrictive (less capable) → rejected.
    assert!(!profile_meets_floor(Locked, Hardened));
    assert!(profile_meets_floor(Hardened, Hardened));
    assert!(profile_meets_floor(Compat, Hardened));
}
