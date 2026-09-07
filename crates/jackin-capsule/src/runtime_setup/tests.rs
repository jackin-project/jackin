// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `runtime_setup`.
use super::*;
use std::fs;
use std::sync::{
    Arc, Barrier,
    atomic::{AtomicBool, Ordering},
};

#[test]
fn runtime_setup_process_boundary_classifies_and_redacts_failures() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        let success = runtime_setup_request(&jackin_process::ExecRequest::new(
            "sh",
            ["-c", "printf operator-secret-success"],
        ))
        .expect("successful process request");
        assert!(success.success);

        let nonzero = runtime_setup_request(&jackin_process::ExecRequest::new(
            "sh",
            ["-c", "printf operator-secret-failure >&2; exit 7"],
        ))
        .expect("nonzero process result");
        assert!(!nonzero.success);

        let spawn_error = runtime_setup_request(&jackin_process::ExecRequest::new(
            "operator-secret-missing-program",
            std::iter::empty::<&str>(),
        ))
        .err();
        assert!(spawn_error.is_some());
    });
    export.force_flush();

    let spans = export.finished_spans();
    assert_eq!(spans.len(), 3);
    assert!(
        spans
            .iter()
            .all(|span| span.name == jackin_telemetry::schema::spans::PROCESS_COMMAND)
    );
    assert_eq!(export.error_span_count(), 2);
    assert!(export.contains_span_text("process_exit_nonzero"));
    assert!(export.contains_span_text("process_spawn_error"));
    for secret in [
        "operator-secret-success",
        "operator-secret-failure",
        "operator-secret-missing-program",
    ] {
        assert!(!export.contains_span_text(secret));
        assert!(!export.contains_log_text(secret));
    }
}

#[test]
fn container_init_marker_is_container_local() {
    assert_eq!(CONTAINER_INIT_MARKER, "/jackin/state/container-init.done");
}

#[test]
fn apply_forwarded_credential_first_seed_reseed_and_no_clobber() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let forwarded = tmp.path().join("forwarded.json");
    let target = tmp.path().join("auth.json");
    fs::write(&forwarded, b"FORWARDED").expect("write forwarded");
    // `api_key_envs: &[]` keeps the policy deterministic — no env reads.
    let spec = ForwardedCredential {
        label: "test",
        forwarded: &forwarded,
        target: &target,
        api_key_envs: &[],
    };

    // First seed with a forwarded file: seeds the target.
    apply_forwarded_credential(true, AuthMode::Sync, &spec).expect("first seed");
    assert_eq!(fs::read_to_string(&target).unwrap(), "FORWARDED");

    // Later launch with the target present: a token the agent refreshed
    // in-container is never clobbered.
    fs::write(&target, b"REFRESHED").unwrap();
    apply_forwarded_credential(false, AuthMode::Sync, &spec).expect("no-clobber");
    assert_eq!(fs::read_to_string(&target).unwrap(), "REFRESHED");

    // Later launch with the target missing but forwarded present: re-seeds.
    fs::remove_file(&target).unwrap();
    apply_forwarded_credential(false, AuthMode::Sync, &spec).expect("re-seed");
    assert_eq!(fs::read_to_string(&target).unwrap(), "FORWARDED");

    // First seed with no forwarded file and no api key: clears the stale target.
    fs::write(&target, b"STALE").unwrap();
    fs::remove_file(&forwarded).unwrap();
    apply_forwarded_credential(true, AuthMode::Sync, &spec).expect("first seed without forward");
    assert!(!target.exists(), "stale target must be removed");
}

