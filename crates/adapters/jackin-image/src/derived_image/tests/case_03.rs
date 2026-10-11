// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn claude_plugins_render_one_readable_run_layer() {
    let claude = ClaudeConfig {
        model: None,
        marketplaces: vec![jackin_core::ClaudeMarketplaceConfig {
            source: "myorg/marketplace".to_owned(),
            sparse: vec!["pkg/a".to_owned()],
        }],
        plugins: vec!["caveman".to_owned(), "rtk".to_owned()],
    };
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude],
        None,
        &BTreeMap::new(),
        Some(&claude),
    );

    assert_eq!(
        dockerfile
            .matches("RUN set -eu; \\\n    (claude plugin marketplace add anthropics/claude-plugins-official || true)")
            .count(),
        1,
        "Claude plugin installs should share one RUN layer: {dockerfile}"
    );
    assert_eq!(
        dockerfile.matches("claude plugin install ").count(),
        2,
        "each plugin command remains visible: {dockerfile}"
    );
    assert!(dockerfile.contains("    claude plugin install 'caveman'"));
    assert!(dockerfile.contains("    claude plugin install 'rtk'"));
    // Official + configured marketplace are readable, with --sparse passed.
    assert!(
        dockerfile
            .contains("(claude plugin marketplace add anthropics/claude-plugins-official || true)")
    );
    assert!(
        dockerfile
            .contains("    claude plugin marketplace add 'myorg/marketplace' --sparse 'pkg/a'")
    );
    assert!(
        !dockerfile.contains(" && claude plugin"),
        "plugin steps must stay as readable continued commands, not && chains: {dockerfile}"
    );
    let install_pos = dockerfile
        .find("    claude plugin install 'rtk'")
        .expect("plugin install");
    let cleanup_pos = dockerfile
        .find("    rm -rf /home/agent/.claude/backups")
        .expect("claude backup cleanup");
    assert!(
        install_pos < cleanup_pos,
        "Claude plugin rollback backups are transient installer artifacts and must be removed before default-home snapshot: {dockerfile}"
    );
}

#[test]
fn entrypoint_delegates_security_tool_mcp_registration_to_jackin_capsule() {
    let claude_section = ENTRYPOINT_SH
        .split("claude)")
        .nth(1)
        .unwrap()
        .split(";;")
        .next()
        .unwrap();
    assert!(claude_section.contains("LAUNCH+=(\"$@\")"));
    assert!(!claude_section.contains("claude mcp add"));
}

#[test]
fn entrypoint_references_runtime_hook_paths() {
    assert!(ENTRYPOINT_SH.contains("/jackin/runtime/hooks/setup-once.sh"));
    assert!(ENTRYPOINT_SH.contains("/jackin/runtime/hooks/source.sh"));
    assert!(ENTRYPOINT_SH.contains("/jackin/runtime/hooks/preflight.sh"));
}

#[test]
fn entrypoint_sources_source_hook_so_exports_persist() {
    assert!(ENTRYPOINT_SH.contains(". /jackin/runtime/hooks/source.sh"));
}

#[test]
fn entrypoint_runs_setup_once_with_writable_marker() {
    assert!(ENTRYPOINT_SH.contains(
        "setup_once_marker=\"${JACKIN_SESSION_STATE_DIR:-/jackin/state}/hooks/setup-once.done\""
    ));
    assert!(!ENTRYPOINT_SH.contains("setup_once_marker=\"/jackin/state/hooks/setup-once.done\""));
    assert!(ENTRYPOINT_SH.contains("touch \"$setup_once_marker\""));
}

#[test]
fn entrypoint_setup_once_marker_is_private_per_session() {
    let fixture = tempdir().expect("marker fixture");
    let assignment = ENTRYPOINT_SH
        .lines()
        .find(|line| line.contains("setup_once_marker=\"${JACKIN_SESSION_STATE_DIR"))
        .expect("production setup-once marker assignment")
        .trim();
    let script = format!(
        "set -eu\n{assignment}\nmkdir -p \"$(dirname \"$setup_once_marker\")\"\ntouch \"$setup_once_marker\"\nprintf '%s' \"$setup_once_marker\"\n"
    );
    let mut markers = Vec::new();
    for session in ["41", "42"] {
        let state = fixture.path().join(format!("session-{session}/state"));
        #[expect(
            clippy::disallowed_methods,
            reason = "this fixture executes the generated shell to verify its marker path"
        )]
        let output = Command::new("bash")
            .arg("-c")
            .arg(&script)
            .env("JACKIN_SESSION_STATE_DIR", &state)
            .output()
            .expect("run setup-once marker shell");
        assert!(output.status.success(), "marker shell failed: {output:?}");
        let marker = String::from_utf8(output.stdout).expect("marker path is UTF-8");
        assert_eq!(
            marker,
            state.join("hooks/setup-once.done").display().to_string()
        );
        assert!(state.join("hooks/setup-once.done").is_file());
        markers.push(marker);
    }
    assert_ne!(markers[0], markers[1], "session markers must not collide");
}

