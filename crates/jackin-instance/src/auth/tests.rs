//! Tests for `instance/auth` — tests.
use super::{Agent, AuthProvisionOutcome, RoleState, validate_sync_source_dir};
use crate::PrepareResolvers;
use jackin_config::AuthForwardMode;
use jackin_core::paths::JackinPaths;
use tempfile::tempdir;

/// The macOS Keychain service name Claude Code derives for a custom
/// `CLAUDE_CONFIG_DIR` must match the live entries observed on disk:
/// the default `~/.claude` uses the bare service, and any other config
/// dir uses `Claude Code-credentials-<sha256(path)[..8]>`. Pure string
/// derivation — no real Keychain access.
#[cfg(target_os = "macos")]
#[test]
fn claude_keychain_service_name_matches_claude_scheme() {
    use std::path::Path;
    let home = Path::new("/Users/donbeave");

    assert_eq!(
        super::claude_keychain_service_for_config_dir(&home.join(".claude"), home),
        "Claude Code-credentials"
    );
    assert_eq!(
        super::claude_keychain_service_for_config_dir(&home.join(".claude-chainargos"), home),
        "Claude Code-credentials-93aecf3d"
    );
    assert_eq!(
        super::claude_keychain_service_for_config_dir(&home.join(".claude-work"), home),
        "Claude Code-credentials-3342f2c7"
    );
}

const TEST_CREDENTIALS: &str = r#"{"claudeAiOauth":{"accessToken":"test","refreshToken":"test"}}"#;

// ── Source-folder validation ────────────────────────────────────────

#[test]
fn validate_rejects_non_directory() {
    let temp = tempdir().unwrap();
    let missing = temp.path().join("nope");
    validate_sync_source_dir(Agent::Codex, &missing, temp.path()).unwrap_err();
}

#[test]
fn validate_claude_accepts_file_credentials_rejects_bare_folder() {
    let temp = tempdir().unwrap();
    let good = temp.path().join("claude-good");
    std::fs::create_dir_all(&good).unwrap();
    std::fs::write(good.join(".credentials.json"), TEST_CREDENTIALS).unwrap();
    validate_sync_source_dir(Agent::Claude, &good, temp.path()).unwrap();

    // No .credentials.json file; host_home is a temp dir so the macOS
    // Keychain probe is skipped — must be rejected, not accepted.
    let bare = temp.path().join("claude-bare");
    std::fs::create_dir_all(&bare).unwrap();
    let err = validate_sync_source_dir(Agent::Claude, &bare, temp.path()).unwrap_err();
    assert!(err.contains("Claude"), "msg should name the agent: {err}");
}

#[test]
fn validate_single_file_agents() {
    let temp = tempdir().unwrap();
    for (agent, name) in [
        (Agent::Codex, "auth.json"),
        (Agent::Grok, "auth.json"),
        (Agent::Opencode, "auth.json"),
        (Agent::Amp, "secrets.json"),
    ] {
        let dir = temp.path().join(format!("{agent:?}-good"));
        std::fs::create_dir_all(&dir).unwrap();
        // Empty file is rejected.
        std::fs::write(dir.join(name), "").unwrap();
        validate_sync_source_dir(agent, &dir, temp.path()).expect_err(&format!("empty {name} must be rejected"));
        // Non-empty credential file is accepted.
        std::fs::write(dir.join(name), "{\"token\":\"x\"}").unwrap();
        validate_sync_source_dir(agent, &dir, temp.path()).expect(&format!("valid {name} must be accepted"));
        // Wrong folder (no credential file) is rejected.
        let bad = temp.path().join(format!("{agent:?}-bad"));
        std::fs::create_dir_all(&bad).unwrap();
        validate_sync_source_dir(agent, &bad, temp.path()).unwrap_err();
    }
}

#[test]
fn validate_kimi_requires_config_and_credentials_tree() {
    let temp = tempdir().unwrap();
    let good = temp.path().join("kimi-good");
    std::fs::create_dir_all(good.join("credentials")).unwrap();
    std::fs::write(good.join("config.toml"), "x = 1\n").unwrap();
    validate_sync_source_dir(Agent::Kimi, &good, temp.path()).unwrap();

    // config.toml present but no credentials/ dir → rejected.
    let bad = temp.path().join("kimi-bad");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(bad.join("config.toml"), "x = 1\n").unwrap();
    validate_sync_source_dir(Agent::Kimi, &bad, temp.path()).unwrap_err();
}

/// Set up a fake host auth environment in the temp dir.
fn seed_host_auth(temp: &tempfile::TempDir) {
    std::fs::write(
        temp.path().join(".claude.json"),
        r#"{"oauthAccount":{"emailAddress":"test@example.com"}}"#,
    )
    .unwrap();
    let creds_dir = temp.path().join(".claude");
    std::fs::create_dir_all(&creds_dir).unwrap();
    std::fs::write(creds_dir.join(".credentials.json"), TEST_CREDENTIALS).unwrap();
}

fn simple_manifest(temp: &tempfile::TempDir) -> jackin_manifest::RoleManifest {
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    jackin_manifest::load_role_manifest(temp.path()).unwrap()
}

// ── Auth forwarding tests ───────────────────────────────────────────

// ── Auth forwarding tests ───────────────────────────────────────────

#[test]
fn ignore_mode_skips_state_when_absent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Ignore,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    // Ignore mode with no prior jackin-owned state provisions nothing: it does
    // not copy host auth and does not write an empty `{}` skeleton, so no
    // jackin-owned state is created for the container to mount. The agent falls
    // back to the image's credential-free default-home — the same no-auth
    // outcome as an explicit `{}`, without an empty bind source. Stale existing
    // jackin-owned state still enters the normal wipe path.
    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(!state.claude_account_json().unwrap().exists());
    assert!(!state.claude_credentials_json().unwrap().exists());
}

#[test]
fn sync_mode_copies_host_auth_on_first_run() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    assert!(
        std::fs::read_to_string(state.claude_account_json().unwrap())
            .unwrap()
            .contains("test@example.com")
    );
    assert_eq!(
        std::fs::read_to_string(state.claude_credentials_json().unwrap()).unwrap(),
        TEST_CREDENTIALS
    );
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
}

#[test]
fn copy_host_claude_json_copies_present_file() {
    let temp = tempdir().unwrap();
    let host = temp.path().join(".claude.json");
    let dest_dir = temp.path().join("state");
    let dest = dest_dir.join(".claude.json");
    std::fs::create_dir_all(&dest_dir).unwrap();
    std::fs::write(
        &host,
        r#"{"oauthAccount":{"emailAddress":"test@example.com"}}"#,
    )
    .unwrap();

    super::copy_host_claude_json(&host, &dest).unwrap();

    assert!(
        std::fs::read_to_string(dest)
            .unwrap()
            .contains("test@example.com")
    );
}

#[test]
fn copy_host_claude_json_writes_empty_object_when_absent() {
    let temp = tempdir().unwrap();
    let host = temp.path().join("missing.claude.json");
    let dest_dir = temp.path().join("state");
    let dest = dest_dir.join(".claude.json");
    std::fs::create_dir_all(&dest_dir).unwrap();

    super::copy_host_claude_json(&host, &dest).unwrap();

    assert_eq!(std::fs::read_to_string(dest).unwrap(), "{}");
}

#[test]
fn copy_host_claude_json_propagates_read_errors_without_writing_empty_object() {
    let temp = tempdir().unwrap();
    let host = temp.path().join(".claude.json");
    let dest_dir = temp.path().join("state");
    let dest = dest_dir.join(".claude.json");
    std::fs::create_dir_all(&host).unwrap();
    std::fs::create_dir_all(&dest_dir).unwrap();

    let err = super::copy_host_claude_json(&host, &dest).unwrap_err();

    assert!(
        err.to_string().contains("reading Claude account metadata"),
        "error should preserve context: {err}"
    );
    assert!(
        !dest.exists(),
        "read errors must not write a synthetic empty account file"
    );
}

#[test]
fn sync_source_dir_copies_claude_config_dir_without_nested_home_layout() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let source_dir = temp.path().join("claude-work");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(
        source_dir.join(".claude.json"),
        r#"{"oauthAccount":{"emailAddress":"workspace@example.com"}}"#,
    )
    .unwrap();
    std::fs::write(source_dir.join(".credentials.json"), TEST_CREDENTIALS).unwrap();
    let manifest = simple_manifest(&temp);

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| Some(source_dir.clone()),
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    assert!(
        std::fs::read_to_string(state.claude_account_json().unwrap())
            .unwrap()
            .contains("workspace@example.com")
    );
    assert_eq!(
        std::fs::read_to_string(state.claude_credentials_json().unwrap()).unwrap(),
        TEST_CREDENTIALS
    );
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
}