#[test]
fn bounded_modes_remove_stale_credentials_and_classify_unavailable_material() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let forwarded = tmp.path().join("private-forwarded.json");
    let target = tmp.path().join("private-auth.json");
    fs::write(&target, b"STALE_SECRET").unwrap();
    let spec = ForwardedCredential {
        label: "test",
        forwarded: &forwarded,
        target: &target,
        api_key_envs: &[],
    };

    let ignored = apply_forwarded_credential(false, AuthMode::Ignore, &spec).unwrap();
    assert!(!target.exists());
    assert_eq!(ignored.outcome.as_str(), "skip");
    assert_eq!(ignored.source.as_str(), "none");

    fs::write(&target, b"STALE_SECRET").unwrap();
    let unavailable = apply_forwarded_credential(false, AuthMode::ApiKey, &spec).unwrap();
    assert!(!target.exists());
    assert_eq!(unavailable.outcome.as_str(), "failure");
    assert_eq!(unavailable.source.as_str(), "none");
    assert_eq!(
        unavailable.error.unwrap().as_str(),
        "credential_unavailable"
    );
}

#[test]
fn capsule_auth_provision_event_is_exactly_once_bounded_and_private() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let materialization = AuthMaterialization {
        source: jackin_telemetry::schema::enums::CredentialSourceType::Environment,
        outcome: jackin_telemetry::schema::enums::OutcomeValue::Error,
        error: Some(jackin_telemetry::schema::enums::ErrorType::IoError),
    };

    tracing::subscriber::with_default(subscriber, || {
        emit_capsule_auth_provision("codex", AuthMode::ApiKey, Ok(&materialization));
    });
    export.force_flush();

    assert_eq!(export.event_count("auth.provision"), 1);
    assert!(export.contains_log_text("codex"));
    assert!(export.contains_log_text("api_key"));
    assert!(export.contains_log_text("environment"));
    assert!(export.contains_log_text("io_error"));
    assert!(!export.contains_log_text("private-forwarded.json"));
    assert!(!export.contains_log_text("STALE_SECRET"));
}

// ── Agent config-dir env resolution ─────────────────────────────────
// Pure `_from` cores so no process-global env mutation is needed.

#[test]
fn claude_paths_default_when_config_dir_unset() {
    // Unset: credentials live inside ~/.claude, but .claude.json sits at the
    // home root — the asymmetry jackin must preserve.
    assert_eq!(
        claude_config_dir_from(None),
        PathBuf::from("/home/agent/.claude")
    );
    assert_eq!(
        claude_account_path_from(None),
        PathBuf::from("/home/agent/.claude.json")
    );
}

#[test]
fn claude_paths_follow_config_dir_when_set() {
    // Set: BOTH .credentials.json and .claude.json move inside the dir. This is
    // the regression fix — previously .claude.json stayed at the home root and
    // the CLI fell back to the login screen.
    let dir = "/home/agent/.claude-work";
    assert_eq!(claude_config_dir_from(Some(dir)), PathBuf::from(dir));
    assert_eq!(
        claude_account_path_from(Some(dir)),
        PathBuf::from("/home/agent/.claude-work/.claude.json")
    );
    assert_eq!(
        claude_config_dir_from(Some(dir)).join(".credentials.json"),
        PathBuf::from("/home/agent/.claude-work/.credentials.json")
    );
}

#[test]
fn codex_home_honors_env_else_defaults() {
    assert_eq!(codex_home_from(None), PathBuf::from("/home/agent/.codex"));
    assert_eq!(
        codex_home_from(Some("/home/agent/.codex-alt")).join("auth.json"),
        PathBuf::from("/home/agent/.codex-alt/auth.json")
    );
}

// ── claude_plugin_fingerprint ────────────────────────────────────────

#[test]
fn fingerprint_empty_config_is_empty() {
    let config = jackin_protocol::CapsuleConfig::default();
    assert_eq!(claude_plugin_fingerprint(&config), "");
}

#[test]
fn fingerprint_marketplace_no_sparse() {
    let config = jackin_protocol::CapsuleConfig {
        claude_marketplaces: vec![jackin_protocol::ClaudeMarketplace {
            source: "org/repo".to_owned(),
            sparse: vec![],
        }],
        ..Default::default()
    };
    assert_eq!(claude_plugin_fingerprint(&config), "m:org/repo\n");
}

#[test]
fn fingerprint_marketplace_with_sparse_paths() {
    let config = jackin_protocol::CapsuleConfig {
        claude_marketplaces: vec![jackin_protocol::ClaudeMarketplace {
            source: "org/repo".to_owned(),
            sparse: vec!["tools/a".to_owned(), "tools/b".to_owned()],
        }],
        ..Default::default()
    };
    assert_eq!(
        claude_plugin_fingerprint(&config),
        "m:org/repo tools/a tools/b\n"
    );
}

