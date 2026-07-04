// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn open_links_allowed_accepts_unset_and_non_deny_values() {
    assert!(open_links_allowed(None));
    assert!(open_links_allowed(Some("")));
    assert!(open_links_allowed(Some("allow")));
    assert!(open_links_allowed(Some("yes")));
}

#[test]
fn open_links_allowed_rejects_deny_values() {
    for value in ["deny", "off", "no"] {
        assert!(!open_links_allowed(Some(value)));
    }
}

#[test]
fn reserved_runtime_env_vars_covers_every_previously_reserved_name() {
    // Each name previously in manifest::RESERVED_RUNTIME_ENV_VARS
    // AND in runtime's old RUNTIME_OWNED_ENV_VARS must be present.
    let names: Vec<&str> = RESERVED_RUNTIME_ENV_VARS.iter().map(|(n, _)| *n).collect();
    for sentinel in &[
        "JACKIN",               // in-container sentinel (was JACKIN)
        "JACKIN_DIND_HOSTNAME", // was manifest JACKIN_DIND_HOSTNAME_ENV_NAME value
        "JACKIN_CONTAINER_NAME",
        "JACKIN_INSTANCE_ID",
        "JACKIN_AGENT", // injected per agent session — agent slug (claude/codex/amp)
        "JACKIN_AGENT_CODENAME", // unique per-tab codename, never reused in container lifetime
        "JACKIN_ROLE",  // runtime-owned role selector key
        "JACKIN_GIT_COAUTHOR_TRAILER",
        "JACKIN_GIT_DCO",
        "DOCKER_HOST",
        "DOCKER_TLS_VERIFY",
        "DOCKER_CERT_PATH",
        "TESTCONTAINERS_HOST_OVERRIDE",
    ] {
        assert!(
            names.contains(sentinel),
            "reserved env list must include {sentinel} for previous manifest/runtime coverage"
        );
    }
}

#[test]
fn is_reserved_accepts_all_sentinel_names() {
    for sentinel in &[
        "JACKIN",
        "JACKIN_DIND_HOSTNAME",
        "JACKIN_CONTAINER_NAME",
        "JACKIN_INSTANCE_ID",
        "JACKIN_AGENT",
        "JACKIN_AGENT_CODENAME",
        "JACKIN_ROLE",
        "JACKIN_GIT_COAUTHOR_TRAILER",
        "JACKIN_GIT_DCO",
        "DOCKER_HOST",
        "DOCKER_TLS_VERIFY",
        "DOCKER_CERT_PATH",
        "TESTCONTAINERS_HOST_OVERRIDE",
    ] {
        assert!(
            is_reserved(sentinel),
            "{sentinel} must be recognized as reserved"
        );
    }
}

#[test]
fn is_reserved_rejects_user_names() {
    assert!(!is_reserved("MY_USER_VAR"));
    assert!(!is_reserved("PATH"));
    assert!(!is_reserved(""));
}

#[test]
fn jackin_git_dco_is_reserved() {
    assert!(
        is_reserved(JACKIN_GIT_DCO_ENV_NAME),
        "JACKIN_GIT_DCO must be reserved so manifests cannot override the DCO hook signal"
    );
}

#[test]
fn extract_interpolation_refs_finds_single_ref() {
    assert_eq!(
        extract_interpolation_refs("Branch for ${env.PROJECT}:"),
        vec!["PROJECT"]
    );
}

#[test]
fn extract_interpolation_refs_finds_multiple_refs() {
    assert_eq!(
        extract_interpolation_refs("${env.TEAM}/${env.PROJECT}"),
        vec!["TEAM", "PROJECT"]
    );
}

#[test]
fn extract_interpolation_refs_returns_empty_for_no_refs() {
    assert!(extract_interpolation_refs("plain text").is_empty());
}

#[test]
fn extract_interpolation_refs_ignores_non_env_namespace() {
    assert!(extract_interpolation_refs("${other.FOO}").is_empty());
    assert!(extract_interpolation_refs("${FOO}").is_empty());
}

#[test]
fn extract_interpolation_refs_returns_empty_name_for_empty_env_ref() {
    assert_eq!(extract_interpolation_refs("${env.}"), vec![""]);
}

#[test]
fn extract_interpolation_refs_handles_unclosed_brace() {
    assert!(extract_interpolation_refs("${env.OPEN").is_empty());
}

#[test]
fn topological_env_order_is_deterministic_for_independent_prompts() {
    fn decl(depends_on: &[&str]) -> crate::manifest::EnvVarDecl {
        crate::manifest::EnvVarDecl {
            default_value: None,
            interactive: true,
            skippable: false,
            prompt: None,
            options: Vec::new(),
            depends_on: depends_on.iter().map(|dep| (*dep).to_owned()).collect(),
        }
    }

    let declarations = std::collections::BTreeMap::from([
        ("BRANCH".to_owned(), decl(&["env.SELECT_PROJECT"])),
        ("FREE_TEXT".to_owned(), decl(&[])),
        ("SELECT_PROJECT".to_owned(), decl(&[])),
    ]);

    assert_eq!(
        topological_env_order(&declarations).unwrap(),
        ["FREE_TEXT", "SELECT_PROJECT", "BRANCH"]
    );
}
