use super::*;
use std::path::Path;

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
    ("DIND_CERTS_CLIENT_DIR", DIND_CERTS_CLIENT_DIR),
    ("CAPSULE_CONFIG", CAPSULE_CONFIG),
    ("CLIPBOARD_DIR", CLIPBOARD_DIR),
    ("USAGE_ACCOUNTS", USAGE_ACCOUNTS),
    ("USAGE_RELAY_V2_DIR", USAGE_RELAY_V2_DIR),
    ("USAGE_RELAY_V2_SOCK", USAGE_RELAY_V2_SOCK),
    ("USAGE_RELAY_V2_LOCK", USAGE_RELAY_V2_LOCK),
    ("GIT_HOOKS_DIR", GIT_HOOKS_DIR),
    ("EXIT_ACTION", EXIT_ACTION),
    ("USAGE_SNAPSHOT_STORE", USAGE_SNAPSHOT_STORE),
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
    assert!(!is_jackin_owned("/jackin/../home"));
    assert!(!is_jackin_owned("/jackin-other"));
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
    assert!(!is_run_owned("/jackin/run/../state"));
    assert!(!is_run_owned("/jackin/runtime"));
    assert!(!is_run_owned("/jackin/state"));
    assert!(!is_run_owned("/jackin"));
}

#[test]
fn path_relations_normalize_components_without_prefix_collisions() {
    assert!(path_is_ancestor_or_equal(
        Path::new("/workspace/project"),
        Path::new("/workspace/project/../project/src"),
    ));
    assert!(paths_overlap(Path::new("/home"), Path::new("/home/agent"),));
    assert!(!path_is_ancestor_or_equal(
        Path::new("/workspace/project"),
        Path::new("/workspace/project-v2"),
    ));
}

const ORIGINAL_PATH_BYTES: &[(&str, &str)] = &[
    (JACKIN_ROOT, "/jackin"),
    (RUNTIME_DIR, "/jackin/runtime"),
    (STATE_DIR, "/jackin/state"),
    (RUN_DIR, "/jackin/run"),
    (SESSION_ROOTS_DIR, "/jackin/run/sessions"),
    (HOST_DIR, "/jackin/host"),
    (DEFAULT_HOME_DIR, "/jackin/default-home"),
    (AMP_DIR, "/jackin/amp"),
    (CLAUDE_DIR, "/jackin/claude"),
    (CODEX_DIR, "/jackin/codex"),
    (GROK_DIR, "/jackin/grok"),
    (OPENCODE_DIR, "/jackin/opencode"),
    (KIMI_CODE_DIR, "/jackin/kimi-code"),
    (ANTIGRAVITY_DIR, "/jackin/antigravity"),
    (GEMINI_DIR, "/jackin/gemini"),
    (CURSOR_DIR, "/jackin/cursor"),
    (MUSE_DIR, "/jackin/muse"),
    (OMP_DIR, "/jackin/omp"),
    (HERMES_DIR, "/jackin/hermes"),
    (CAPSULE_BIN, "/jackin/runtime/jackin-capsule"),
    (ENTRYPOINT, "/jackin/runtime/entrypoint.sh"),
    (AGENT_STATUS_PACKS_DIR, "/jackin/runtime/agent-status/packs"),
    (AGENT_STATUS_HOOKS_DIR, "/jackin/runtime/agent-status/hooks"),
    (CAPSULE_SOCKET, "/jackin/run/jackin.sock"),
    (HOST_SOCK, "/jackin/run/host.sock"),
    (USAGE_SOCK, "/jackin/run/usage.sock"),
    (DIND_CERTS_CLIENT_DIR, "/jackin/run/dind-certs/client"),
    (CAPSULE_CONFIG, "/jackin/run/agent.toml"),
    (CLIPBOARD_DIR, "/jackin/run/clipboard"),
    (USAGE_ACCOUNTS, "/jackin/run/usage/accounts.json"),
    (GIT_HOOKS_DIR, "/jackin/state/git-hooks"),
    (EXIT_ACTION, "/jackin/state/exit-action.json"),
    (USAGE_SNAPSHOT_STORE, "/jackin/state/usage/snapshots.db"),
    (
        AGENT_STATUS_CAPTURES_DIR,
        "/jackin/state/agent-status/captures",
    ),
    (CONTAINER_INIT_MARKER, "/jackin/state/container-init.done"),
    (GIT_DCO_IDENTITY_CACHE, "/jackin/state/git-dco-identity"),
    (CLAUDE_CREDENTIALS, "/jackin/claude/credentials.json"),
    (CLAUDE_ACCOUNT, "/jackin/claude/account.json"),
    (CODEX_AUTH, "/jackin/codex/auth.json"),
    (AMP_SECRETS, "/jackin/amp/secrets.json"),
    (OPENCODE_AUTH, "/jackin/opencode/auth.json"),
    (GROK_AUTH, "/jackin/grok/auth.json"),
    (ANTIGRAVITY_SETTINGS, "/jackin/antigravity/settings.json"),
    (GEMINI_AUTH, "/jackin/gemini/oauth_creds.json"),
    (CURSOR_AUTH, "/jackin/cursor/auth.json"),
    (MUSE_AUTH, "/jackin/muse/auth.json"),
    (OMP_AGENT_DB, "/jackin/omp/agent.db"),
    (HERMES_AUTH, "/jackin/hermes/auth.json"),
    (
        GIT_HOOK_PREPARE_COMMIT_MSG,
        "/jackin/state/git-hooks/prepare-commit-msg",
    ),
    (
        GIT_HOOK_PREPARE_COMMIT_MSG_MARKER,
        "/jackin/state/git-hooks/prepare-commit-msg.v3.done",
    ),
    (
        AGENT_STATUS_CLAUDE_HOOK,
        "/jackin/runtime/agent-status/hooks/claude/report-hook.sh",
    ),
    (
        AGENT_STATUS_CODEX_HOOK,
        "/jackin/runtime/agent-status/hooks/codex/report-hook.sh",
    ),
    (
        AGENT_STATUS_OPENCODE_PLUGIN,
        "/jackin/runtime/agent-status/hooks/opencode/plugin.js",
    ),
];

