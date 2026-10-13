// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn github_allowlist_hosts_default_and_enterprise() {
    assert_eq!(
        github_allowlist_hosts(None),
        vec!["github.com".to_owned(), "api.github.com".to_owned()]
    );
    assert_eq!(
        github_allowlist_hosts(Some("ghe.corp.example")),
        vec![
            "github.com".to_owned(),
            "api.github.com".to_owned(),
            "ghe.corp.example".to_owned()
        ]
    );
}

#[test]
fn grok_has_default_allowed_host() {
    assert!(default_allowed_hosts_for_agent("grok").contains(&"api.x.ai"));
}

#[test]
fn validate_effective_grants_catches_cross_source_root_and_sudo() {
    let grants = EffectiveGrants {
        user: "root".to_owned(),
        sudo: true,
        ..profile_base_grants(DockerSecurityProfile::Compat)
    };
    let errors = validate_effective_grants(&grants);
    assert!(
        !errors.is_empty(),
        "user=root + sudo=true must be caught by validate_effective_grants"
    );
    assert!(matches!(errors[0], GrantValidationError::RootAndSudo));
}

#[test]
fn validate_effective_grants_catches_cross_source_reservation_exceeds_memory() {
    let grants = EffectiveGrants {
        memory_bytes: Some(4 * GB),
        memory_reservation_bytes: Some(8 * GB),
        ..profile_base_grants(DockerSecurityProfile::Standard)
    };
    let errors = validate_effective_grants(&grants);
    assert!(
        !errors.is_empty(),
        "memory_reservation > memory must be caught by validate_effective_grants"
    );
    assert!(matches!(
        errors[0],
        GrantValidationError::MemoryReservationExceedsMemory { .. }
    ));
}

#[test]
fn validate_effective_grants_passes_when_invariants_hold() {
    let grants = EffectiveGrants {
        memory_bytes: Some(16 * GB),
        memory_reservation_bytes: Some(12 * GB),
        ..profile_base_grants(DockerSecurityProfile::Standard)
    };
    let errors = validate_effective_grants(&grants);
    assert!(
        errors.is_empty(),
        "valid grants should produce no errors: {errors:?}"
    );
}

#[test]
fn resolve_effective_grants_no_grants_still_gets_implicit_caps() {
    // When locked profile launches with no config/workspace grants,
    // resolve_effective_grants must inject NET_ADMIN/NET_RAW so the
    // iptables allowlist (`jackin-capsule firewall-apply`) can run.
    let grants = resolve_effective_grants(DockerSecurityProfile::Locked, None, None);
    assert_eq!(grants.network, NetworkGrant::Allowlist);
    assert!(
        grants.capabilities_add.iter().any(|c| c == "NET_ADMIN"),
        "Locked with no grants must have implicit NET_ADMIN from resolve_effective_grants"
    );
    assert!(
        grants.capabilities_add.iter().any(|c| c == "NET_RAW"),
        "Locked with no grants must have implicit NET_RAW from resolve_effective_grants"
    );
}

#[test]
fn locked_tmpfs_is_minimal_subset() {
    let grants = profile_base_grants(DockerSecurityProfile::Locked);
    let flags = readonly_root_flags(DockerSecurityProfile::Locked, &grants);
    assert!(
        flags.contains(&"--read-only".to_owned()),
        "locked must be read-only"
    );
    let tmpfs_values = tmpfs_paths_from_flags(&flags);
    assert!(
        tmpfs_values.contains(&"/tmp"),
        "locked must have /tmp tmpfs"
    );
    assert!(
        tmpfs_values.contains(&"/run"),
        "locked must have /run tmpfs"
    );
    // Package-manager paths absent from locked.
    for path in TMPFS_PATHS_HARDENED_EXTRA {
        assert!(
            !tmpfs_values.contains(path),
            "locked must not have {path} (package-manager path, hardened only)"
        );
    }
}

#[test]
fn hardened_tmpfs_includes_extra_paths() {
    let grants = profile_base_grants(DockerSecurityProfile::Hardened);
    let flags = readonly_root_flags(DockerSecurityProfile::Hardened, &grants);
    let tmpfs_values = tmpfs_paths_from_flags(&flags);
    for path in TMPFS_PATHS_HARDENED_EXTRA {
        assert!(
            tmpfs_values.contains(path),
            "hardened tmpfs must include {path}"
        );
    }
}