/// Regression: an explicit Claude source folder with no readable
/// credentials must NOT fall back to the default host `~/.claude`
/// credentials. The operator selected a specific config dir (e.g. an
/// Enterprise account); leaking the default Max account into the capsule
/// is the bug this guards against. Expect `HostMissing`, not `Synced`.
#[test]
fn sync_source_dir_does_not_fall_back_to_default_host_credentials() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let source_dir = temp.path().join("claude-work");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(
        source_dir.join(".claude.json"),
        r#"{"oauthAccount":{"emailAddress":"workspace@example.com"}}"#,
    )
    .unwrap();
    // Default host credentials exist — but the source folder has none, so
    // these must be ignored rather than leaked into the capsule.
    std::fs::create_dir_all(temp.path().join(".claude")).unwrap();
    std::fs::write(
        temp.path().join(".claude/.credentials.json"),
        TEST_CREDENTIALS,
    )
    .unwrap();
    let manifest = simple_manifest(&temp);

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| Some(source_dir.clone()),
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    // No fallback: the default host account never reaches the capsule.
    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    let creds = state
        .claude_credentials_json()
        .and_then(|p| std::fs::read_to_string(p).ok());
    assert!(
        creds.as_deref() != Some(TEST_CREDENTIALS),
        "default host credentials must not leak into an explicit source folder"
    );
}

/// An explicit source folder that DOES carry its own file-based
/// credentials syncs them straight through.
#[test]
fn sync_source_dir_uses_source_folder_own_credentials() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let source_dir = temp.path().join("claude-chainargos");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(
        source_dir.join(".claude.json"),
        r#"{"oauthAccount":{"emailAddress":"enterprise@chainargos.com"}}"#,
    )
    .unwrap();
    let source_creds =
        r#"{"claudeAiOauth":{"accessToken":"enterprise","refreshToken":"enterprise"}}"#;
    std::fs::write(source_dir.join(".credentials.json"), source_creds).unwrap();
    // A different default host account is present and must be ignored.
    std::fs::create_dir_all(temp.path().join(".claude")).unwrap();
    std::fs::write(
        temp.path().join(".claude/.credentials.json"),
        TEST_CREDENTIALS,
    )
    .unwrap();
    let manifest = simple_manifest(&temp);

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| Some(source_dir.clone()),
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(
        std::fs::read_to_string(state.claude_account_json().unwrap())
            .unwrap()
            .contains("enterprise@chainargos.com")
    );
    assert_eq!(
        std::fs::read_to_string(state.claude_credentials_json().unwrap()).unwrap(),
        source_creds,
        "source folder credentials must win over the default host account"
    );
}

/// An empty `.credentials.json` in the source folder must be treated as
/// absent, never as valid credentials: an empty file must not provision the
/// capsule with blank creds (booting the agent unauthenticated with no
/// signal) nor let the default host account leak in as a fallback.
#[test]
fn sync_source_dir_empty_credentials_file_is_not_treated_as_valid() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let source_dir = temp.path().join("claude-empty");
    std::fs::create_dir_all(&source_dir).unwrap();
    // Present but blank — the bug guard: whitespace-only must not count.
    std::fs::write(source_dir.join(".credentials.json"), "   \n").unwrap();
    // A different default host account is present and must never leak in.
    std::fs::create_dir_all(temp.path().join(".claude")).unwrap();
    std::fs::write(
        temp.path().join(".claude/.credentials.json"),
        TEST_CREDENTIALS,
    )
    .unwrap();
    let manifest = simple_manifest(&temp);

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| Some(source_dir.clone()),
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    assert_eq!(
        outcome,
        AuthProvisionOutcome::HostMissing,
        "an empty source credentials file must not resolve as Synced"
    );
    if let Some(creds_json) = state.claude_credentials_json() {
        let written = std::fs::read_to_string(creds_json).unwrap_or_default();
        assert!(
            !written.contains("accessToken"),
            "no credentials (default host account included) may be written when \
             the source file is empty"
        );
    }
}

#[test]
fn sync_mode_falls_back_to_empty_json_when_host_has_none() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    // No host auth seeded
    let manifest = simple_manifest(&temp);

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(state.claude_account_json().unwrap()).unwrap(),
        "{}"
    );
    assert!(!state.claude_credentials_json().unwrap().exists());
    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
}

#[test]
fn sync_source_dir_copies_direct_opencode_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let source_dir = temp.path().join("opencode-work");
    std::fs::create_dir_all(&source_dir).unwrap();
    let expected = r#"{"provider":{"credential":"workspace"}}"#;
    std::fs::write(source_dir.join("auth.json"), expected).unwrap();

    let (outcome, mounted) = RoleState::provision_opencode_auth_from_source_dir(
        &auth_json,
        AuthForwardMode::Sync,
        &source_dir,
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(auth_json.as_path()));
    assert_eq!(std::fs::read_to_string(&auth_json).unwrap(), expected);
}

#[test]
fn sync_source_dir_copies_direct_grok_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let source_dir = temp.path().join("grok-work");
    std::fs::create_dir_all(&source_dir).unwrap();
    let expected = r#"{"https://auth.x.ai::workspace":{"key":"jwt"}}"#;
    std::fs::write(source_dir.join("auth.json"), expected).unwrap();

    let (outcome, mounted) = RoleState::provision_grok_auth_from_source_dir(
        &auth_json,
        AuthForwardMode::Sync,
        &source_dir,
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(auth_json.as_path()));
    assert_eq!(std::fs::read_to_string(&auth_json).unwrap(), expected);
}

#[test]
fn sync_refreshes_changed_single_file_provider_credentials() {
    let temp = tempdir().unwrap();

    let codex_target = temp.path().join("codex-auth.json");
    let codex_source = temp.path().join("codex-source");
    std::fs::create_dir_all(&codex_source).unwrap();
    std::fs::write(codex_source.join("auth.json"), r#"{"token":"old-codex"}"#).unwrap();
    let (outcome, mounted) = RoleState::provision_codex_auth_from_source_dir(
        &codex_target,
        AuthForwardMode::Sync,
        &codex_source,
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(codex_target.as_path()));
    std::fs::write(codex_source.join("auth.json"), r#"{"token":"new-codex"}"#).unwrap();
    let (outcome, mounted) = RoleState::provision_codex_auth_from_source_dir(
        &codex_target,
        AuthForwardMode::Sync,
        &codex_source,
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(codex_target.as_path()));
    assert_eq!(
        std::fs::read_to_string(&codex_target).unwrap(),
        r#"{"token":"new-codex"}"#
    );

    let amp_target = temp.path().join("secrets.json");
    let amp_source = temp.path().join("amp-source");
    std::fs::create_dir_all(&amp_source).unwrap();
    std::fs::write(amp_source.join("secrets.json"), r#"{"amp":"old"}"#).unwrap();
    RoleState::provision_amp_auth_from_source_dir(&amp_target, AuthForwardMode::Sync, &amp_source)
        .unwrap();
    std::fs::write(amp_source.join("secrets.json"), r#"{"amp":"new"}"#).unwrap();
    let (outcome, mounted) = RoleState::provision_amp_auth_from_source_dir(
        &amp_target,
        AuthForwardMode::Sync,
        &amp_source,
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(amp_target.as_path()));
    assert_eq!(
        std::fs::read_to_string(&amp_target).unwrap(),
        r#"{"amp":"new"}"#
    );

    let grok_target = temp.path().join("grok-auth.json");
    let grok_source = temp.path().join("grok-source");
    std::fs::create_dir_all(&grok_source).unwrap();
    std::fs::write(grok_source.join("auth.json"), r#"{"grok":"old"}"#).unwrap();
    RoleState::provision_grok_auth_from_source_dir(
        &grok_target,
        AuthForwardMode::Sync,
        &grok_source,
    )
    .unwrap();
    std::fs::write(grok_source.join("auth.json"), r#"{"grok":"new"}"#).unwrap();
    let (outcome, mounted) = RoleState::provision_grok_auth_from_source_dir(
        &grok_target,
        AuthForwardMode::Sync,
        &grok_source,
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(grok_target.as_path()));
    assert_eq!(
        std::fs::read_to_string(&grok_target).unwrap(),
        r#"{"grok":"new"}"#
    );
}

#[test]
fn sync_mode_overwrites_existing() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);

    // First run with host auth
    seed_host_auth(&temp);
    let (state, outcome1) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();
    assert_eq!(outcome1, AuthProvisionOutcome::Synced);

    // Simulate container modifying its own .claude.json
    std::fs::write(
        state.claude_account_json().unwrap(),
        r#"{"container":"data"}"#,
    )
    .unwrap();

    // Update host credentials
    let updated_creds = r#"{"claudeAiOauth":{"accessToken":"new","refreshToken":"new"}}"#;
    std::fs::write(temp.path().join(".claude/.credentials.json"), updated_creds).unwrap();

    // Second run: should overwrite with host content
    let (state2, outcome2) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(state2.claude_credentials_json().unwrap()).unwrap(),
        updated_creds
    );
    assert_eq!(outcome2, AuthProvisionOutcome::Synced);
}

// ── Mode transition tests ───────────────────────────────────────────

#[test]
fn switching_from_sync_to_ignore_revokes_forwarded_credentials() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    // First run: sync mode writes credentials
    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();
    assert!(state.claude_credentials_json().unwrap().exists());

    // Operator switches to ignore — credentials must be wiped
    let (state2, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Ignore,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(state2.claude_account_json().unwrap()).unwrap(),
        "{}"
    );
    assert!(!state2.claude_credentials_json().unwrap().exists());
}

#[test]
fn token_mode_writes_onboarding_skeleton_and_no_credentials() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    // Seed host auth — token mode must NOT copy it.
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::OAuthToken,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    // Skeleton tells Claude CLI to skip the interactive login wizard;
    // actual auth comes from CLAUDE_CODE_OAUTH_TOKEN in the env.
    assert_eq!(
        std::fs::read_to_string(state.claude_account_json().unwrap()).unwrap(),
        r#"{"hasCompletedOnboarding":true}"#
    );
    assert!(
        !state.claude_credentials_json().unwrap().exists(),
        "token mode must not write .credentials.json"
    );
    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
}

/// `ApiKey` shares the wipe-state contract with `OAuthToken` (both
/// env-driven modes) but is dispatched as a distinct enum variant —
/// pin its filesystem behavior independently so a future per-mode
/// split can't silently break the `ApiKey` path. The pre-seeded
/// `.credentials.json` here doubles as a "switching from sync to
/// `ApiKey` revokes forwarded creds" assertion: the file existed
/// before the `ApiKey` run and must be gone after.
#[test]
fn api_key_mode_wipes_credentials_and_writes_empty_json() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    // Seed host auth — api_key mode must NOT copy it.
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    // First run: sync mode writes credentials we'll then need to verify
    // get wiped under api_key.
    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();
    assert!(
        state.claude_credentials_json().unwrap().exists(),
        "precondition: sync seeded .credentials.json"
    );

    let (state2, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::ApiKey,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(state2.claude_account_json().unwrap()).unwrap(),
        "{}",
        "api_key mode must reset .claude.json to empty object"
    );
    assert!(
        !state2.claude_credentials_json().unwrap().exists(),
        "api_key mode must wipe .credentials.json"
    );
    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
}