#[test]
fn canonical_root_migration_preserves_all_existing_path_bytes() {
    assert_eq!(ORIGINAL_PATH_BYTES.len(), 53);
    for (actual, expected) in ORIGINAL_PATH_BYTES {
        assert_eq!(actual, expected);
    }
    assert_eq!(USAGE_RELAY_V2_DIR, "/jackin/usage-relay-v2");
    assert_eq!(USAGE_RELAY_V2_SOCK, "/jackin/usage-relay-v2/usage.sock");
    assert_eq!(USAGE_RELAY_V2_LOCK, "/jackin/usage-relay-v2/owner.lock");
    assert_eq!(CHECKS_PREFIX, "/jackin/checks/");
    assert_eq!(ACCOUNT_CREDENTIALS_DIR, "/jackin/account-credentials");
}

#[test]
fn system_paths_retain_distro_paths_and_sudoers_options() {
    assert_eq!(SYSTEM_ETC_DIR, "/etc");
    assert_eq!(SYSTEM_VAR_DIR, "/var");
    assert_eq!(SYSTEM_RUN_DIR, "/run");
    assert_eq!(SYSTEM_VAR_RUN_DIR, "/var/run");
    assert_eq!(SYSTEM_VAR_TMP_DIR, "/var/tmp");
    assert_eq!(SYSTEM_VAR_CACHE_DIR, "/var/cache");
    assert_eq!(SYSTEM_VAR_LOG_DIR, "/var/log");
    assert_eq!(SYSTEM_APT_LISTS_DIR, "/var/lib/apt/lists");
    assert_eq!(SYSTEM_APT_ARCHIVES_DIR, "/var/cache/apt/archives");
    assert_eq!(SYSTEM_DPKG_DIR, "/var/lib/dpkg");
    assert_eq!(
        SUDOERS_TMPFS_SPEC,
        "/etc/sudoers.d:rw,nosuid,nodev,mode=0755"
    );
}
