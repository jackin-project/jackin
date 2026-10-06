// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

#[test]
fn validate_cgroup_compat_accepts_v1() {
    let result = validate_cgroup_for_profile(DockerSecurityProfile::Compat, "v1");
    result.expect("compat must accept cgroup v1");
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
    result.expect_err("hardened must fail-closed on cgroup v1");
}

#[test]
fn validate_cgroup_locked_fails_on_v1() {
    let result = validate_cgroup_for_profile(DockerSecurityProfile::Locked, "v1");
    result.expect_err("locked must fail-closed on cgroup v1");
}

#[test]
fn validate_cgroup_hardened_accepts_v2() {
    let result = validate_cgroup_for_profile(DockerSecurityProfile::Hardened, "v2");
    result.expect("hardened must accept cgroup v2");
}

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

#[test]
fn dind_rootless_uses_rootless_image_without_privileged() {
    assert_eq!(
        dind_image_and_privileged(DindGrant::Rootless),
        ("docker:29-dind-rootless", false)
    );
    assert_eq!(
        dind_image_and_privileged(DindGrant::Privileged),
        ("docker:29-dind", true)
    );
}

#[test]
fn rootless_dind_fails_closed_on_cgroup_v1() {
    validate_dind_grant_for_cgroup(DindGrant::Rootless, "v1")
        .expect_err("rootless DinD must fail closed on cgroup v1, never fall back to privileged");
    validate_dind_grant_for_cgroup(DindGrant::Rootless, "v2").unwrap();
    // privileged / none impose no cgroup requirement here.
    validate_dind_grant_for_cgroup(DindGrant::Privileged, "v1").unwrap();
    validate_dind_grant_for_cgroup(DindGrant::None, "v1").unwrap();
}

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