#[test]
fn switching_from_sync_to_token_revokes_forwarded_credentials() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    // First run: sync mode writes credentials
    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();
    assert!(state.claude_credentials_json().unwrap().exists());

    // Operator switches to token — credentials must be wiped and
    // .claude.json reset to skeleton so Claude Code skips the login
    // wizard and authenticates exclusively via CLAUDE_CODE_OAUTH_TOKEN.
    let (state2, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::OAuthToken,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(state2.claude_account_json().unwrap()).unwrap(),
        r#"{"hasCompletedOnboarding":true}"#
    );
    assert!(!state2.claude_credentials_json().unwrap().exists());
    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
}

#[test]
fn switching_from_token_to_sync_forwards_fresh_host_creds() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    // First run: token mode writes the onboarding skeleton
    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::OAuthToken,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(state.claude_account_json().unwrap()).unwrap(),
        r#"{"hasCompletedOnboarding":true}"#
    );

    // Operator switches to sync — host auth must now be forwarded
    let (state2, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();
    assert!(
        std::fs::read_to_string(state2.claude_account_json().unwrap())
            .unwrap()
            .contains("test@example.com")
    );
    assert_eq!(
        std::fs::read_to_string(state2.claude_credentials_json().unwrap()).unwrap(),
        TEST_CREDENTIALS
    );
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
}

#[test]
fn switching_from_token_to_ignore_remains_empty() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    // Token mode seeds an empty state
    let (_, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::OAuthToken,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    // Switching to ignore must keep the empty shape (no .credentials.json)
    let (state2, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Ignore,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(state2.claude_account_json().unwrap()).unwrap(),
        "{}"
    );
    assert!(!state2.claude_credentials_json().unwrap().exists());
    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
}

#[test]
fn sync_mode_preserves_container_auth_when_host_file_missing() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);

    // First run: host has auth, sync copies it
    seed_host_auth(&temp);
    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    // Host auth disappears (e.g. user logged out)
    std::fs::remove_file(temp.path().join(".claude.json")).unwrap();
    std::fs::remove_file(temp.path().join(".claude/.credentials.json")).unwrap();

    // Container may have its own auth by now (from manual login inside)
    let container_auth = r#"{"oauthAccount":{"emailAddress":"container@example.com"}}"#;
    std::fs::write(state.claude_account_json().unwrap(), container_auth).unwrap();

    // Second run: host auth missing — container auth must be preserved
    let (state2, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(state2.claude_account_json().unwrap()).unwrap(),
        container_auth
    );
    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
}

#[cfg(unix)]
#[test]
fn auth_file_has_restricted_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    let perms = std::fs::metadata(state.claude_account_json().unwrap())
        .unwrap()
        .permissions();
    assert_eq!(
        perms.mode() & 0o777,
        0o600,
        "claude.json should have 0600 permissions"
    );
    let creds_perms = std::fs::metadata(state.claude_credentials_json().unwrap())
        .unwrap()
        .permissions();
    assert_eq!(
        creds_perms.mode() & 0o777,
        0o600,
        ".credentials.json should have 0600 permissions"
    );
}

#[cfg(unix)]
#[test]
fn sync_repairs_permissions_on_legacy_permissive_file() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);
    seed_host_auth(&temp);

    // First run: sync host auth so the jackin-owned account.json exists at 0600.
    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    // Simulate a legacy state file with permissive mode
    std::fs::set_permissions(
        state.claude_account_json().unwrap(),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let perms = std::fs::metadata(state.claude_account_json().unwrap())
        .unwrap()
        .permissions();
    assert_eq!(perms.mode() & 0o777, 0o644, "precondition: file is 0644");

    // A subsequent sync must tighten permissions back to 0600.
    let (state2, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    let perms = std::fs::metadata(state2.claude_account_json().unwrap())
        .unwrap()
        .permissions();
    assert_eq!(
        perms.mode() & 0o777,
        0o600,
        "sync should repair permissions on existing file"
    );
}

#[cfg(unix)]
#[test]
fn sync_repairs_permissions_when_host_auth_missing() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);

    // First run: sync with host auth to seed both files
    seed_host_auth(&temp);
    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    // Simulate legacy permissive modes on both auth files
    std::fs::set_permissions(
        state.claude_account_json().unwrap(),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let creds_path = state.claude_credentials_json().unwrap();
    std::fs::set_permissions(creds_path, std::fs::Permissions::from_mode(0o644)).unwrap();

    // Remove host auth so sync takes the preserve path
    std::fs::remove_file(temp.path().join(".claude.json")).unwrap();
    std::fs::remove_file(temp.path().join(".claude/.credentials.json")).unwrap();

    // Second run: host auth missing — files preserved but permissions repaired
    let (state2, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);

    let json_perms = std::fs::metadata(state2.claude_account_json().unwrap())
        .unwrap()
        .permissions();
    assert_eq!(
        json_perms.mode() & 0o777,
        0o600,
        "sync should repair .claude.json permissions even when host auth is missing"
    );
    let creds_perms = std::fs::metadata(state2.claude_credentials_json().unwrap())
        .unwrap()
        .permissions();
    assert_eq!(
        creds_perms.mode() & 0o777,
        0o600,
        "sync should repair .credentials.json permissions even when host auth is missing"
    );
}

// ── Symlink traversal protection ────────────────────────────────────

#[cfg(unix)]
#[test]
fn rejects_symlink_at_claude_json() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    // First run: create the state directory
    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    // Replace .claude.json with a symlink to a decoy file
    let decoy = temp.path().join("decoy.txt");
    std::fs::write(&decoy, "original").unwrap();
    std::fs::remove_file(state.claude_account_json().unwrap()).unwrap();
    std::os::unix::fs::symlink(&decoy, state.claude_account_json().unwrap()).unwrap();

    // Sync should refuse to write through the symlink
    let err = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("symlink"),
        "expected symlink error, got: {err}"
    );

    // Decoy file must be untouched
    assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "original");
}

#[cfg(unix)]
#[test]
fn rejects_symlink_at_credentials_json() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    // First run: create the state directory with credentials
    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    // Replace .credentials.json with a symlink
    let decoy = temp.path().join("decoy-creds.txt");
    std::fs::write(&decoy, "secret").unwrap();
    let creds_path = state.claude_credentials_json().unwrap();
    std::fs::remove_file(creds_path).unwrap();
    std::os::unix::fs::symlink(&decoy, creds_path).unwrap();

    // Sync should refuse to write through the symlink
    let err = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("symlink"),
        "expected symlink error, got: {err}"
    );

    // Decoy file must be untouched
    assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "secret");
}

// Tests for `instance/auth` — amp auth tests.
use std::path::Path;

fn stage_host_secrets(temp: &tempfile::TempDir, content: &str) -> std::path::PathBuf {
    let host_home = temp.path().join("host_home");
    let amp_dir = host_home.join(".local/share/amp");
    std::fs::create_dir_all(&amp_dir).unwrap();
    std::fs::write(amp_dir.join("secrets.json"), content).unwrap();
    host_home
}

#[test]
fn sync_copies_host_secrets_json_when_present() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = stage_host_secrets(
        &temp,
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
    );

    let (outcome, mounted) =
        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(secrets_json.as_path()));
    assert_eq!(
        std::fs::read_to_string(&secrets_json).unwrap(),
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}"
    );
}