#[test]
fn entrypoint_exports_hook_state_dir_before_hooks_run() {
    // Landlock-confined sessions cannot write capsule-wide /jackin/state, so
    // hooks must write under JACKIN_HOOK_STATE_DIR. The export must precede
    // ALL hook execution (setup-once, source, preflight) so hooks inherit it.
    assert!(ENTRYPOINT_SH.contains(
        "export JACKIN_HOOK_STATE_DIR=\"${JACKIN_SESSION_STATE_DIR:-/jackin/state}/hook-state\""
    ));
    assert!(ENTRYPOINT_SH.contains("mkdir -p \"$JACKIN_HOOK_STATE_DIR\""));
    let export_pos = ENTRYPOINT_SH.find("export JACKIN_HOOK_STATE_DIR=").unwrap();
    for hook in [
        "/jackin/runtime/hooks/setup-once.sh",
        "/jackin/runtime/hooks/source.sh",
        "/jackin/runtime/hooks/preflight.sh",
    ] {
        let hook_pos = ENTRYPOINT_SH.find(hook).unwrap();
        assert!(
            export_pos < hook_pos,
            "hook state export must precede {hook}"
        );
    }
}

#[test]
fn entrypoint_delegates_deterministic_setup_to_jackin_capsule() {
    assert!(ENTRYPOINT_SH.contains("/jackin/runtime/jackin-capsule runtime-setup"));
    assert!(!ENTRYPOINT_SH.contains("git config --global user.name"));
    assert!(!ENTRYPOINT_SH.contains("gh auth setup-git"));
    assert!(!ENTRYPOINT_SH.contains("prepare-commit-msg"));
}

#[test]
fn entrypoint_marker_touched_only_after_setup_once_succeeds() {
    // Reordering would write the marker on hook failure and break first-launch retries.
    let run_pos = ENTRYPOINT_SH.find("run_hook setup-once").unwrap();
    let touch_pos = ENTRYPOINT_SH.find("touch \"$setup_once_marker\"").unwrap();
    assert!(run_pos < touch_pos);
}

#[test]
fn entrypoint_run_hook_helper_captures_rc_before_failure() {
    // `$?` after `if ! cmd; then` is 0 — capture before the test.
    // Pin the pattern so a regression to `if ! "$path"` (which
    // silently makes failure exit 0) is caught.
    let helper = extract_block(ENTRYPOINT_SH, "run_hook() {", "\n}\n");
    assert!(helper.contains("local rc=0"));
    assert!(helper.contains("( cd \"$hook_cwd\" && \"$path\" ) || rc=$?"));
    assert!(helper.contains("\"$path\" || rc=$?"));
    assert!(helper.contains("if [ \"$rc\" -ne 0 ]"));
    assert!(helper.contains("exit \"$rc\""));
}

#[test]
fn entrypoint_runs_preflight_from_agent_home() {
    let preflight = extract_block(
        ENTRYPOINT_SH,
        "if [ -x /jackin/runtime/hooks/preflight.sh ]; then",
        "\nfi\n",
    );
    assert!(
        preflight.contains("run_hook preflight /jackin/runtime/hooks/preflight.sh \"\" \"$HOME\"")
    );
}

#[test]
fn entrypoint_source_hook_block_clears_trap_and_restores_pwd_and_xtrace() {
    // The source block must:
    //   - save PWD before sourcing
    //   - suspend xtrace via `case $- in *x*)` to avoid leaking
    //     expanded secrets when the shell was started with tracing
    //   - capture rc BEFORE testing (same `$?`-after-`!cmd` trap as run_hook)
    //   - restore xtrace
    //   - clear the ERR trap before the cd so a vanished pwd
    //     doesn't fire a hook-installed trap
    let block = extract_block(
        ENTRYPOINT_SH,
        "if [ -x /jackin/runtime/hooks/source.sh ]; then",
        "\nfi\n",
    );
    assert!(block.contains("source_pwd=\"$PWD\""));
    assert!(block.contains("case $- in *x*)"));
    assert!(block.contains(". /jackin/runtime/hooks/source.sh || rc=$?"));
    assert!(block.contains("trap - ERR"));
    let xtrace_suspend_pos = block.find("case $- in *x*)").unwrap();
    let source_pos = block.find(". /jackin/runtime/hooks/source.sh").unwrap();
    assert!(
        xtrace_suspend_pos < source_pos,
        "xtrace suspend must precede the dot-source"
    );
    let trap_pos = block.find("trap - ERR").unwrap();
    let cd_pos = block.find("cd \"$source_pwd\"").unwrap();
    assert!(
        trap_pos < cd_pos,
        "trap - ERR must precede the cd back to source_pwd"
    );
}