#[test]
fn fingerprint_plugin_only() {
    let config = jackin_protocol::CapsuleConfig {
        claude_plugins: vec!["my-plugin".to_owned()],
        ..Default::default()
    };
    assert_eq!(claude_plugin_fingerprint(&config), "p:my-plugin\n");
}

#[test]
fn fingerprint_mixed_marketplace_and_plugins() {
    let config = jackin_protocol::CapsuleConfig {
        claude_marketplaces: vec![jackin_protocol::ClaudeMarketplace {
            source: "org/tools".to_owned(),
            sparse: vec!["fmt".to_owned()],
        }],
        claude_plugins: vec!["fmt-plugin".to_owned(), "lint-plugin".to_owned()],
        ..Default::default()
    };
    assert_eq!(
        claude_plugin_fingerprint(&config),
        "m:org/tools fmt\np:fmt-plugin\np:lint-plugin\n"
    );
}

#[test]
fn xdg_data_home_drives_amp_and_opencode() {
    assert_eq!(
        xdg_data_home_from(None),
        PathBuf::from("/home/agent/.local/share")
    );
    let xdg = "/home/agent/.xdg-data";
    assert_eq!(
        xdg_data_home_from(Some(xdg)).join("amp/secrets.json"),
        PathBuf::from("/home/agent/.xdg-data/amp/secrets.json")
    );
    assert_eq!(
        xdg_data_home_from(Some(xdg)).join("opencode/auth.json"),
        PathBuf::from("/home/agent/.xdg-data/opencode/auth.json")
    );
}

#[test]
fn seed_home_dir_absent_dst_uses_atomic_rename() {
    // When dst does not exist, seed_home_dir must create it atomically (via a
    // staging dir + rename) and signal FirstSeed. Tests the rename path missed
    // by the other seed tests which pre-create dst.
    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst"); // NOT created — exercises the rename path
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("config.json"), b"{}").unwrap();

    let outcome = seed_home_dir(&src, &dst).expect("atomic seed should succeed");
    assert_eq!(outcome, SeedOutcome::FirstSeed);
    assert!(
        dst.join("config.json").exists(),
        "renamed tree must contain seeded file"
    );
    // No stale staging dirs should remain beside dst after a successful rename.
    let siblings: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(std::result::Result::ok)
        .collect();
    assert!(
        !siblings
            .iter()
            .any(|e| e.file_name().to_string_lossy().starts_with(".jackin-seed")),
        "staging dir must be cleaned up after successful rename"
    );
}

#[test]
fn is_dir_empty_treats_read_error_as_nonempty() {
    // A path that does not exist causes read_dir to fail; must return false
    // (non-empty = conservative) rather than true (empty = would trigger first-seed).
    assert!(!is_dir_empty(Path::new(
        "/nonexistent/path/that/cannot/exist"
    )));
}

#[test]
fn runtime_setup_runs_agent_setup_while_container_init_is_foreground() {
    // A two-party Barrier proves the foreground and agent-setup closures run
    // concurrently without a flaky bounded spin: foreground cannot pass the
    // barrier until the spawned agent thread also reaches it, so the test only
    // completes if both run at once. A bounded `yield_now` loop instead raced
    // the scheduler and spuriously failed on a busy/low-core CI runner.
    let barrier = Arc::new(Barrier::new(2));
    let barrier_for_thread = Arc::clone(&barrier);

    run_runtime_setup_concurrently(
        move || {
            barrier.wait();
            Ok(())
        },
        || Ok(()),
        || {},
        move || {
            barrier_for_thread.wait();
            Ok(())
        },
    )
    .expect("runtime setup should complete");
}