#[test]
fn sync_source_dir_copies_direct_secrets_json() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let source_dir = temp.path().join("amp-work");
    std::fs::create_dir_all(&source_dir).unwrap();
    let expected = "{\"apiKey@https://ampcode.com/\":\"sgamp_workspace_test\"}";
    std::fs::write(source_dir.join("secrets.json"), expected).unwrap();

    let (outcome, mounted) = RoleState::provision_amp_auth_from_source_dir(
        &secrets_json,
        AuthForwardMode::Sync,
        &source_dir,
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(secrets_json.as_path()));
    assert_eq!(std::fs::read_to_string(&secrets_json).unwrap(), expected);
}

#[test]
fn sync_preserves_existing_secrets_when_host_file_missing() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    std::fs::write(&secrets_json, "{\"in_container_login\":true}").unwrap();
    let host_home = temp.path().join("empty_host_home");

    let (outcome, mounted) =
        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert_eq!(mounted.as_deref(), Some(secrets_json.as_path()));
    assert_eq!(
        std::fs::read_to_string(&secrets_json).unwrap(),
        "{\"in_container_login\":true}"
    );
}

#[test]
fn sync_with_no_host_and_no_prior_file_skips_mount() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = temp.path().join("empty_host_home");

    let (outcome, mounted) =
        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(mounted.is_none());
    assert!(!secrets_json.exists());
}

#[test]
fn api_key_mode_wipes_role_secrets_json() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    std::fs::write(&secrets_json, "{\"stale\":\"creds\"}").unwrap();
    let host_home = stage_host_secrets(
        &temp,
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
    );

    let (outcome, mounted) =
        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::ApiKey, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(mounted.is_none());
    assert!(!secrets_json.exists());
}

#[test]
fn ignore_mode_wipes_role_secrets_json() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    std::fs::write(&secrets_json, "{\"stale\":\"creds\"}").unwrap();

    let (outcome, mounted) = RoleState::provision_amp_auth(
        &secrets_json,
        AuthForwardMode::Ignore,
        Path::new("/nonexistent"),
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(mounted.is_none());
    assert!(!secrets_json.exists());
}

#[test]
fn oauth_token_defensive_arm_wipes_role_state() {
    // OAuthToken is parser-rejected for Amp; the defensive arm in
    // provision_amp_auth wipes any prior Sync's role-state file so a
    // config bypass cannot leak forwarded credentials into the
    // container. The arm should never run in production, but if it
    // does the bypass must be loud and safe.
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    std::fs::write(&secrets_json, "{\"prior_sync\":true}").unwrap();

    let (outcome, mounted) = RoleState::provision_amp_auth(
        &secrets_json,
        AuthForwardMode::OAuthToken,
        Path::new("/nonexistent"),
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(mounted.is_none(), "bypass arm must not produce a mount");
    assert!(
        !secrets_json.exists(),
        "bypass arm must wipe the prior Sync residue"
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlink_at_secrets_json_under_every_mode() {
    // Loop every mode. The symlink check is hoisted above the mode
    // match, so the defense holds for all four arms today — but a
    // future refactor that pushes the check inside specific arms
    // could silently regress Sync (highest blast radius — it would
    // otherwise read through the symlink).
    for mode in [
        AuthForwardMode::Sync,
        AuthForwardMode::ApiKey,
        AuthForwardMode::OAuthToken,
        AuthForwardMode::Ignore,
    ] {
        let temp = tempdir().unwrap();
        let secrets_json = temp.path().join("secrets.json");
        let decoy = temp.path().join("decoy.txt");
        std::fs::write(&decoy, "secret").unwrap();
        std::os::unix::fs::symlink(&decoy, &secrets_json).unwrap();

        let err = RoleState::provision_amp_auth(&secrets_json, mode, Path::new("/nonexistent"))
            .unwrap_err();

        assert!(
            err.to_string().contains("symlink"),
            "mode={mode:?}: expected symlink rejection, got: {err}"
        );
        assert_eq!(
            std::fs::read_to_string(&decoy).unwrap(),
            "secret",
            "mode={mode:?}: decoy contents must survive"
        );
    }
}

#[cfg(unix)]
#[test]
fn surfaces_unreadable_host_secrets_json_as_error() {
    // Sync arm with an unreadable host secrets.json must surface
    // the io::Error rather than misdiagnosing as HostMissing —
    // otherwise the operator gets trapped in a re-login loop they
    // cannot escape until they spot the bad permissions on the
    // host file.
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = temp.path().join("host_home");
    let amp_dir = host_home.join(".local/share/amp");
    std::fs::create_dir_all(&amp_dir).unwrap();
    let host_secrets = amp_dir.join("secrets.json");
    std::fs::write(
        &host_secrets,
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
    )
    .unwrap();
    std::fs::set_permissions(&host_secrets, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_to_string(&host_secrets).is_ok() {
        return;
    }

    let result = RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home);

    // Restore perms so tempdir cleanup succeeds regardless of test
    // outcome.
    drop(std::fs::set_permissions(
        &host_secrets,
        std::fs::Permissions::from_mode(0o600),
    ));

    let err = result.expect_err("EACCES on host secrets.json must surface as an error");
    let rendered = format!("{err:#}");
    assert!(
        rendered.contains("secrets.json"),
        "error must name the host file: {rendered}"
    );
    assert!(
        !rendered.contains("not found") && !rendered.to_ascii_lowercase().contains("nonexistent"),
        "EACCES must not be reported as not-found: {rendered}"
    );
}

#[cfg(unix)]
#[test]
fn synced_secrets_json_has_restricted_permissions() {
    // Bypassing `write_private_file` would land at 0o644 and leak
    // the token. Pin 0o600 explicitly.
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = stage_host_secrets(
        &temp,
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
    );

    RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    let mode = std::fs::metadata(&secrets_json)
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o600,
        "synced secrets.json must be 0o600, got {mode:o}"
    );
}

#[cfg(unix)]
#[test]
fn sync_repairs_permissions_when_host_secrets_missing() {
    // HostMissing is the only path that carries a prior file
    // across launches; the arm must tighten its perms.
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    std::fs::write(&secrets_json, "{\"in_container_login\":true}").unwrap();
    std::fs::set_permissions(&secrets_json, std::fs::Permissions::from_mode(0o644)).unwrap();
    let host_home = temp.path().join("empty_host_home");

    let (outcome, _) =
        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    let mode = std::fs::metadata(&secrets_json)
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o600,
        "HostMissing must repair the carry-over file's permissions; got {mode:o}"
    );
}

#[test]
fn sync_ignores_xdg_config_settings_json_decoy() {
    // Catches a regression that swapped the Sync source from
    // XDG_DATA `secrets.json` to XDG_CONFIG `settings.json`.
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = temp.path().join("host_home");

    // Only the XDG_CONFIG decoy exists; the canonical XDG_DATA
    // path is empty.
    let xdg_config = host_home.join(".config/amp");
    std::fs::create_dir_all(&xdg_config).unwrap();
    std::fs::write(xdg_config.join("settings.json"), "WRONG").unwrap();

    let (outcome, mounted) =
        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(
        outcome,
        AuthProvisionOutcome::HostMissing,
        "decoy at XDG_CONFIG must not produce a Sync"
    );
    assert!(mounted.is_none());
    assert!(
        !secrets_json.exists(),
        "decoy contents must not be copied into the role state"
    );
}

#[test]
fn sync_treats_empty_host_secrets_as_host_missing() {
    // Without this guard, an empty host file would be Synced and
    // the agent would fail to auth with no breadcrumb.
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = stage_host_secrets(&temp, "   \n\t  \n");

    let (outcome, mounted) =
        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(mounted.is_none());
    assert!(
        !secrets_json.exists(),
        "empty host file must not produce a role-state copy"
    );
}

// Tests for `instance/auth` — codex auth tests.

/// Stage a fake host home with a populated `~/.codex/auth.json` so
/// the sync-mode tests below have a real source file to copy from.
/// Returns the host-home root and the auth.json contents written.
fn stage_host_auth_json(temp: &tempfile::TempDir, tail: &str) -> (std::path::PathBuf, String) {
    let host_home = temp.path().join("host_home");
    let codex_dir = host_home.join(".codex");
    std::fs::create_dir_all(&codex_dir).unwrap();
    let content = format!(
        "{{\"auth_mode\":\"chatgpt\",\"OPENAI_API_KEY\":null,\"tokens\":{{\"id_token\":\"{tail}\"}}}}",
    );
    std::fs::write(codex_dir.join("auth.json"), &content).unwrap();
    (host_home, content)
}

#[test]
fn sync_copies_host_auth_json_when_present() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let (host_home, expected) = stage_host_auth_json(&temp, "abc.test");

    let (outcome, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(std::fs::read_to_string(&auth_json).unwrap(), expected);
}

/// Re-syncing identical host content must NOT rewrite the role-state
/// file. `write_private_file` replaces the inode (temp + rename); on
/// macOS that invalidates a live single-file bind mount into the running
/// container, which is exactly how the background sibling-auth prewarm
/// silently broke Codex/amp/grok/opencode auth (the file became
/// unreadable inside the container, so runtime-setup skipped the copy
/// and the agent started unauthenticated). Pin the no-churn guard by
/// asserting the inode is stable across an unchanged re-sync.
#[cfg(unix)]
#[test]
fn sync_does_not_rewrite_inode_when_content_unchanged() {
    use std::os::unix::fs::MetadataExt as _;

    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let (host_home, _) = stage_host_auth_json(&temp, "stable.test");

    let (outcome1, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();
    assert_eq!(outcome1, AuthProvisionOutcome::Synced);
    let ino_before = std::fs::metadata(&auth_json).unwrap().ino();

    // Second identical sync (mirrors the background prewarm re-provisioning
    // the same file the foreground launch already bind-mounted).
    let (outcome2, mounted) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();
    assert_eq!(outcome2, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(auth_json.as_path()));
    let ino_after = std::fs::metadata(&auth_json).unwrap().ino();

    assert_eq!(
        ino_before, ino_after,
        "unchanged re-sync must not replace the inode (would stale a live bind mount)"
    );
}

#[test]
fn sync_source_dir_copies_direct_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let source_dir = temp.path().join("codex-work");
    std::fs::create_dir_all(&source_dir).unwrap();
    let expected = r#"{"auth_mode":"chatgpt","tokens":{"id_token":"workspace.test"}}"#;
    std::fs::write(source_dir.join("auth.json"), expected).unwrap();

    let (outcome, mounted) = RoleState::provision_codex_auth_from_source_dir(
        &auth_json,
        AuthForwardMode::Sync,
        &source_dir,
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(auth_json.as_path()));
    assert_eq!(std::fs::read_to_string(&auth_json).unwrap(), expected);
}

#[test]
fn sync_returns_host_missing_when_host_lacks_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let host_home = temp.path().join("host_home_without_codex_dir");

    let (outcome, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(!auth_json.exists(), "no bootstrap file should be created");
}

#[test]
fn sync_preserves_existing_role_auth_json_when_host_file_missing() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    std::fs::write(&auth_json, "{\"in_container_login\":true}").unwrap();
    let host_home = temp.path().join("empty_host_home");

    let (outcome, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert_eq!(
        std::fs::read_to_string(&auth_json).unwrap(),
        "{\"in_container_login\":true}",
        "in-container login state must survive sync-with-no-host"
    );
}

#[test]
fn ignore_deletes_existing_role_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    std::fs::write(&auth_json, "{\"stale\":\"creds\"}").unwrap();

    let (outcome, _) = RoleState::provision_codex_auth(
        &auth_json,
        AuthForwardMode::Ignore,
        Path::new("/nonexistent"),
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(!auth_json.exists());
}

/// `OAuthToken` is parser-rejected for Codex so this arm is
/// unreachable from operator config in production. The test pins the
/// defensive no-wipe behavior of the `OAuthToken` arm anyway: if a
/// parser bypass ever lands a Codex+OAuthToken config at this layer,
/// the existing role-state `auth.json` is preserved (rather than
/// silently destroyed) so the operator can recover.
#[test]
fn token_mode_leaves_role_auth_json_untouched() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    std::fs::write(&auth_json, "{\"existing\":true}").unwrap();
    let (host_home, _) = stage_host_auth_json(&temp, "should-not-be-copied");

    let (outcome, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::OAuthToken, &host_home)
            .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert_eq!(
        std::fs::read_to_string(&auth_json).unwrap(),
        "{\"existing\":true}"
    );
}

