// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn dockerignore_capsule_allowlist_requires_staged_capsule() {
    let tmp = tempdir().unwrap();
    let context_dir = tmp.path();
    std::fs::write(context_dir.join(".dockerignore"), "*\n").unwrap();
    std::fs::create_dir_all(context_dir.join(".jackin-runtime")).unwrap();
    std::fs::write(
        context_dir.join(".jackin-runtime/entrypoint.sh"),
        "#!/bin/sh\n",
    )
    .unwrap();
    std::fs::write(
        context_dir.join(".jackin-runtime/DerivedDockerfile"),
        "FROM scratch\n",
    )
    .unwrap();

    ensure_runtime_assets_are_included(context_dir, None).unwrap();
    let dockerignore = std::fs::read_to_string(context_dir.join(".dockerignore")).unwrap();

    assert!(dockerignore.contains("!.jackin-runtime/entrypoint.sh"));
    assert!(dockerignore.contains("!.jackin-runtime/zsh-title-shim"));
    assert!(dockerignore.contains("!.jackin-runtime/DerivedDockerfile"));
    assert!(!dockerignore.contains("!.jackin-runtime/jackin-capsule"));
    assert!(!dockerignore.contains("!.jackin-runtime/zshenv-source-shim"));

    std::fs::write(
        context_dir.join(".jackin-runtime/jackin-capsule"),
        b"capsule",
    )
    .unwrap();
    ensure_runtime_assets_are_included(context_dir, None).unwrap();
    let dockerignore = std::fs::read_to_string(context_dir.join(".dockerignore")).unwrap();
    assert!(dockerignore.contains("!.jackin-runtime/jackin-capsule"));
}

#[test]
fn dockerignore_source_shim_allowlist_requires_source_hook_asset() {
    let tmp = tempdir().unwrap();
    let context_dir = tmp.path();
    std::fs::create_dir_all(context_dir.join(".jackin-runtime")).unwrap();

    ensure_runtime_assets_are_included(context_dir, None).unwrap();
    let dockerignore = std::fs::read_to_string(context_dir.join(".dockerignore")).unwrap();
    assert!(!dockerignore.contains("!.jackin-runtime/zshenv-source-shim"));

    std::fs::write(
        context_dir.join(".jackin-runtime/zshenv-source-shim"),
        "# shim\n",
    )
    .unwrap();
    ensure_runtime_assets_are_included(context_dir, None).unwrap();
    let dockerignore = std::fs::read_to_string(context_dir.join(".dockerignore")).unwrap();
    assert!(dockerignore.contains("!.jackin-runtime/zshenv-source-shim"));
}

#[test]
fn dockerignore_agent_binary_allowlist_requires_staged_binary_dir() {
    let tmp = tempdir().unwrap();
    let context_dir = tmp.path();
    std::fs::create_dir_all(context_dir.join(".jackin-runtime")).unwrap();

    ensure_runtime_assets_are_included(context_dir, None).unwrap();
    let dockerignore = std::fs::read_to_string(context_dir.join(".dockerignore")).unwrap();
    assert!(!dockerignore.contains("!.jackin-runtime/agent-binaries/"));

    std::fs::create_dir_all(context_dir.join(".jackin-runtime/agent-binaries")).unwrap();
    std::fs::write(
        context_dir.join(".jackin-runtime/agent-binaries/claude"),
        b"binary",
    )
    .unwrap();
    ensure_runtime_assets_are_included(context_dir, None).unwrap();
    let dockerignore = std::fs::read_to_string(context_dir.join(".dockerignore")).unwrap();
    assert!(dockerignore.contains("!.jackin-runtime/agent-binaries/"));
    assert!(dockerignore.contains("!.jackin-runtime/agent-binaries/claude"));
    assert!(!dockerignore.contains("!.jackin-runtime/agent-binaries/*"));
}

#[test]
fn renders_codex_only_dockerfile_final_user_is_agent() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Codex],
        None,
        &BTreeMap::new(),
        None,
    );
    let last_user = dockerfile
        .lines()
        .rfind(|l| l.starts_with("USER "))
        .unwrap();
    assert_eq!(last_user, "USER agent");
    let cleanup_pos = dockerfile
        .find("rm -rf '/home/agent/.codex/tmp'")
        .expect("codex temp cleanup");
    let snapshot_pos = dockerfile
        .find("mv \"/home/agent/$dir\" \"/jackin/default-home/$dir\"")
        .expect("default-home snapshot move");
    assert!(
        cleanup_pos < snapshot_pos,
        "Codex scratch files must be removed before default-home snapshot: {dockerfile}"
    );
}

