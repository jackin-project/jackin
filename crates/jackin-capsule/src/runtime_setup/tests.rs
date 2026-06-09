//! Tests for `runtime_setup`.
use super::*;
use std::fs;

#[test]
fn container_init_marker_is_container_local() {
    assert_eq!(CONTAINER_INIT_MARKER, "/jackin/state/container-init.done");
}

#[test]
fn agent_auth_marker_is_agent_scoped() {
    assert_eq!(AGENT_AUTH_MARKER_DIR, "/jackin/state/agent-auth");
    assert_eq!(
        agent_auth_marker_path("claude"),
        PathBuf::from("/jackin/state/agent-auth/claude.done")
    );
    assert_eq!(
        agent_auth_marker_path("codex"),
        PathBuf::from("/jackin/state/agent-auth/codex.done")
    );
}

#[test]
fn agent_auth_marker_records_one_bootstrap_per_agent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let claude_marker = dir.path().join("claude.done");
    let codex_marker = dir.path().join("codex.done");

    assert!(!claude_marker.exists(), "claude auth should copy first");
    assert!(!codex_marker.exists(), "codex auth should copy first");

    mark_agent_auth_initialized(&claude_marker, "claude").expect("mark claude initialized");

    assert!(claude_marker.exists(), "claude auth should be initialized");
    assert!(
        !codex_marker.exists(),
        "codex auth must stay independently uninitialized"
    );

    mark_agent_auth_initialized(&codex_marker, "codex").expect("mark codex initialized");
    assert!(codex_marker.exists(), "codex auth should be initialized");
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
    // Two runs (simulating container reuse) must not duplicate the table.
    write_codex_provider_config_inner(codex_dir, true).expect("first write");
    write_codex_provider_config_inner(codex_dir, true).expect("second write");
    let body = fs::read_to_string(codex_dir.join("config.toml")).expect("read config.toml");
    assert_eq!(
        body.matches("[model_providers.minimax]").count(),
        1,
        "MiniMax provider block must appear exactly once"
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
    write_codex_provider_config_inner(codex_dir, true).expect("write with existing content");
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
    write_codex_provider_config_inner(codex_dir, false).expect("noop write");
    assert!(
        !codex_dir.join("config.toml").exists(),
        "no config.toml should be written when MiniMax key absent"
    );
}