/// `ApiKey` mode authenticates via `OPENAI_API_KEY`; a leftover
/// `auth.json` from a prior Sync run would let Codex silently fall
/// back to forwarded OAuth credentials that the operator has
/// explicitly chosen to bypass. Pin the wipe contract here so a
/// future refactor can't quietly downgrade `ApiKey` to no-op.
#[test]
fn api_key_mode_wipes_role_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    std::fs::write(&auth_json, "{\"stale\":\"creds\"}").unwrap();
    // Stage a host auth.json too — api_key mode must NOT copy it,
    // and must NOT leave the stale role-state file in place either.
    let (host_home, _) = stage_host_auth_json(&temp, "should-not-be-copied");

    let (outcome, mounted) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::ApiKey, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(
        !auth_json.exists(),
        "api_key mode must wipe role-state auth.json"
    );
    assert!(
        mounted.is_none(),
        "api_key mode must report no auth.json to mount"
    );
}

/// Switching from Sync (creds present on host) to `ApiKey` must wipe
/// the synced `auth.json` so the next container start cannot fall
/// back to forwarded OAuth credentials. Without this, an operator
/// who toggles to `ApiKey` to use `OPENAI_API_KEY` would still be
/// running on stale OAuth state from the previous sync run.
#[test]
fn switching_from_sync_to_api_key_wipes_synced_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let (host_home, _) = stage_host_auth_json(&temp, "switch.test");

    let (outcome, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(auth_json.exists());

    let (outcome, mounted) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::ApiKey, &host_home).unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(!auth_json.exists(), "ApiKey must wipe prior synced creds");
    assert!(mounted.is_none());
}

#[cfg(unix)]
#[test]
fn synced_auth_json_has_restricted_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let (host_home, _) = stage_host_auth_json(&temp, "perm.test");

    RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

    let mode = std::fs::metadata(&auth_json).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "codex auth.json must be 0o600, got {mode:o}");
}

#[cfg(unix)]
#[test]
fn rejects_symlink_at_auth_json_under_ignore() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");

    let decoy = temp.path().join("decoy.txt");
    std::fs::write(&decoy, "secret").unwrap();
    std::os::unix::fs::symlink(&decoy, &auth_json).unwrap();

    let err = RoleState::provision_codex_auth(
        &auth_json,
        AuthForwardMode::Ignore,
        Path::new("/nonexistent"),
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("symlink"),
        "expected symlink rejection, got: {err}"
    );
    // Decoy file must be untouched.
    assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "secret");
}

/// Pin symlink rejection across all credential-bearing modes via the
/// pre-mode-dispatch check at the top of `provision_codex_auth`.
/// Without this, a compromised role could plant a symlink at the
/// role-state `auth.json` and have subsequent sync/token-mode
/// provisioning bind-mount it into the container as-is.
#[cfg(unix)]
#[test]
fn rejects_symlink_at_auth_json_under_sync_and_token() {
    for mode in [
        AuthForwardMode::Sync,
        AuthForwardMode::OAuthToken,
        AuthForwardMode::ApiKey,
    ] {
        let temp = tempdir().unwrap();
        let auth_json = temp.path().join("auth.json");

        let decoy = temp.path().join("decoy.txt");
        std::fs::write(&decoy, "secret").unwrap();
        std::os::unix::fs::symlink(&decoy, &auth_json).unwrap();

        let err = RoleState::provision_codex_auth(&auth_json, mode, Path::new("/nonexistent"))
            .unwrap_err();
        assert!(
            err.to_string().contains("symlink"),
            "mode {mode:?} did not reject symlink: {err}"
        );
        assert_eq!(
            std::fs::read_to_string(&decoy).unwrap(),
            "secret",
            "mode {mode:?} clobbered decoy"
        );
    }
}

/// Switching from Sync (creds present on host) to Ignore must wipe
/// the synced auth.json so the next container start forces a fresh
/// in-container login. Without this, an operator who toggles to
/// Ignore to revoke access keeps the prior credentials accessible.
#[test]
fn switching_from_sync_to_ignore_wipes_synced_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let (host_home, _) = stage_host_auth_json(&temp, "rev.test");

    let (outcome, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(auth_json.exists());

    let (outcome, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Ignore, &host_home).unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(!auth_json.exists(), "Ignore must wipe prior synced creds");
}

/// An unreadable host `auth.json` (e.g. `chmod 0` after a `sudo
/// codex login`) used to be silently bucketed as `HostMissing`,
/// trapping operators in a re-login loop. Verify the EACCES path
/// now surfaces an explicit error mentioning the host path.
#[cfg(unix)]
#[test]
fn surfaces_unreadable_host_auth_json_as_error() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let host_home = temp.path().join("host_home");
    let host_codex = host_home.join(".codex");
    std::fs::create_dir_all(&host_codex).unwrap();
    let host_auth_json = host_codex.join("auth.json");
    std::fs::write(&host_auth_json, "{\"auth_mode\":\"chatgpt\"}").unwrap();
    // chmod 0 — file exists but is unreadable. Skip if we can't
    // produce an unreadable file (e.g. running as root in CI).
    std::fs::set_permissions(&host_auth_json, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_to_string(&host_auth_json).is_ok() {
        // Running as root — chmod 0 doesn't block reads. Skip.
        return;
    }

    let err =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("auth.json"),
        "error must mention the host path: {msg}"
    );
    assert!(
        !msg.to_lowercase().contains("not found"),
        "EACCES must not be reported as not-found: {msg}"
    );
}