#[test]
fn runtime_setup_surfaces_agent_setup_failure_after_foreground_work() {
    let foreground_finished = Arc::new(AtomicBool::new(false));
    let foreground_finished_for_check = Arc::clone(&foreground_finished);

    let err = run_runtime_setup_concurrently(
        || Ok(()),
        || Ok(()),
        move || {
            foreground_finished.store(true, Ordering::SeqCst);
        },
        || anyhow::bail!("agent boom"),
    )
    .unwrap_err();

    assert!(foreground_finished_for_check.load(Ordering::SeqCst));
    assert!(err.to_string().contains("agent boom"));
}

#[test]
fn reporter_install_failure_message_names_agent_and_error() {
    let err = anyhow::anyhow!("plugins.json is not valid JSON");
    let message = reporter_install_failure_message("opencode", &err);

    assert!(message.contains("agent-status: reporter install for opencode failed"));
    assert!(message.contains("non-fatal"));
    assert!(message.contains("plugins.json is not valid JSON"));
}

#[test]
fn seed_home_dir_empty_dst_seeds_from_src_and_signals_first_seed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("config.json"), b"{}").unwrap();
    fs::create_dir(&dst).unwrap(); // empty

    let outcome = seed_home_dir(&src, &dst).expect("seed should succeed");
    assert_eq!(outcome, SeedOutcome::FirstSeed, "empty dst → first seed");
    assert!(dst.join("config.json").exists(), "file copied to dst");
}

#[test]
fn seed_home_dir_nonempty_dst_skips_and_signals_already_seeded() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("default.json"), b"{}").unwrap();
    fs::create_dir_all(&dst).unwrap();
    // dst has a user file → non-empty
    fs::write(dst.join("user.json"), b"{}").unwrap();

    let outcome = seed_home_dir(&src, &dst).expect("skip should succeed");
    assert_eq!(
        outcome,
        SeedOutcome::AlreadySeeded,
        "non-empty dst → already seeded"
    );
    assert!(
        !dst.join("default.json").exists(),
        "src files not copied into non-empty dst"
    );
}

#[test]
fn seed_home_dir_absent_src_still_signals_first_seed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src-absent");
    let dst = tmp.path().join("dst");
    fs::create_dir(&dst).unwrap(); // empty

    let outcome = seed_home_dir(&src, &dst).expect("no-src seed should succeed");
    assert_eq!(
        outcome,
        SeedOutcome::FirstSeed,
        "absent src + empty dst → still first seed (auth may be copied)"
    );
}

#[test]
fn seed_agent_home_seeds_data_and_paired_config_in_one_transaction() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_src = tmp.path().join("default/data");
    let cfg_src = tmp.path().join("default/config");
    let data_dst = tmp.path().join("home/data");
    let cfg_dst = tmp.path().join("home/config");
    fs::create_dir_all(&data_src).unwrap();
    fs::create_dir_all(&cfg_src).unwrap();
    fs::write(data_src.join("state.json"), b"{}").unwrap();
    fs::write(cfg_src.join("settings.json"), b"{}").unwrap();
    fs::create_dir_all(&data_dst).unwrap(); // empty
    fs::create_dir_all(&cfg_dst).unwrap(); // empty

    let outcome = seed_agent_home(
        data_src.to_str().unwrap(),
        data_dst.to_str().unwrap(),
        Some((cfg_src.to_str().unwrap(), cfg_dst.to_str().unwrap())),
    )
    .expect("seed should succeed");
    assert_eq!(
        outcome,
        SeedOutcome::FirstSeed,
        "empty data root → first seed"
    );
    assert!(data_dst.join("state.json").exists(), "data root seeded");
    assert!(cfg_dst.join("settings.json").exists(), "config root seeded");
}