#[test]
fn default_home_snapshot_removes_agent_generated_private_files_before_move() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude, Agent::Codex, Agent::Opencode],
        None,
        &BTreeMap::new(),
        None,
    );

    let cleanup_pos = dockerfile.find("rm -rf").expect("cleanup command");
    let snapshot_pos = dockerfile
        .find("mv \"/home/agent/$dir\" \"/jackin/default-home/$dir\"")
        .expect("default-home snapshot move");
    let chmod_pos = dockerfile
        .find("find /jackin/default-home -type f -exec chmod g+r {} +")
        .expect("default-home chmod");
    let guard_pos = dockerfile
        .find("jackin default-home contains a non-group-readable path")
        .expect("default-home guard");
    for path in [
        "'/home/agent/.claude/backups'",
        "'/home/agent/.codex/tmp'",
        "'/home/agent/.config/opencode/opencode.json'",
    ] {
        assert!(
            dockerfile.contains(path),
            "missing generated private-file cleanup for {path}: {dockerfile}"
        );
    }
    assert!(
        cleanup_pos < snapshot_pos,
        "generated private files must be removed before default-home snapshot: {dockerfile}"
    );
    assert!(
        snapshot_pos < chmod_pos && chmod_pos < guard_pos,
        "remaining durable seed files must be normalized before the guard: {dockerfile}"
    );
}

#[test]
fn renders_dockerfile_targets_agent_user_not_claude() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude],
        None,
        &BTreeMap::new(),
        None,
    );

    assert!(dockerfile.contains("/home/agent"));
    assert!(!dockerfile.contains("groupmod "));
    assert!(!dockerfile.contains("usermod "));
    assert!(dockerfile.contains(
        "install -d -o agent -g 0 /jackin/run /jackin/state /jackin/account-credentials"
    ));
    assert!(!dockerfile.contains("chown agent:agent /jackin/run /jackin/state"));
    assert!(!dockerfile.contains("chown -R agent:agent /jackin/state"));
    assert!(dockerfile.contains("ENTRYPOINT [\"/jackin/runtime/jackin-capsule\"]"));
}

#[test]
fn renders_dockerfile_does_not_set_jackin_agent_env() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude, Agent::Codex],
        None,
        &BTreeMap::new(),
        None,
    );

    assert!(!dockerfile.contains("ENV JACKIN_AGENT"));
}

#[test]
fn entrypoint_does_not_override_claude_env() {
    assert!(!ENTRYPOINT_SH.contains("JACKIN="));
}

#[test]
fn entrypoint_dispatches_on_jackin_agent() {
    assert!(ENTRYPOINT_SH.contains("case \"${JACKIN_AGENT:?"));
    assert!(ENTRYPOINT_SH.contains("  claude)"));
    assert!(ENTRYPOINT_SH.contains("  codex)"));
    assert!(ENTRYPOINT_SH.contains("  amp)"));
    assert!(ENTRYPOINT_SH.contains("  kimi)"));
    assert!(ENTRYPOINT_SH.contains("  opencode)"));
}

#[test]
fn entrypoint_does_not_install_claude_plugins_at_runtime() {
    assert!(!ENTRYPOINT_SH.contains("install-claude-plugins.sh"));
}

#[test]
fn entrypoint_codex_branch_does_not_invoke_install_claude_plugins() {
    let codex_section = ENTRYPOINT_SH
        .split("codex)")
        .nth(1)
        .unwrap()
        .split(";;")
        .next()
        .unwrap();
    assert!(!codex_section.contains("install-claude-plugins.sh"));
}

#[test]
fn entrypoint_codex_branch_uses_cli_flags_not_generated_config() {
    let codex_section = ENTRYPOINT_SH
        .split("codex)")
        .nth(1)
        .unwrap()
        .split(";;")
        .next()
        .unwrap();
    assert!(
        codex_section.contains("codex --enable goals --dangerously-bypass-approvals-and-sandbox")
    );
    assert!(codex_section.contains("LAUNCH+=(\"$@\")"));
    assert!(!codex_section.contains("config.toml"));
}