// Tests for `instance/auth` — github auth tests.
// The `gh auth token` shellout in `read_host_gh_token` is gated on
// `host_home_is_real(host_home)` — every test in this module passes
// a temp-dir `host_home` so the shellout is skipped and the real
// host's `gh` binary cannot leak into hermetic tests. The file
// fallback is the only path exercised here. See
// `read_host_gh_token` source for the gate.
use super::{GithubAuthMode, parse_gh_hosts_yml};
use crate::{
    GithubAuthContext, GithubProvisionKind, GithubProvisionOutcome, GithubTokenSource,
    HostMissingReason,
};

/// Stage a fake host home with a populated `~/.config/gh/hosts.yml`
/// so the file-fallback path can be exercised hermetically.
fn stage_host_hosts_yml(temp: &tempfile::TempDir, token: &str) -> std::path::PathBuf {
    let host_home = temp.path().join("host_home");
    let gh_dir = host_home.join(".config/gh");
    std::fs::create_dir_all(&gh_dir).unwrap();
    std::fs::write(
        gh_dir.join("hosts.yml"),
        format!(
            "github.com:\n    oauth_token: {token}\n    git_protocol: https\n    user: alice\n",
        ),
    )
    .unwrap();
    host_home
}

fn ctx(mode: GithubAuthMode, token: Option<&str>) -> GithubAuthContext {
    GithubAuthContext {
        mode,
        token: token.map(str::to_owned),
    }
}

// ── parse_gh_hosts_yml ────────────────────────────────────────────────

#[test]
fn parse_hosts_yml_extracts_oauth_token_and_user() {
    let text = "github.com:\n    oauth_token: ghp_xxx\n    user: alice\n";
    let parsed = parse_gh_hosts_yml(text).expect("must parse");
    assert_eq!(parsed.token, "ghp_xxx");
    assert_eq!(parsed.user.as_deref(), Some("alice"));
}

#[test]
fn parse_hosts_yml_handles_quoted_values() {
    let text = "github.com:\n    oauth_token: \"ghp_xxx\"\n    user: \'bob\'\n";
    let parsed = parse_gh_hosts_yml(text).expect("must parse");
    assert_eq!(parsed.token, "ghp_xxx");
    assert_eq!(parsed.user.as_deref(), Some("bob"));
}

#[test]
fn parse_hosts_yml_returns_none_when_github_block_missing() {
    let text = "ghe.acme.com:\n    oauth_token: ghp_acme\n";
    assert!(parse_gh_hosts_yml(text).is_none());
}

#[test]
fn parse_hosts_yml_returns_none_without_oauth_token() {
    let text = "github.com:\n    user: alice\n";
    assert!(parse_gh_hosts_yml(text).is_none());
}

#[test]
fn parse_hosts_yml_ignores_other_hosts() {
    let text = concat!(
        "ghe.acme.com:\n    oauth_token: ghp_acme\n    user: bob\n",
        "github.com:\n    oauth_token: ghp_real\n    user: alice\n",
    );
    let parsed = parse_gh_hosts_yml(text).expect("must parse");
    assert_eq!(parsed.token, "ghp_real");
    assert_eq!(parsed.user.as_deref(), Some("alice"));
}

/// Per YAML 1.x spec, a `#` inside a bare scalar is part of the
/// value (only `#` preceded by whitespace starts a comment). A
/// real-world `gh` token will never contain `#`, but pinning this
/// behavior protects against regressions in the YAML parser
/// dependency.
#[test]
fn parse_hosts_yml_preserves_hash_inside_token_value() {
    let text = "github.com:\n    oauth_token: ghp_real#segment\n";
    let parsed = parse_gh_hosts_yml(text).expect("must parse");
    assert_eq!(parsed.token, "ghp_real#segment");
}

/// Trailing-whitespace `#` IS a comment per YAML and must be
/// stripped from the parsed value.
#[test]
fn parse_hosts_yml_strips_trailing_whitespace_comment() {
    let text = "github.com:\n    oauth_token: ghp_real # rotated 2026-01\n";
    let parsed = parse_gh_hosts_yml(text).expect("must parse");
    assert_eq!(parsed.token, "ghp_real");
}

/// Malformed YAML (e.g. mismatched quotes) must NOT yield a
/// partial result. The `serde_yaml_ng` parser returns an error;
/// `parse_gh_hosts_yml` maps that to `None` so callers fall
/// through to `HostMissing` instead of writing a bogus token.
#[test]
fn parse_hosts_yml_rejects_malformed_yaml() {
    let text = "github.com:\n    oauth_token: \'broken\"\n";
    assert!(parse_gh_hosts_yml(text).is_none());
}

// ── provision_github_auth ────────────────────────────────────────────

#[test]
fn sync_falls_back_to_hosts_yml_file_when_gh_binary_absent() {
    let temp = tempdir().unwrap();
    let host_home = stage_host_hosts_yml(&temp, "ghp_filebased");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    let outcome =
        RoleState::provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home)
            .unwrap();

    match &outcome {
        GithubProvisionOutcome::Synced { token, source } => {
            assert_eq!(token, "ghp_filebased");
            assert_eq!(*source, GithubTokenSource::HostsFile);
        }
        other => panic!("expected Synced, got {other:?}"),
    }
    assert_eq!(outcome.token(), Some("ghp_filebased"));
    assert_eq!(outcome.kind(), GithubProvisionKind::Synced);
    let written = std::fs::read_to_string(&hosts_yml).unwrap();
    assert!(written.contains("oauth_token: ghp_filebased"));
    assert!(written.contains("git_protocol: https"));
    assert!(written.contains("user: alice"));
}

#[test]
fn sync_returns_host_missing_when_neither_source_resolves() {
    let temp = tempdir().unwrap();
    let host_home = temp.path().join("host_home_with_no_gh_state");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    let outcome =
        RoleState::provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home)
            .unwrap();

    assert_eq!(
        outcome,
        GithubProvisionOutcome::HostMissing {
            reason: HostMissingReason::NoGhAndNoHostsFile
        }
    );
    assert!(outcome.token().is_none());
    assert!(!hosts_yml.exists());
}

#[test]
fn sync_preserves_existing_role_hosts_yml_when_host_lacks_token() {
    let temp = tempdir().unwrap();
    let host_home = temp.path().join("empty_host_home");
    let hosts_yml = temp.path().join("role-state-hosts.yml");
    std::fs::write(&hosts_yml, "github.com:\n    oauth_token: in_container\n").unwrap();

    let outcome =
        RoleState::provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home)
            .unwrap();

    assert_eq!(outcome.kind(), GithubProvisionKind::HostMissing);
    let preserved = std::fs::read_to_string(&hosts_yml).unwrap();
    assert!(
        preserved.contains("in_container"),
        "in-container login state must survive sync-with-no-host"
    );
}

#[test]
fn token_mode_wipes_role_hosts_yml() {
    let temp = tempdir().unwrap();
    let host_home = temp.path().join("host_home");
    let hosts_yml = temp.path().join("role-state-hosts.yml");
    std::fs::write(&hosts_yml, "github.com:\n    oauth_token: stale\n").unwrap();

    let outcome = RoleState::provision_github_auth(
        &hosts_yml,
        &ctx(GithubAuthMode::Token, Some("ghp_token")),
        &host_home,
    )
    .unwrap();

    assert_eq!(
        outcome,
        GithubProvisionOutcome::TokenMode {
            token: "ghp_token".to_owned()
        }
    );
    assert_eq!(outcome.token(), Some("ghp_token"));
    assert!(
        !hosts_yml.exists(),
        "token mode must wipe role-state hosts.yml"
    );
}

#[test]
fn ignore_mode_wipes_role_hosts_yml() {
    let temp = tempdir().unwrap();
    let host_home = temp.path().join("host_home");
    let hosts_yml = temp.path().join("role-state-hosts.yml");
    std::fs::write(&hosts_yml, "github.com:\n    oauth_token: stale\n").unwrap();

    let outcome = RoleState::provision_github_auth(
        &hosts_yml,
        &ctx(GithubAuthMode::Ignore, None),
        &host_home,
    )
    .unwrap();

    assert_eq!(outcome, GithubProvisionOutcome::Skipped);
    assert!(outcome.token().is_none());
    assert!(!hosts_yml.exists());
}

#[test]
fn switching_from_sync_to_token_wipes_synced_hosts_yml() {
    let temp = tempdir().unwrap();
    let host_home = stage_host_hosts_yml(&temp, "ghp_synced");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    let outcome =
        RoleState::provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home)
            .unwrap();
    assert_eq!(outcome.kind(), GithubProvisionKind::Synced);
    assert!(hosts_yml.exists());

    let outcome = RoleState::provision_github_auth(
        &hosts_yml,
        &ctx(GithubAuthMode::Token, Some("ghp_scoped")),
        &host_home,
    )
    .unwrap();
    assert_eq!(outcome.kind(), GithubProvisionKind::TokenMode);
    assert!(!hosts_yml.exists());
}

#[test]
fn switching_from_sync_to_ignore_wipes_synced_hosts_yml() {
    let temp = tempdir().unwrap();
    let host_home = stage_host_hosts_yml(&temp, "ghp_synced");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    let outcome =
        RoleState::provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home)
            .unwrap();
    assert_eq!(outcome.kind(), GithubProvisionKind::Synced);

    let outcome = RoleState::provision_github_auth(
        &hosts_yml,
        &ctx(GithubAuthMode::Ignore, None),
        &host_home,
    )
    .unwrap();
    assert_eq!(outcome, GithubProvisionOutcome::Skipped);
    assert!(!hosts_yml.exists());
}

