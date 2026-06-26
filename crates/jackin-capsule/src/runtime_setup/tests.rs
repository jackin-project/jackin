//! Tests for `runtime_setup`.
use super::*;
use std::fs;
use std::sync::{
    Arc, Barrier,
    atomic::{AtomicBool, Ordering},
};

#[test]
fn container_init_marker_is_container_local() {
    assert_eq!(CONTAINER_INIT_MARKER, "/jackin/state/container-init.done");
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
fn opencode_config_blocks_are_self_contained_and_match_picker_models() {
    use jackin_protocol::Provider;
    let cfg = build_opencode_config(
        Some("zai-tok".to_owned()),
        Some("minimax-tok".to_owned()),
        Some("kimi-tok".to_owned()),
    );
    assert_eq!(cfg["permission"], "allow");
    let providers = cfg["provider"].as_object().expect("provider block present");

    // MiniMax/Kimi baseURL carries a `/v1` suffix the Claude-path constant
    // omits: `@ai-sdk/anthropic` appends only `/messages`, not `/v1/messages`.
    for (provider, npm, base_url, api_key) in [
        (
            Provider::Zai,
            "@ai-sdk/openai-compatible",
            jackin_protocol::ZAI_OPENAI_BASE_URL.to_owned(),
            "zai-tok",
        ),
        (
            Provider::Minimax,
            "@ai-sdk/anthropic",
            format!("{}/v1", jackin_protocol::MINIMAX_BASE_URL),
            "minimax-tok",
        ),
        (
            Provider::Kimi,
            "@ai-sdk/anthropic",
            format!("{}/v1", jackin_protocol::KIMI_BASE_URL),
            "kimi-tok",
        ),
    ] {
        // The picker emits the `-m <provider>/<model>` string; the config
        // must define that exact provider id and model id, or the session
        // fails to start.
        let flag = provider
            .opencode_model()
            .expect("alt provider has -m string");
        let (provider_id, model_id) = flag.split_once('/').expect("provider/model shape");
        let block = providers
            .get(provider_id)
            .unwrap_or_else(|| panic!("config missing provider {provider_id}"));
        assert_eq!(block["npm"], npm);
        assert_eq!(block["options"]["baseURL"], base_url);
        assert_eq!(block["options"]["apiKey"], api_key);
        assert!(
            block["models"].get(model_id).is_some(),
            "provider {provider_id} block missing model {model_id}"
        );
    }
}

#[test]
fn opencode_config_omits_absent_providers() {
    let cfg = build_opencode_config(None, None, None);
    assert_eq!(cfg["permission"], "allow");
    assert!(cfg.get("provider").is_none());
}

#[test]
fn opencode_json_is_written_owner_only() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("opencode.json");
    let cfg = build_opencode_config(Some("zai-tok".to_owned()), None, None);
    write_opencode_json(&path, &cfg).expect("write opencode.json");
    let mode = fs::metadata(&path).expect("metadata").permissions().mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "opencode.json must be 0o600, got {:o}",
        mode & 0o777
    );
}

#[test]
fn codex_provider_config_is_idempotent_across_repeated_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let codex_dir = dir.path();
    // Two runs (simulating container reuse) must not duplicate the table or profile.
    write_codex_provider_config_inner(codex_dir, true, jackin_protocol::MINIMAX_DEFAULT_MODEL)
        .expect("first write");
    write_codex_provider_config_inner(codex_dir, true, jackin_protocol::MINIMAX_DEFAULT_MODEL)
        .expect("second write");
    let body = fs::read_to_string(codex_dir.join("config.toml")).expect("read config.toml");
    assert_eq!(
        body.matches("[model_providers.minimax]").count(),
        1,
        "MiniMax provider block must appear exactly once"
    );
    assert!(
        codex_dir.join("minimax.config.toml").exists(),
        "minimax.config.toml profile file must exist"
    );
}