#[test]
fn network_enforcement_label_all_cases() {
    // n/a for open network.
    let open = EffectiveGrants {
        network: NetworkGrant::Open,
        ..profile_base_grants(DockerSecurityProfile::Standard)
    };
    assert_eq!(network_enforcement_label(&open), "n/a");

    // full: allowlist, no sudo, no dind.
    let full = profile_base_grants(DockerSecurityProfile::Locked);
    assert_eq!(network_enforcement_label(&full), "full");

    // partial: allowlist + sudo.
    let partial_sudo = EffectiveGrants {
        network: NetworkGrant::Allowlist,
        sudo: true,
        ..profile_base_grants(DockerSecurityProfile::Hardened)
    };
    assert_eq!(
        network_enforcement_label(&partial_sudo),
        "partial (sudo grants iptables access)"
    );

    // partial: allowlist + dind active.
    let partial_dind = EffectiveGrants {
        network: NetworkGrant::Allowlist,
        dind: DindGrant::Privileged,
        ..profile_base_grants(DockerSecurityProfile::Hardened)
    };
    assert_eq!(
        network_enforcement_label(&partial_dind),
        "partial (DinD inner containers bypass host iptables)"
    );
}

#[test]
fn session_contract_reports_dind_inner_egress_partial_enforcement() {
    let grants = EffectiveGrants {
        network: NetworkGrant::Allowlist,
        dind: DindGrant::Rootless,
        ..profile_base_grants(DockerSecurityProfile::Standard)
    };
    let contract = format_session_contract(
        DockerSecurityProfile::Standard,
        "config",
        &grants,
        true,
        "docker-default",
        "v2",
        "provisioned",
        true,
    );
    assert!(
        contract.contains("enforcement: partial (DinD inner containers bypass host iptables)"),
        "{contract}"
    );
    assert!(
        contract.contains("DinD sidecar has kernel access"),
        "{contract}"
    );
}

#[test]
fn allowlist_network_with_grants_still_gets_implicit_caps() {
    // Hardened profile has Allowlist network; add a memory config grant.
    // apply_grants runs (so it fired), then apply_implicit_grants must still add caps.
    let config_grants = DockerGrants {
        memory: Some("8G".to_owned()),
        ..Default::default()
    };
    let grants =
        resolve_effective_grants(DockerSecurityProfile::Hardened, Some(&config_grants), None);
    assert_eq!(grants.network, NetworkGrant::Allowlist);
    assert!(
        grants.capabilities_add.iter().any(|c| c == "NET_ADMIN"),
        "Hardened with config grants must still have implicit NET_ADMIN; got: {:?}",
        grants.capabilities_add
    );
}

#[test]
fn grant_layering_workspace_wins_over_config() {
    let config_grants = DockerGrants {
        memory: Some("4G".to_owned()),
        cpus: Some(2.0),
        ..Default::default()
    };
    let workspace_grants = DockerGrants {
        memory: Some("16G".to_owned()), // workspace raises memory
        ..Default::default()
    };
    let grants = resolve_effective_grants(
        DockerSecurityProfile::Standard,
        Some(&config_grants),
        Some(&workspace_grants),
    );
    // Workspace memory wins (higher).
    assert_eq!(grants.memory_bytes, Some(16 * GB));
    // Config cpus preserved (workspace didn't override).
    assert_eq!(grants.cpus, Some(4.0_f64.max(2.0))); // profile default 4.0 wins over config 2.0
}

#[test]
fn validate_grants_pids_must_be_positive() {
    let grants = DockerGrants {
        pids: Some(-1),
        ..Default::default()
    };
    let errors = validate_grants(&grants);
    assert!(!errors.is_empty(), "pids = -1 should be an error");
    assert!(
        matches!(&errors[0], GrantValidationError::ValueOutOfRange { field, .. } if *field == "pids"),
        "error should be ValueOutOfRange for pids"
    );
}

