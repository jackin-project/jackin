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
    assert_eq!(
        DockerSecurityProfile::default(),
        DockerSecurityProfile::Compat
    );
}

#[test]
fn unknown_profile_is_error() {
    assert!("ultra".parse::<DockerSecurityProfile>().is_err());
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

/// Sudo granted on a read-only-root profile mounts a writable tmpfs over
/// /etc/sudoers.d so `sudo-provision` can write the sudoers entry (otherwise
/// EROFS on the read-only /etc fails the launch). No sudo → no such mount.
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

// ── validate_effective_grants ─────────────────────────────────────────────

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

// ── Test gap fixes ────────────────────────────────────────────────────────

/// Extract the path component from every `--tmpfs <path>:opts` pair in a flags vec.
fn tmpfs_paths_from_flags(flags: &[String]) -> Vec<&str> {
    flags
        .iter()
        .enumerate()
        .filter(|(i, _)| *i > 0 && flags.get(*i - 1).is_some_and(|f| f == "--tmpfs"))
        .map(|(_, v)| v.split(':').next().unwrap_or(""))
        .collect()
}

/// locked tmpfs: minimal set only (no package-manager paths).
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

/// hardened tmpfs includes both minimal + package-manager paths.
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

/// `network_enforcement_label`: full, partial-sudo, partial-dind, n/a.
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

/// Implicit caps are also present when hardened profile (Allowlist) has config grants.
/// `apply_implicit_grants` must fire even when `apply_grants` already ran.
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

/// Grant layering: workspace wins over config when raising.
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

/// `validate_grants` rejects pids <= 0.
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

/// `validate_grants` rejects memory exceeding `i64::MAX`.
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

/// `validate_grants` rejects non-finite or non-positive `cpus`.
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

/// `validate_grants` rejects `nofile = 0` (forbids opening any fd).
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

// ── WP-SUDO: profile sudo defaults ───────────────────────────────────────

#[test]
fn compat_profile_base_grants_sudo_on() {
    let grants = profile_base_grants(DockerSecurityProfile::Compat);
    assert!(grants.sudo, "compat base grants must have sudo=true");
}

// Per-profile base `.sudo` defaults are asserted together in
// `sudo_default_off_outside_compat_on_for_compat`.

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

// ── WP-SUDO: no_new_privileges tied to !sudo ──────────────────────────────

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

// ── WP4: standard DinD default is None ───────────────────────────────────

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

// ── WP3: AppArmor probe parser ────────────────────────────────────────────

#[test]
fn parse_apparmor_present_host() {
    let (available, layer) =
        parse_apparmor_from_docker_info("name=apparmor name=seccomp,profile=default");
    assert!(available, "apparmor string should be detected");
    // Layer is host on non-macOS test runner.
    assert!(layer == "host" || layer == "backend-vm");
}

#[test]
fn parse_apparmor_absent() {
    let (available, _layer) = parse_apparmor_from_docker_info("name=seccomp,profile=default");
    assert!(
        !available,
        "should report unavailable when no apparmor token"
    );
}

#[test]
fn parse_apparmor_empty_string() {
    let (available, _layer) = parse_apparmor_from_docker_info("");
    assert!(!available);
}

// ── WP3: cgroup validation ────────────────────────────────────────────────

#[test]
fn validate_cgroup_compat_accepts_v1() {
    let result = validate_cgroup_for_profile(DockerSecurityProfile::Compat, "v1");
    assert!(result.is_ok(), "compat must accept cgroup v1");
}

#[test]
fn validate_cgroup_standard_warns_on_v1() {
    let result = validate_cgroup_for_profile(DockerSecurityProfile::Standard, "v1");
    assert!(
        matches!(result, Ok(Some(w)) if w.contains("memory_reservation")),
        "standard on v1 must warn about memory_reservation (warn only, no hard fail)"
    );
}

#[test]
fn validate_cgroup_hardened_fails_on_v1() {
    let result = validate_cgroup_for_profile(DockerSecurityProfile::Hardened, "v1");
    assert!(result.is_err(), "hardened must fail-closed on cgroup v1");
}

#[test]
fn validate_cgroup_locked_fails_on_v1() {
    let result = validate_cgroup_for_profile(DockerSecurityProfile::Locked, "v1");
    assert!(result.is_err(), "locked must fail-closed on cgroup v1");
}

#[test]
fn validate_cgroup_hardened_accepts_v2() {
    let result = validate_cgroup_for_profile(DockerSecurityProfile::Hardened, "v2");
    assert!(result.is_ok(), "hardened must accept cgroup v2");
}

// ── WP1: egress allowlist assembly ────────────────────────────────────────

#[test]
fn allowlist_union_dedups_and_includes_all_sources() {
    let mut grants = profile_base_grants(DockerSecurityProfile::Hardened);
    grants.allowed_hosts = vec!["example.com".to_owned(), "api.anthropic.com".to_owned()];
    let github = vec!["github.com".to_owned()];
    let hosts = allowlist_hosts("claude", &grants, &github, Some("host.docker.internal"));
    // configured first, agent default (api.anthropic.com already present, deduped),
    // github, then OTLP.
    assert_eq!(
        hosts,
        vec![
            "example.com".to_owned(),
            "api.anthropic.com".to_owned(),
            "github.com".to_owned(),
            "host.docker.internal".to_owned(),
        ]
    );
}

#[test]
fn allowlist_always_includes_otlp_even_when_otherwise_empty() {
    let grants = profile_base_grants(DockerSecurityProfile::Locked);
    let hosts = allowlist_hosts("unknown-agent", &grants, &[], Some("host.docker.internal"));
    assert_eq!(hosts, vec!["host.docker.internal".to_owned()]);
}

#[test]
fn allowlist_empty_is_fail_closed_when_no_otlp() {
    let grants = profile_base_grants(DockerSecurityProfile::Locked);
    let hosts = allowlist_hosts("unknown-agent", &grants, &[], None);
    assert!(
        hosts.is_empty(),
        "no sources + no OTLP yields an empty (DROP-only, fail-closed) allowlist"
    );
}

#[test]
fn firewall_exec_only_for_allowlist_and_runs_as_root() {
    let mut grants = profile_base_grants(DockerSecurityProfile::Hardened);
    // hardened is allowlist by default.
    let argv = firewall_post_run_argv(&grants, "ctr-1").expect("allowlist emits exec");
    assert_eq!(
        argv,
        [
            "exec",
            "--user",
            "root",
            "ctr-1",
            CAPSULE_BIN_PATH,
            "firewall-apply"
        ]
    );
    // open / none emit no firewall.
    grants.network = NetworkGrant::Open;
    assert!(firewall_post_run_argv(&grants, "ctr-1").is_none());
    grants.network = NetworkGrant::None;
    assert!(firewall_post_run_argv(&grants, "ctr-1").is_none());
}

// ── WP0: 8-cap minimum guard (Docker-free) ────────────────────────────────

#[test]
fn minimum_capability_set_is_exactly_eight_expected_caps() {
    // The roadmap's "8-cap minimum" under hardened/locked. Guards against
    // accidental drift of the dropped-to set without needing a container.
    assert_eq!(
        MINIMUM_CAPABILITIES,
        [
            "CHOWN",
            "DAC_OVERRIDE",
            "FOWNER",
            "FSETID",
            "SETUID",
            "SETGID",
            "SETFCAP",
            "KILL",
        ]
    );
}

#[test]
fn hardened_locked_drop_all_then_add_exactly_the_minimum_caps() {
    for profile in [
        DockerSecurityProfile::Hardened,
        DockerSecurityProfile::Locked,
    ] {
        let flags = capability_flags(profile, &[]);
        assert_eq!(flags.first().map(String::as_str), Some("--cap-drop=ALL"));
        let added: Vec<&str> = flags
            .iter()
            .skip_while(|f| f.as_str() != "--cap-add")
            .collect::<Vec<_>>()
            .chunks(2)
            .filter_map(|pair| match pair {
                [flag, cap] if flag.as_str() == "--cap-add" => Some(cap.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            added, MINIMUM_CAPABILITIES,
            "{profile} must add exactly the 8 minimum caps after drop-all"
        );
    }
}

// ── WP-SUDO: runtime sudo provisioning ────────────────────────────────────

#[test]
fn sudo_provision_exec_runs_as_root() {
    assert_eq!(
        sudo_provision_post_run_argv("ctr-9"),
        [
            "exec",
            "--user",
            "root",
            "ctr-9",
            CAPSULE_BIN_PATH,
            "sudo-provision"
        ]
    );
}

#[test]
fn sudo_default_off_outside_compat_on_for_compat() {
    // The grant the launch path turns into JACKIN_SUDO=1.
    assert!(profile_base_grants(DockerSecurityProfile::Compat).sudo);
    assert!(!profile_base_grants(DockerSecurityProfile::Standard).sudo);
    assert!(!profile_base_grants(DockerSecurityProfile::Hardened).sudo);
    assert!(!profile_base_grants(DockerSecurityProfile::Locked).sudo);
}

// ── WP4 Part B: rootless DinD tier ────────────────────────────────────────

#[test]
fn dind_rootless_uses_rootless_image_without_privileged() {
    assert_eq!(
        dind_image_and_privileged(DindGrant::Rootless),
        ("docker:dind-rootless", false)
    );
    assert_eq!(
        dind_image_and_privileged(DindGrant::Privileged),
        ("docker:dind", true)
    );
}

#[test]
fn rootless_dind_fails_closed_on_cgroup_v1() {
    assert!(
        validate_dind_grant_for_cgroup(DindGrant::Rootless, "v1").is_err(),
        "rootless DinD must fail closed on cgroup v1, never fall back to privileged"
    );
    assert!(validate_dind_grant_for_cgroup(DindGrant::Rootless, "v2").is_ok());
    // privileged / none impose no cgroup requirement here.
    assert!(validate_dind_grant_for_cgroup(DindGrant::Privileged, "v1").is_ok());
    assert!(validate_dind_grant_for_cgroup(DindGrant::None, "v1").is_ok());
}

// ── WP2: locked uses a Docker-internal network ────────────────────────────

#[test]
fn role_network_internal_only_for_locked() {
    assert!(
        role_network_internal(DockerSecurityProfile::Locked),
        "locked must run on a Docker-internal network"
    );
    for profile in [
        DockerSecurityProfile::Hardened,
        DockerSecurityProfile::Standard,
        DockerSecurityProfile::Compat,
    ] {
        assert!(
            !role_network_internal(profile),
            "{profile} must use a routable network"
        );
    }
}
