//! Tests for `instance/auth` — tests.
use crate::instance::{AuthProvisionOutcome, PrepareResolvers, RoleState};
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

mod source_validation {
    use crate::instance::validate_sync_source_dir;
    use jackin_core::agent::Agent;
    use tempfile::tempdir;

    const TEST_CREDENTIALS: &str = super::TEST_CREDENTIALS;

    #[test]
    fn validate_rejects_non_directory() {
        let temp = tempdir().unwrap();
        let missing = temp.path().join("nope");
        assert!(validate_sync_source_dir(Agent::Codex, &missing, temp.path()).is_err());
    }

    #[test]
    fn validate_claude_accepts_file_credentials_rejects_bare_folder() {
        let temp = tempdir().unwrap();
        let good = temp.path().join("claude-good");
        std::fs::create_dir_all(&good).unwrap();
        std::fs::write(good.join(".credentials.json"), TEST_CREDENTIALS).unwrap();
        assert!(validate_sync_source_dir(Agent::Claude, &good, temp.path()).is_ok());

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
            assert!(
                validate_sync_source_dir(agent, &dir, temp.path()).is_err(),
                "{agent:?}: empty {name} must be rejected"
            );
            // Non-empty credential file is accepted.
            std::fs::write(dir.join(name), "{\"token\":\"x\"}").unwrap();
            assert!(
                validate_sync_source_dir(agent, &dir, temp.path()).is_ok(),
                "{agent:?}: valid {name} must be accepted"
            );
            // Wrong folder (no credential file) is rejected.
            let bad = temp.path().join(format!("{agent:?}-bad"));
            std::fs::create_dir_all(&bad).unwrap();
            assert!(validate_sync_source_dir(agent, &bad, temp.path()).is_err());
        }
    }

    #[test]
    fn validate_kimi_requires_config_and_credentials_tree() {
        let temp = tempdir().unwrap();
        let good = temp.path().join("kimi-good");
        std::fs::create_dir_all(good.join("credentials")).unwrap();
        std::fs::write(good.join("config.toml"), "x = 1\n").unwrap();
        assert!(validate_sync_source_dir(Agent::Kimi, &good, temp.path()).is_ok());

        // config.toml present but no credentials/ dir → rejected.
        let bad = temp.path().join("kimi-bad");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("config.toml"), "x = 1\n").unwrap();
        assert!(validate_sync_source_dir(Agent::Kimi, &bad, temp.path()).is_err());
    }
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
fn ignore_mode_writes_empty_json() {
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(state.claude_account_json().unwrap()).unwrap(),
        "{}"
    );
    assert!(!state.claude_credentials_json().unwrap().exists());
    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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

    // First run: create the file with ignore mode (gets 0600)
    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Ignore,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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

    // Sync with host auth — must tighten permissions
    seed_host_auth(&temp);
    let (state2, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
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
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("symlink"),
        "expected symlink error, got: {err}"
    );

    // Decoy file must be untouched
    assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "secret");
}