#[test]
fn validate_grants_memory_overflow_is_error() {
    // 2^63 bytes = i64::MAX + 1, expressed with a parseable `G` suffix so the
    // value actually reaches the i64-boundary check. A bare-"B" value would be
    // rejected as UnparsableSize first and never exercise the overflow branch.
    let grants = DockerGrants {
        memory: Some("8589934592G".to_owned()),
        ..Default::default()
    };
    let errors = validate_grants(&grants);
    assert!(
        errors.iter().any(|e| matches!(
            e,
            GrantValidationError::ValueOutOfRange {
                field: "memory",
                ..
            }
        )),
        "memory > i64::MAX must be ValueOutOfRange, got {errors:?}"
    );
}

#[test]
fn validate_grants_cpus_must_be_finite_positive() {
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let grants = DockerGrants {
            cpus: Some(bad),
            ..Default::default()
        };
        let errors = validate_grants(&grants);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                GrantValidationError::ValueOutOfRange { field: "cpus", .. }
            )),
            "cpus={bad} must be ValueOutOfRange, got {errors:?}"
        );
    }
}

#[test]
fn validate_grants_nofile_zero_is_error() {
    let grants = DockerGrants {
        nofile: Some(0),
        ..Default::default()
    };
    let errors = validate_grants(&grants);
    assert!(
        errors.iter().any(|e| matches!(
            e,
            GrantValidationError::ValueOutOfRange {
                field: "nofile",
                ..
            }
        )),
        "nofile=0 must be ValueOutOfRange, got {errors:?}"
    );
}

#[test]
fn compat_profile_base_grants_sudo_on() {
    let grants = profile_base_grants(DockerSecurityProfile::Compat);
    assert!(grants.sudo, "compat base grants must have sudo=true");
}

#[test]
fn explicit_sudo_grant_flips_standard() {
    let config = DockerGrants {
        sudo: Some(true),
        ..Default::default()
    };
    let grants = resolve_effective_grants(DockerSecurityProfile::Standard, Some(&config), None);
    assert!(
        grants.sudo,
        "explicit sudo=true grant must override standard default"
    );
}

#[test]
fn no_new_privileges_on_when_sudo_off() {
    let grants = resolve_effective_grants(DockerSecurityProfile::Standard, None, None);
    assert!(!grants.sudo);
    assert!(
        grants.no_new_privileges,
        "no_new_privileges must be true when sudo=false (standard no-grant)"
    );
}

#[test]
fn no_new_privileges_off_when_sudo_granted() {
    let config = DockerGrants {
        sudo: Some(true),
        ..Default::default()
    };
    let grants = resolve_effective_grants(DockerSecurityProfile::Standard, Some(&config), None);
    assert!(grants.sudo);
    assert!(
        !grants.no_new_privileges,
        "no_new_privileges must be false when sudo=true"
    );
}

#[test]
fn compat_sudo_on_means_no_new_privileges_off() {
    let grants = resolve_effective_grants(DockerSecurityProfile::Compat, None, None);
    assert!(grants.sudo);
    assert!(
        !grants.no_new_privileges,
        "compat profile: sudo=true so no_new_privileges must be false"
    );
}

#[test]
fn hardened_sudo_grant_clears_no_new_privileges() {
    // hardened base is no_new_privileges:true + sudo:false. An explicit
    // sudo=true grant must clear no_new_privileges, else sudo is provisioned
    // but no-new-privileges blocks the setuid escalation (silent failure).
    let config = DockerGrants {
        sudo: Some(true),
        ..Default::default()
    };
    let grants = resolve_effective_grants(DockerSecurityProfile::Hardened, Some(&config), None);
    assert!(grants.sudo);
    assert!(
        !grants.no_new_privileges,
        "hardened + sudo=true must clear no_new_privileges so sudo works"
    );
}

#[test]
fn profile_base_grants_dind_defaults() {
    // Decision 12 / WP4: only compat keeps privileged DinD; every secure-default
    // profile (standard/hardened/locked) defaults DinD off.
    for (profile, expected) in [
        (DockerSecurityProfile::Standard, DindGrant::None),
        (DockerSecurityProfile::Compat, DindGrant::Privileged),
        (DockerSecurityProfile::Hardened, DindGrant::None),
        (DockerSecurityProfile::Locked, DindGrant::None),
    ] {
        assert_eq!(
            profile_base_grants(profile).dind,
            expected,
            "{profile} base dind"
        );
    }
}