#[test]
fn renders_derived_dockerfile_with_only_source_hook() {
    // Mixed-presence: only `source` set. Header block + exactly
    // one COPY line; absent hook filenames must not appear.
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        Some(&HooksConfig {
            setup_once: None,
            source: Some("hooks/source.sh".to_owned()),
            preflight: None,
        }),
        &[Agent::Claude],
        None,
        &BTreeMap::new(),
        None,
    );

    assert!(dockerfile.contains(
        "RUN install -d /jackin/runtime/hooks \\\n    && install -d -o agent -g 0 /jackin/state /jackin/state/hooks"
    ));
    assert_eq!(
        dockerfile
            .matches("\nRUN install -d /jackin/runtime/hooks")
            .count(),
        1
    );
    assert!(!dockerfile.contains("chown -R agent:agent /jackin/state"));
    assert!(dockerfile.contains(
        "COPY --link --chown=agent:0 --chmod=0755 hooks/source.sh /jackin/runtime/hooks/source.sh"
    ));
    assert!(dockerfile.contains(">> /home/agent/.zshenv"));
    assert!(ZSHENV_SOURCE_SHIM.contains("source /jackin/runtime/hooks/source.sh"));
    assert!(!dockerfile.contains("setup-once.sh"));
    assert!(!dockerfile.contains("preflight.sh"));
    assert_eq!(
        dockerfile
            .matches("COPY --link --chown=agent:0 --chmod=0755 hooks/")
            .count(),
        1
    );
}

#[test]
fn source_hook_zshenv_shim_is_not_rendered_for_non_source_hooks() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        Some(&HooksConfig {
            setup_once: Some("hooks/setup-once.sh".to_owned()),
            source: None,
            preflight: Some("hooks/preflight.sh".to_owned()),
        }),
        &[Agent::Claude],
        None,
        &BTreeMap::new(),
        None,
    );

    assert!(dockerfile.contains("/jackin/runtime/hooks/setup-once.sh"));
    assert!(dockerfile.contains("/jackin/runtime/hooks/preflight.sh"));
    assert!(!dockerfile.contains(">> /home/agent/.zshenv"));
    assert!(!dockerfile.contains("__JACKIN_ZSHENV_SOURCE_LOADED"));
}

#[test]
fn build_context_dockerignore_allowlists_only_declared_hooks() {
    // ensure_runtime_assets_are_included must allowlist exactly the
    // hook source paths in the manifest. A regression that dropped
    // the per-hook loop would silently filter scripts out of the
    // build context and fail at docker build time only.
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("hooks")).unwrap();
    std::fs::write(repo.path().join("hooks/source.sh"), "#!/bin/bash\n").unwrap();
    std::fs::write(
        repo.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo.path().join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["claude", "kimi"]

[claude]
plugins = []

[kimi]

[hooks]
source = "hooks/source.sh"
"#,
    )
    .unwrap();
    std::fs::create_dir_all(repo.path().join(".git/objects")).unwrap();
    std::fs::write(
        repo.path().join(".git/objects/large"),
        "not part of build\n",
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let build = create_derived_build_context(repo.path(), &validated, None, None).unwrap();
    let dockerignore = std::fs::read_to_string(build.context_dir.join(".dockerignore")).unwrap();

    assert!(dockerignore.contains("!hooks/source.sh"));
    assert!(!dockerignore.contains("!hooks/setup-once.sh"));
    assert!(!dockerignore.contains("!hooks/preflight.sh"));
}

#[test]
fn creates_temp_context_with_repo_copy_and_runtime_assets() {
    let repo = tempdir().unwrap();
    std::fs::write(
        repo.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo.path().join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["claude", "kimi"]

[claude]
plugins = []

[kimi]
"#,
    )
    .unwrap();
    std::fs::create_dir_all(repo.path().join(".git/objects")).unwrap();
    std::fs::write(
        repo.path().join(".git/objects/large"),
        "not part of build\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo.path().join(".jackin-runtime/agent-binaries")).unwrap();
    std::fs::write(
        repo.path().join(".jackin-runtime/agent-binaries/stale"),
        "stale generated payload\n",
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let build = create_derived_build_context(repo.path(), &validated, None, None).unwrap();

    assert!(build.context_dir.join("Dockerfile").is_file());
    assert!(!build.context_dir.join(".git").exists());
    assert!(
        !build
            .context_dir
            .join(".jackin-runtime/agent-binaries/stale")
            .exists()
    );
    assert!(
        build
            .context_dir
            .join(".jackin-runtime/entrypoint.sh")
            .is_file()
    );
    assert!(build.dockerfile_path.is_file());
}
