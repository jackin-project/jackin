use super::*;

/// Extending `container_paths` means extending this list — the xtask
/// container-paths gate makes forgetting expensive.
const ALL_PUB_CONSTS: &[(&str, &str)] = &[
    ("JACKIN_ROOT", JACKIN_ROOT),
    ("RUNTIME_DIR", RUNTIME_DIR),
    ("STATE_DIR", STATE_DIR),
    ("RUN_DIR", RUN_DIR),
    ("HOST_DIR", HOST_DIR),
    ("DEFAULT_HOME_DIR", DEFAULT_HOME_DIR),
    ("AMP_DIR", AMP_DIR),
    ("CLAUDE_DIR", CLAUDE_DIR),
    ("CODEX_DIR", CODEX_DIR),
    ("GROK_DIR", GROK_DIR),
    ("OPENCODE_DIR", OPENCODE_DIR),
    ("KIMI_CODE_DIR", KIMI_CODE_DIR),
    ("CAPSULE_BIN", CAPSULE_BIN),
    ("ENTRYPOINT", ENTRYPOINT),
    ("AGENT_STATUS_PACKS_DIR", AGENT_STATUS_PACKS_DIR),
    ("AGENT_STATUS_HOOKS_DIR", AGENT_STATUS_HOOKS_DIR),
    ("CAPSULE_SOCKET", CAPSULE_SOCKET),
    ("HOST_SOCK", HOST_SOCK),
    ("CAPSULE_CONFIG", CAPSULE_CONFIG),
    ("CLIPBOARD_DIR", CLIPBOARD_DIR),
    ("USAGE_ACCOUNTS", USAGE_ACCOUNTS),
    ("GIT_HOOKS_DIR", GIT_HOOKS_DIR),
    ("MULTIPLEXER_LOG", MULTIPLEXER_LOG),
    ("EXIT_ACTION", EXIT_ACTION),
    ("TELEMETRY_STORE", TELEMETRY_STORE),
    ("AGENT_STATUS_CAPTURES_DIR", AGENT_STATUS_CAPTURES_DIR),
    ("CONTAINER_INIT_MARKER", CONTAINER_INIT_MARKER),
    ("GIT_DCO_IDENTITY_CACHE", GIT_DCO_IDENTITY_CACHE),
    ("CLAUDE_CREDENTIALS", CLAUDE_CREDENTIALS),
    ("CLAUDE_ACCOUNT", CLAUDE_ACCOUNT),
    ("CODEX_AUTH", CODEX_AUTH),
    ("AMP_SECRETS", AMP_SECRETS),
    ("OPENCODE_AUTH", OPENCODE_AUTH),
    ("GROK_AUTH", GROK_AUTH),
    ("GIT_HOOK_PREPARE_COMMIT_MSG", GIT_HOOK_PREPARE_COMMIT_MSG),
    (
        "GIT_HOOK_PREPARE_COMMIT_MSG_MARKER",
        GIT_HOOK_PREPARE_COMMIT_MSG_MARKER,
    ),
    ("AGENT_STATUS_CLAUDE_HOOK", AGENT_STATUS_CLAUDE_HOOK),
    ("AGENT_STATUS_CODEX_HOOK", AGENT_STATUS_CODEX_HOOK),
    ("AGENT_STATUS_OPENCODE_PLUGIN", AGENT_STATUS_OPENCODE_PLUGIN),
];

#[test]
fn every_pub_const_starts_with_jackin_root() {
    for (name, value) in ALL_PUB_CONSTS {
        assert!(
            *value == JACKIN_ROOT || value.starts_with(&format!("{JACKIN_ROOT}/")),
            "{name}={value:?} must start with {JACKIN_ROOT}"
        );
    }
}

#[test]
fn no_const_has_dotdot_double_slash_or_trailing_slash() {
    for (name, value) in ALL_PUB_CONSTS {
        assert!(!value.contains(".."), "{name} contains ..");
        assert!(!value.contains("//"), "{name} contains //");
        if *name != "JACKIN_ROOT" {
            // JACKIN_ROOT is "/jackin" without trailing slash already.
        }
        assert!(!value.ends_with('/'), "{name} has trailing slash");
    }
}

#[test]
fn join_composes_under_jackin() {
    let p = join(STATE_DIR, "git-hooks");
    assert_eq!(p, "/jackin/state/git-hooks");
    assert!(p.starts_with(&format!("{JACKIN_ROOT}/")));
    let p2 = join(RUNTIME_DIR, "jackin-capsule");
    assert_eq!(p2, CAPSULE_BIN);
}

#[test]
fn is_jackin_owned_rejects_fhs_roots() {
    assert!(is_jackin_owned("/jackin"));
    assert!(is_jackin_owned("/jackin/run/x"));
    assert!(!is_jackin_owned("/run/x"));
    assert!(!is_jackin_owned("/var/x"));
    assert!(!is_jackin_owned("/etc/x"));
    assert!(!is_jackin_owned("/opt/x"));
    assert!(!is_jackin_owned("/tmp/jackin-x"));
    assert!(!is_jackin_owned("relative"));
}

#[test]
fn is_run_owned_matches_run_prefix() {
    assert!(is_run_owned("/jackin/run"));
    assert!(is_run_owned("/jackin/run/clipboard"));
    assert!(!is_run_owned("/jackin/state"));
    assert!(!is_run_owned("/jackin"));
}