#[test]
fn entrypoint_claude_branch_skips_dangerous_mode_prompt() {
    let claude_section = ENTRYPOINT_SH
        .split("claude)")
        .nth(1)
        .unwrap()
        .split(";;")
        .next()
        .unwrap();
    assert!(
            claude_section
                .contains("claude --settings '{\"skipDangerousModePermissionPrompt\":true}' --dangerously-skip-permissions --verbose")
        );
}

#[test]
fn entrypoint_amp_branch_launches_amp() {
    let amp_section = ENTRYPOINT_SH
        .split_once("\n  amp)")
        .unwrap()
        .1
        .split(";;")
        .next()
        .unwrap();
    assert!(amp_section.contains("LAUNCH=(amp --dangerously-allow-all)"));
    assert!(!amp_section.contains("/jackin/amp/secrets.json"));
}

#[test]
fn entrypoint_kimi_branch_forwards_model_args() {
    let kimi_section = ENTRYPOINT_SH
        .split_once("\n  kimi)")
        .unwrap()
        .1
        .split(";;")
        .next()
        .unwrap();
    assert!(kimi_section.contains("LAUNCH=(kimi --yolo)"));
    assert!(kimi_section.contains("LAUNCH+=(\"$@\")"));
    // Guard against re-adding incompatible flags (--yolo and --auto are mutually exclusive).
    assert!(!kimi_section.contains("--auto"));
}

#[test]
fn entrypoint_opencode_branch_allows_permissions_with_inline_config() {
    let opencode_section = ENTRYPOINT_SH
        .split_once("\n  opencode)")
        .unwrap()
        .1
        .split(";;")
        .next()
        .unwrap();
    assert!(
        opencode_section.contains("export OPENCODE_CONFIG_CONTENT='{\"permission\":\"allow\"}'")
    );
    assert!(opencode_section.contains("LAUNCH=(opencode)"));
    assert!(opencode_section.contains("LAUNCH+=(\"$@\")"));
}

#[test]
fn entrypoint_delegates_agent_home_setup_to_jackin_capsule() {
    assert!(ENTRYPOINT_SH.contains("/jackin/runtime/jackin-capsule runtime-setup"));
    assert!(!ENTRYPOINT_SH.contains("seed_home_dir"));
    assert!(!ENTRYPOINT_SH.contains("/jackin/default-home/.claude"));
    assert!(!ENTRYPOINT_SH.contains("/jackin/default-home/.codex"));
    assert!(!ENTRYPOINT_SH.contains("/jackin/default-home/.local/share/amp"));
    assert!(!ENTRYPOINT_SH.contains("/jackin/default-home/.local/share/opencode"));
}

#[test]
fn derived_image_snapshots_agent_home_defaults() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[
            Agent::Claude,
            Agent::Codex,
            Agent::Amp,
            Agent::Kimi,
            Agent::Opencode,
            Agent::Grok,
        ],
        None,
        &BTreeMap::new(),
        None,
    );

    // The snapshot roots (data + paired config) are the `for dir in …` list,
    // sorted, moved by one templated mv. Targets are NOT pre-created (so the mv
    // renames onto them instead of nesting `.claude/.claude`).
    assert!(dockerfile.contains(
        "for dir in '.claude' '.codex' '.config/amp' '.config/opencode' '.grok' '.kimi-code' '.local/share/amp' '.local/share/opencode'; do"
    ));
    assert!(
        !dockerfile.contains("/jackin/default-home/.claude /jackin/default-home/.codex"),
        "per-agent targets must not be pre-created in install -d: {dockerfile}"
    );
    assert_eq!(
        dockerfile.matches("mv \"/home/agent/$dir\"").count(),
        1,
        "default-home snapshot should not emit one mv command per agent: {dockerfile}"
    );
}

#[test]
fn derived_image_snapshots_only_selected_agent_home_defaults() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude],
        None,
        &BTreeMap::new(),
        None,
    );

    // Only Claude's root is in the snapshot loop; sibling roots are absent.
    assert!(dockerfile.contains("for dir in '.claude'; do"));
    for name in ["'.codex'", "'.local/share/amp'", "'.kimi-code'", "'.grok'"] {
        assert!(
            !dockerfile.contains(name),
            "selected Claude image should not snapshot sibling home {name}: {dockerfile}"
        );
    }
}