#[test]
fn seed_agent_home_nonempty_config_root_leaves_both_untouched() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_src = tmp.path().join("default/data");
    let cfg_src = tmp.path().join("default/config");
    let data_dst = tmp.path().join("home/data");
    let cfg_dst = tmp.path().join("home/config");
    fs::create_dir_all(&data_src).unwrap();
    fs::create_dir_all(&cfg_src).unwrap();
    fs::write(data_src.join("state.json"), b"{}").unwrap();
    fs::create_dir_all(&data_dst).unwrap(); // empty data root
    fs::create_dir_all(&cfg_dst).unwrap();
    fs::write(cfg_dst.join("user.json"), b"{}").unwrap(); // durable config content

    let outcome = seed_agent_home(
        data_src.to_str().unwrap(),
        data_dst.to_str().unwrap(),
        Some((cfg_src.to_str().unwrap(), cfg_dst.to_str().unwrap())),
    )
    .expect("skip should succeed");
    assert_eq!(
        outcome,
        SeedOutcome::AlreadySeeded,
        "non-empty config root → treat as durable, no seed/auth"
    );
    assert!(
        !data_dst.join("state.json").exists(),
        "data root left untouched when config root holds durable state"
    );
}

#[test]
fn seed_agent_home_no_config_root_seeds_data_only() {
    // The single-root agents (claude/codex/grok/kimi) call seed_agent_home with
    // config = None; that branch must seed the data root and signal first seed.
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_src = tmp.path().join("default/data");
    let data_dst = tmp.path().join("home/data");
    fs::create_dir_all(&data_src).unwrap();
    fs::write(data_src.join("state.json"), b"{}").unwrap();
    fs::create_dir_all(&data_dst).unwrap(); // empty

    let outcome = seed_agent_home(data_src.to_str().unwrap(), data_dst.to_str().unwrap(), None)
        .expect("seed should succeed");
    assert_eq!(
        outcome,
        SeedOutcome::FirstSeed,
        "empty data root → first seed"
    );
    assert!(data_dst.join("state.json").exists(), "data root seeded");

    // A second call now sees a non-empty data root → already seeded, no re-copy.
    fs::write(data_src.join("new.json"), b"{}").unwrap();
    let again = seed_agent_home(data_src.to_str().unwrap(), data_dst.to_str().unwrap(), None)
        .expect("second call should succeed");
    assert_eq!(
        again,
        SeedOutcome::AlreadySeeded,
        "non-empty data root → skip"
    );
    assert!(
        !data_dst.join("new.json").exists(),
        "second seed must not copy into a non-empty durable home"
    );
}

#[test]
fn git_hook_marker_is_versioned() {
    assert_eq!(
        GIT_HOOK_MARKER,
        "/jackin/state/git-hooks/prepare-commit-msg.v3.done"
    );
}

#[test]
fn hook_uses_canonical_agent_trailers() {
    assert_eq!(
        coauthor_trailer_for_agent("claude"),
        Some("Co-authored-by: Claude <noreply@anthropic.com>")
    );
    assert_eq!(
        coauthor_trailer_for_agent("codex"),
        Some("Co-authored-by: Codex <codex@openai.com>")
    );
    assert_eq!(
        coauthor_trailer_for_agent("amp"),
        Some("Co-authored-by: Amp <amp@ampcode.com>")
    );
    assert_eq!(
        coauthor_trailer_for_agent("opencode"),
        Some("Co-authored-by: opencode-agent[bot] <opencode-agent[bot]@users.noreply.github.com>")
    );
    assert_eq!(coauthor_trailer_for_agent("kimi"), None);
    assert_eq!(coauthor_trailer_for_agent("grok"), None);
}

#[test]
fn hook_marker_points_at_capsule_runtime_binary() {
    assert_eq!(CAPSULE_RUNTIME_BIN, "/jackin/runtime/jackin-capsule");
}

#[test]
fn enforced_claude_config_keeps_mutable_metadata_inside_directory_mount() {
    let directory = container_paths::CLAUDE_CONFIG_DIR;
    assert_eq!(
        claude_account_path_from(Some(directory)),
        claude_config_dir_from(Some(directory)).join(".claude.json")
    );
    let temporary = tempfile::tempdir().unwrap();
    let metadata = temporary.path().join(".claude.json");
    fs::write(&metadata, b"stale-account").unwrap();
    remove_file_if_exists(&metadata).unwrap();
    let replacement = temporary.path().join(".claude.json.tmp");
    fs::write(&replacement, b"new-account").unwrap();
    fs::rename(&replacement, &metadata).unwrap();
    assert_eq!(fs::read(&metadata).unwrap(), b"new-account");
}