#[test]
fn codex_provider_config_writes_v2_profile_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let codex_dir = dir.path();
    write_codex_provider_config_inner(codex_dir, true, jackin_protocol::MINIMAX_DEFAULT_MODEL)
        .expect("write");
    let profile = fs::read_to_string(codex_dir.join("minimax.config.toml")).expect("read profile");
    assert!(
        profile.contains("model_provider = \"minimax\""),
        "profile must set model_provider"
    );
    assert!(
        profile.contains("model = \"MiniMax-M3\""),
        "profile must pin the MiniMax model"
    );
    // The context window lives in the catalog (minimax.models.json), not the
    // profile: a profile-scoped model_context_window is clamped to the fallback.
    assert!(
        !profile.contains("model_context_window"),
        "context window must not be set in the profile (it would be clamped there)"
    );
    // Legacy [profiles.minimax] table must NOT be in config.toml — Codex
    // errors if both --profile and a legacy profiles table exist.
    let config = fs::read_to_string(codex_dir.join("config.toml")).expect("read config.toml");
    assert!(
        !config.contains("[profiles.minimax]"),
        "legacy profiles table must not be written to config.toml"
    );
}

#[test]
fn codex_provider_config_honors_model_override() {
    // A role's [codex.providers.minimax].model override reaches the profile.
    let dir = tempfile::tempdir().expect("tempdir");
    let codex_dir = dir.path();
    write_codex_provider_config_inner(codex_dir, true, "MiniMax-Pro").expect("write");
    let profile = fs::read_to_string(codex_dir.join("minimax.config.toml")).expect("read profile");
    assert!(
        profile.contains("model = \"MiniMax-Pro\""),
        "profile must carry the per-provider override model"
    );
}

#[test]
fn codex_provider_config_preserves_operator_content() {
    let dir = tempfile::tempdir().expect("tempdir");
    let codex_dir = dir.path();
    let config_path = codex_dir.join("config.toml");
    fs::write(
        &config_path,
        b"# existing operator config\n[settings]\nsome_key = true\n",
    )
    .expect("write existing config");
    write_codex_provider_config_inner(codex_dir, true, jackin_protocol::MINIMAX_DEFAULT_MODEL)
        .expect("write with existing content");
    let body = fs::read_to_string(&config_path).expect("read config.toml");
    // Original content preserved.
    assert!(body.contains("# existing operator config"));
    assert!(body.contains("some_key = true"));
    // Provider block appended.
    assert!(body.contains("[model_providers.minimax]"));
}

#[test]
fn codex_provider_config_noop_without_minimax_key() {
    let dir = tempfile::tempdir().expect("tempdir");
    let codex_dir = dir.path();
    write_codex_provider_config_inner(codex_dir, false, jackin_protocol::MINIMAX_DEFAULT_MODEL)
        .expect("noop write");
    assert!(
        !codex_dir.join("config.toml").exists(),
        "no config.toml should be written when MiniMax key absent"
    );
    assert!(
        !codex_dir.join("minimax.config.toml").exists(),
        "no minimax.config.toml should be written when MiniMax key absent"
    );
    assert!(
        !codex_dir.join("minimax.models.json").exists(),
        "no model catalog should be written when MiniMax key absent"
    );
}

#[test]
fn build_minimax_catalog_patches_identity_and_window() {
    // A representative Codex catalog entry (trimmed) as the template.
    let template = serde_json::json!({
        "slug": "gpt-5.5",
        "display_name": "GPT-5.5",
        "description": "Frontier model.",
        "context_window": 272_000,
        "max_context_window": 272_000,
        "auto_compact_token_limit": null,
        "availability_nux": { "message": "promo" },
        "upgrade": null,
        "shell_type": "shell_command",
        "supports_parallel_tool_calls": true
    });
    let template = template.as_object().expect("template object").clone();
    // A non-default model proves the catalog slug is driven by the `model`
    // argument (the per-provider override), not a hardcoded constant.
    let catalog = build_minimax_catalog(&template, "MiniMax-Custom");
    let models = catalog["models"].as_array().expect("models array");
    assert_eq!(models.len(), 1);
    let entry = &models[0];
    // Identity rewritten to the passed model so it matches the profile's `model`.
    assert_eq!(entry["slug"], "MiniMax-Custom");
    assert_eq!(entry["display_name"], "MiniMax-Custom");
    // Real MiniMax window, lifting the fallback cap; compact at 90% of it.
    let window = jackin_protocol::MINIMAX_CONTEXT_WINDOW;
    assert_eq!(entry["context_window"], window);
    assert_eq!(entry["max_context_window"], window);
    assert_eq!(entry["auto_compact_token_limit"], window * 9 / 10);
    // Template-model promo field cleared.
    assert!(entry["availability_nux"].is_null());
    // Capability fields carry over from the template untouched.
    assert_eq!(entry["shell_type"], "shell_command");
    assert_eq!(entry["supports_parallel_tool_calls"], true);
}