/// Round-trip across all four modes — pins state cleanliness on
/// every transition so a regression in `wipe_file_if_present`
/// (e.g. leaving a 0o600 stub) gets caught here.
#[test]
fn round_trip_ignore_sync_token_ignore_state_clean() {
    let temp = tempdir().unwrap();
    let host_home = stage_host_hosts_yml(&temp, "ghp_round");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    let outcome = RoleState::provision_github_auth(
        &hosts_yml,
        &ctx(GithubAuthMode::Ignore, None),
        &host_home,
    )
    .unwrap();
    assert_eq!(outcome.kind(), GithubProvisionKind::Skipped);
    assert!(!hosts_yml.exists());

    let outcome =
        RoleState::provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home)
            .unwrap();
    assert_eq!(outcome.kind(), GithubProvisionKind::Synced);
    assert!(hosts_yml.exists());

    let outcome = RoleState::provision_github_auth(
        &hosts_yml,
        &ctx(GithubAuthMode::Token, Some("scoped")),
        &host_home,
    )
    .unwrap();
    assert_eq!(outcome.kind(), GithubProvisionKind::TokenMode);
    assert!(!hosts_yml.exists());

    let outcome = RoleState::provision_github_auth(
        &hosts_yml,
        &ctx(GithubAuthMode::Ignore, None),
        &host_home,
    )
    .unwrap();
    assert_eq!(outcome.kind(), GithubProvisionKind::Skipped);
    assert!(!hosts_yml.exists());
}

/// Two consecutive Sync calls with the same host token must not
/// re-write `hosts.yml` — mtime stable. Mirrors the codex no-churn
/// guard; a regression that drops the content-equal check would
/// fire `write_private_file` (atomic rename) on every launch.
#[test]
fn sync_idempotent_skips_write_when_content_unchanged() {
    let temp = tempdir().unwrap();
    let host_home = stage_host_hosts_yml(&temp, "ghp_unchanged");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    RoleState::provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home)
        .unwrap();
    let forced_mtime = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    #[expect(
        clippy::disallowed_methods,
        reason = "test fixture forces mtime on an already-created hosts.yml file"
    )]
    std::fs::File::options()
        .write(true)
        .open(&hosts_yml)
        .unwrap()
        .set_modified(forced_mtime)
        .unwrap();
    let mtime_first = std::fs::metadata(&hosts_yml).unwrap().modified().unwrap();

    RoleState::provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home)
        .unwrap();
    let mtime_second = std::fs::metadata(&hosts_yml).unwrap().modified().unwrap();

    assert_eq!(
        mtime_first, mtime_second,
        "no-op Sync provisioning must not touch hosts.yml mtime"
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlink_at_hosts_yml_under_sync_and_token_and_ignore() {
    for mode in [
        GithubAuthMode::Sync,
        GithubAuthMode::Token,
        GithubAuthMode::Ignore,
    ] {
        let temp = tempdir().unwrap();
        let host_home = temp.path().join("host_home");
        let hosts_yml = temp.path().join("role-state-hosts.yml");

        let decoy = temp.path().join("decoy.yml");
        std::fs::write(&decoy, "secret").unwrap();
        std::os::unix::fs::symlink(&decoy, &hosts_yml).unwrap();

        let token = matches!(mode, GithubAuthMode::Token).then_some("tok");
        let err = RoleState::provision_github_auth(&hosts_yml, &ctx(mode, token), &host_home)
            .unwrap_err();

        assert!(
            err.to_string().contains("symlink"),
            "mode {mode:?} did not reject symlink: {err}"
        );
        assert_eq!(
            std::fs::read_to_string(&decoy).unwrap(),
            "secret",
            "mode {mode:?} clobbered decoy"
        );
    }
}

#[cfg(unix)]
#[test]
fn synced_hosts_yml_has_0600_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let host_home = stage_host_hosts_yml(&temp, "ghp_perm");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    RoleState::provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home)
        .unwrap();

    let mode = std::fs::metadata(&hosts_yml).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "synced hosts.yml must be 0o600, got {mode:o}");
}

/// Sync mode consumes an operator-supplied `GH_TOKEN` before consulting the
/// host `gh` CLI or `hosts.yml`, so configured credentials do not trigger
/// extra host credential work.
#[test]
fn sync_consumes_supplied_token_before_host_lookup() {
    let temp = tempdir().unwrap();
    let host_home = temp.path().join("empty_host_home");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    let outcome = RoleState::provision_github_auth(
        &hosts_yml,
        &ctx(GithubAuthMode::Sync, Some("operator_supplied")),
        &host_home,
    )
    .unwrap();

    assert_eq!(outcome.kind(), GithubProvisionKind::Synced);
    assert_eq!(outcome.token(), Some("operator_supplied"));
    assert_eq!(
        outcome,
        GithubProvisionOutcome::Synced {
            token: "operator_supplied".to_owned(),
            source: GithubTokenSource::ConfiguredEnv,
        }
    );
    let hosts = std::fs::read_to_string(hosts_yml).unwrap();
    assert!(hosts.contains("oauth_token: operator_supplied"));
}

/// Manual `Debug` impl on `GithubAuthContext` must redact the
/// token so `tracing::debug!("{ctx:?}")` cannot leak it.
#[test]
fn github_auth_context_debug_redacts_token() {
    let ctx = ctx(GithubAuthMode::Token, Some("ghp_secret_value"));
    let s = format!("{ctx:?}");
    assert!(
        !s.contains("ghp_secret_value"),
        "token leaked in Debug: {s}"
    );
    assert!(s.contains("<redacted>"));
}

/// Manual `Debug` impl on `GithubProvisionOutcome` must redact the
/// token in `Synced` and `TokenMode` variants.
#[test]
fn github_provision_outcome_debug_redacts_token() {
    let synced = GithubProvisionOutcome::Synced {
        token: "ghp_synced_secret".to_owned(),
        source: GithubTokenSource::GhCli,
    };
    let s = format!("{synced:?}");
    assert!(!s.contains("ghp_synced_secret"), "Synced token leaked: {s}");

    let tok = GithubProvisionOutcome::TokenMode {
        token: "ghp_token_secret".to_owned(),
    };
    let s = format!("{tok:?}");
    assert!(
        !s.contains("ghp_token_secret"),
        "TokenMode token leaked: {s}"
    );
}

// Tests for `instance/auth` — kimi auth tests.

fn stage_host_kimi_dir(
    temp: &tempfile::TempDir,
    config_content: Option<&str>,
    cred_files: &[(&str, &str)],
    mcp_json: Option<&str>,
    device_id: Option<&str>,
) -> std::path::PathBuf {
    let host_home = temp.path().join("host_home");
    let kimi_dir = host_home.join(".kimi-code");
    std::fs::create_dir_all(&kimi_dir).unwrap();
    if let Some(content) = config_content {
        std::fs::write(kimi_dir.join("config.toml"), content).unwrap();
    }
    if !cred_files.is_empty() {
        let creds_dir = kimi_dir.join("credentials");
        std::fs::create_dir_all(&creds_dir).unwrap();
        for (name, content) in cred_files {
            std::fs::write(creds_dir.join(name), content).unwrap();
        }
    }
    if let Some(content) = mcp_json {
        std::fs::write(kimi_dir.join("mcp.json"), content).unwrap();
    }
    if let Some(content) = device_id {
        std::fs::write(kimi_dir.join("device_id"), content).unwrap();
    }
    host_home
}

#[test]
fn sync_copies_config_toml_when_present() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, Some("[profile]\nname = \"test\""), &[], None, None);

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("config.toml")).unwrap(),
        "[profile]\nname = \"test\""
    );
}

#[test]
fn sync_source_dir_copies_direct_kimi_dir() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let source_dir = temp.path().join("kimi-work");
    std::fs::create_dir_all(source_dir.join("credentials")).unwrap();
    std::fs::write(
        source_dir.join("config.toml"),
        "[profile]\nname = \"workspace\"",
    )
    .unwrap();
    std::fs::write(source_dir.join("device_id"), "device-workspace").unwrap();
    std::fs::write(
        source_dir.join("credentials").join("token_main"),
        "tok_workspace",
    )
    .unwrap();

    let (outcome, forward_auth) = RoleState::provision_kimi_auth_from_source_dir(
        &kimi_dir,
        AuthForwardMode::Sync,
        &source_dir,
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("config.toml")).unwrap(),
        "[profile]\nname = \"workspace\""
    );
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("device_id")).unwrap(),
        "device-workspace"
    );
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("credentials").join("token_main")).unwrap(),
        "tok_workspace"
    );
}

#[test]
fn sync_copies_credentials_files_when_present() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, None, &[("token_main", "tok_abc123")], None, None);

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("credentials").join("token_main")).unwrap(),
        "tok_abc123"
    );
}

