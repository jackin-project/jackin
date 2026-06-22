//! Tests for `instance/auth` — github auth tests.
// The `gh auth token` shellout in `read_host_gh_token` is gated on
// `host_home_is_real(host_home)` — every test in this module passes
// a temp-dir `host_home` so the shellout is skipped and the real
// host's `gh` binary cannot leak into hermetic tests. The file
// fallback is the only path exercised here. See
// `read_host_gh_token` source for the gate.
use super::super::{GithubAuthMode, parse_gh_hosts_yml};
use crate::instance::{
    GithubAuthContext, GithubProvisionKind, GithubProvisionOutcome, GithubTokenSource,
    HostMissingReason, RoleState,
};
use tempfile::tempdir;

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
fn sync_skips_write_when_content_unchanged() {
    let temp = tempdir().unwrap();
    let host_home = stage_host_hosts_yml(&temp, "ghp_unchanged");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    RoleState::provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home)
        .unwrap();
    let mtime_first = std::fs::metadata(&hosts_yml).unwrap().modified().unwrap();

    #[expect(
        clippy::disallowed_methods,
        reason = "mtime idempotency test needs a wall-clock boundary before checking no rewrite"
    )]
    std::thread::sleep(std::time::Duration::from_millis(1100));
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