#[test]
fn sync_does_not_forward_mcp_json() {
    // `mcp.json` is operator-preference MCP server config, not auth
    // state. Forwarding it would leak host paths/binaries into the
    // sealed container and bypass the role-author model for declaring
    // in-container MCP servers. Regression guard for that decision:
    // even with the host file present, Sync must not copy it.
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, None, &[], Some(r#"{"servers":{}}"#), None);

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(
        !kimi_dir.join("mcp.json").exists(),
        "mcp.json must not be forwarded into the role state"
    );
}

#[test]
fn sync_copies_device_id_when_present() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, None, &[], None, Some("device-abc123\n"));

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("device_id")).unwrap(),
        "device-abc123\n"
    );
}

#[test]
fn sync_with_empty_kimi_dir_creates_role_state_dir() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, None, &[], None, None);

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(kimi_dir.is_dir(), "role-state kimi dir must be created");
}

#[test]
fn sync_with_no_host_kimi_dir_returns_host_missing_with_forward_auth_true() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("empty_host_home");

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(forward_auth);
}

#[test]
fn sync_host_missing_still_creates_kimi_dir() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("empty_host_home");

    RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert!(
        kimi_dir.is_dir(),
        "role-state kimi dir must exist even when host is absent"
    );
}

#[test]
fn api_key_mode_wipes_prior_kimi_dir() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    std::fs::create_dir_all(&kimi_dir).unwrap();
    std::fs::write(kimi_dir.join("config.toml"), "stale").unwrap();
    let host_home = stage_host_kimi_dir(&temp, Some("[profile]\nname=\"test\""), &[], None, None);

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::ApiKey, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(!forward_auth);
    assert!(!kimi_dir.exists(), "api_key mode must wipe the kimi dir");
}

#[test]
fn ignore_mode_wipes_prior_kimi_dir() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    std::fs::create_dir_all(&kimi_dir).unwrap();
    std::fs::write(kimi_dir.join("config.toml"), "old_config").unwrap();

    let (outcome, forward_auth) = RoleState::provision_kimi_auth(
        &kimi_dir,
        AuthForwardMode::Ignore,
        Path::new("/nonexistent"),
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(!forward_auth);
    assert!(!kimi_dir.exists(), "ignore mode must wipe the kimi dir");
}

#[test]
fn oauth_token_defensive_arm_wipes_kimi_dir() {
    // OAuthToken is parser-rejected for Kimi; the defensive arm wipes
    // any prior Sync's role-state dir so a config bypass cannot leak
    // forwarded credentials into the container.
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    std::fs::create_dir_all(&kimi_dir).unwrap();
    std::fs::write(kimi_dir.join("config.toml"), "prior_sync = true").unwrap();

    let (outcome, forward_auth) = RoleState::provision_kimi_auth(
        &kimi_dir,
        AuthForwardMode::OAuthToken,
        Path::new("/nonexistent"),
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(!forward_auth, "bypass arm must not set forward_auth");
    assert!(
        !kimi_dir.exists(),
        "bypass arm must wipe the prior Sync residue"
    );
}

#[test]
fn forward_auth_true_for_synced() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, Some("[x]"), &[], None, None);

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth, "forward_auth must be true for Synced");
}

#[test]
fn forward_auth_true_for_host_missing() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("no_host");

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(forward_auth, "forward_auth must be true for HostMissing");
}

#[test]
fn forward_auth_false_for_api_key() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("host_home");

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::ApiKey, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(!forward_auth, "forward_auth must be false for TokenMode");
}

#[test]
fn forward_auth_false_for_ignore() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("host_home");

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Ignore, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(!forward_auth, "forward_auth must be false for Skipped");
}

#[cfg(unix)]
#[test]
fn rejects_symlink_at_kimi_dir_under_every_mode() {
    // The symlink check is hoisted above the mode match; verify all
    // four arms are protected so a future refactor cannot regress the
    // Sync arm (highest blast radius).
    for mode in [
        AuthForwardMode::Sync,
        AuthForwardMode::ApiKey,
        AuthForwardMode::OAuthToken,
        AuthForwardMode::Ignore,
    ] {
        let temp = tempdir().unwrap();
        let kimi_dir = temp.path().join("kimi_state");
        let decoy = temp.path().join("decoy_dir");
        std::fs::create_dir_all(&decoy).unwrap();
        std::os::unix::fs::symlink(&decoy, &kimi_dir).unwrap();

        let err =
            RoleState::provision_kimi_auth(&kimi_dir, mode, Path::new("/nonexistent")).unwrap_err();

        assert!(
            err.to_string().contains("symlink"),
            "mode={mode:?}: expected symlink rejection, got: {err}"
        );
        assert!(decoy.exists(), "mode={mode:?}: decoy dir must survive");
    }
}

#[cfg(unix)]
#[test]
fn synced_credential_files_have_0600_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(
        &temp,
        Some("[profile]"),
        &[("access_token", "tok_secret_xyz")],
        None,
        Some("device-secret"),
    );

    RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    for rel in &["config.toml", "credentials/access_token", "device_id"] {
        let path = kimi_dir.join(rel);
        let mode = std::fs::metadata(&path)
            .unwrap_or_else(|e| panic!("missing synced file {rel}: {e}"))
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "synced file {rel} must be 0o600, got {mode:o}");
    }
}

#[test]
fn credentials_subdir_copies_recursively() {
    // Kimi Code stores MCP OAuth credentials under credentials/mcp/, so
    // subdirectories must be copied recursively.
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("host_home");
    let host_creds = host_home.join(".kimi-code/credentials");
    std::fs::create_dir_all(&host_creds).unwrap();
    std::fs::write(host_creds.join("real_token"), "real_tok_value").unwrap();
    let nested = host_creds.join("nested_subdir");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("inner_file"), "should_copy").unwrap();

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(
        kimi_dir.join("credentials/real_token").exists(),
        "real_token must be copied"
    );
    assert!(
        kimi_dir
            .join("credentials/nested_subdir/inner_file")
            .exists(),
        "nested subdir must be copied recursively"
    );
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("credentials/nested_subdir/inner_file")).unwrap(),
        "should_copy"
    );
}

#[cfg(unix)]
#[test]
fn surfaces_unreadable_credential_file_as_error() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, None, &[("access_token", "secret")], None, None);

    let cred = host_home.join(".kimi-code/credentials/access_token");
    std::fs::set_permissions(&cred, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_to_string(&cred).is_ok() {
        return;
    }

    let result = RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home);

    drop(std::fs::set_permissions(
        &cred,
        std::fs::Permissions::from_mode(0o600),
    ));

    let err = result.expect_err("unreadable credential file must surface as error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("access_token"),
        "error must name the unreadable file: {msg}"
    );
}

#[cfg(unix)]
#[test]
fn surfaces_unreadable_config_toml_as_error() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, Some("[profile]\nname=\"x\""), &[], None, None);

    let cfg = host_home.join(".kimi-code/config.toml");
    std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_to_string(&cfg).is_ok() {
        return;
    }

    let result = RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home);

    drop(std::fs::set_permissions(
        &cfg,
        std::fs::Permissions::from_mode(0o600),
    ));

    let err = result.expect_err("unreadable config.toml must surface as error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("config.toml"),
        "error must name the unreadable file: {msg}"
    );
}

#[cfg(unix)]
#[test]
fn credentials_nested_symlink_is_skipped_not_followed() {
    // A symlink planted under `credentials/mcp/` (e.g. by a hostile or
    // misconfigured host) must NOT be copied or dereferenced into the
    // sealed container. Real files in the same subtree must still copy.
    use std::os::unix::fs::symlink;
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("host_home");
    let host_creds = host_home.join(".kimi-code/credentials");
    std::fs::create_dir_all(host_creds.join("mcp")).unwrap();
    std::fs::write(host_creds.join("mcp").join("real_token"), "real").unwrap();
    let decoy = temp.path().join("decoy_outside_tree");
    std::fs::write(&decoy, "must_not_leak").unwrap();
    symlink(&decoy, host_creds.join("mcp").join("evil")).unwrap();

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(
        kimi_dir.join("credentials/mcp/real_token").exists(),
        "real nested file must still be copied"
    );
    assert!(
        !kimi_dir.join("credentials/mcp/evil").exists(),
        "nested symlink must not appear in role state"
    );
    // The decoy on the host must remain untouched: no write through the
    // skipped symlink, no read into the role state.
    assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "must_not_leak");
}

#[cfg(unix)]
#[test]
fn credentials_directories_are_chmodded_0700() {
    // Every copied credentials directory (root + nested) must be 0o700 so
    // the OAuth token subtree is not group/other-readable inside the
    // role-state bind mount.
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("host_home");
    let host_mcp = host_home.join(".kimi-code/credentials/mcp");
    std::fs::create_dir_all(&host_mcp).unwrap();
    std::fs::write(host_mcp.join("token"), "tok").unwrap();

    RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    for rel in &["credentials", "credentials/mcp"] {
        let mode = std::fs::metadata(kimi_dir.join(rel))
            .unwrap_or_else(|e| panic!("missing credentials dir {rel}: {e}"))
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "{rel} must be 0o700, got 0o{mode:o}");
    }
}
