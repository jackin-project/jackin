//! Tests for `derived_image`.
use super::*;
use jackin_core::Agent;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use tempfile::tempdir;

fn default_agent_binary_path(agent: Agent) -> String {
    format!(".jackin-runtime/agent-binaries/{}", agent.slug())
}

fn extract_agent_install_block(dockerfile: &str, agent: Agent) -> &str {
    let source = default_agent_binary_path(agent);
    let copy = format!("COPY --chown=agent:agent {source}");
    let copy_pos = dockerfile
        .find(&copy)
        .unwrap_or_else(|| panic!("missing COPY line for {}", agent.slug()));
    let start = dockerfile[..copy_pos]
        .rfind("USER agent\n")
        .unwrap_or_else(|| panic!("missing USER agent before {}", agent.slug()));
    let rest = &dockerfile[start..];
    let candidates = [
        rest[1..]
            .find("\nUSER agent\nARG JACKIN_CACHE_BUST=0\nRUN mkdir -p")
            .map(|pos| pos + 1),
        rest.find("\n# Install Claude plugins"),
        rest.find("\nUSER root\nRUN mkdir -p /jackin/runtime/hooks"),
        rest.find("\nUSER root\nRUN mkdir -p /jackin/default-home"),
    ];
    let end = candidates
        .into_iter()
        .flatten()
        .min()
        .map_or(rest.len(), |pos| pos + 1);
    &rest[..end]
}

#[test]
fn renders_derived_dockerfile_with_workspace_and_entrypoint() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude],
        None,
        None,
        &[],
        &[],
    );

    assert_eq!(
        extract_agent_install_block(&dockerfile, Agent::Claude),
        Agent::Claude.install_block(&default_agent_binary_path(Agent::Claude))
    );
    assert!(!dockerfile.contains("WORKDIR"));
    assert!(
        dockerfile.contains("COPY .jackin-runtime/entrypoint.sh /jackin/runtime/entrypoint.sh")
    );
    assert!(!dockerfile.contains("ENV JACKIN_SUPPORTED_AGENTS="));
    assert!(dockerfile.contains("ENTRYPOINT [\"/jackin/runtime/jackin-capsule\"]"));
}

#[test]
fn renders_derived_dockerfile_installs_claude_as_agent_user() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude],
        None,
        None,
        &[],
        &[],
    );

    assert!(dockerfile.contains("USER agent\n"));
    assert!(dockerfile.contains("ARG JACKIN_CACHE_BUST=0"));
    assert_eq!(
        extract_agent_install_block(&dockerfile, Agent::Claude),
        Agent::Claude.install_block(&default_agent_binary_path(Agent::Claude))
    );
    assert!(
        dockerfile.contains("COPY .jackin-runtime/entrypoint.sh /jackin/runtime/entrypoint.sh")
    );
    assert!(!dockerfile.contains("ENV JACKIN_SUPPORTED_AGENTS="));
}

#[test]
fn renders_derived_dockerfile_rewrites_agent_uid_and_gid() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude],
        None,
        None,
        &[],
        &[],
    );

    assert!(dockerfile.contains("ARG JACKIN_HOST_UID=1000"));
    assert!(dockerfile.contains("ARG JACKIN_HOST_GID=1000"));
    assert!(dockerfile.contains("groupmod -o -g \"$JACKIN_HOST_GID\" agent"));
    assert!(dockerfile.contains("usermod -g \"$JACKIN_HOST_GID\" agent"));
    assert!(dockerfile.contains("usermod -o -u \"$JACKIN_HOST_UID\" agent"));
}

#[test]
fn renders_derived_dockerfile_with_runtime_hooks() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        Some(&HooksConfig {
            setup_once: Some("hooks/setup-once.sh".to_owned()),
            source: Some("hooks/source.sh".to_owned()),
            preflight: Some("hooks/preflight.sh".to_owned()),
        }),
        &[Agent::Claude],
        None,
        None,
        &[],
        &[],
    );

    assert!(dockerfile.contains(
        "COPY --chown=agent:agent hooks/setup-once.sh /jackin/runtime/hooks/setup-once.sh"
    ));
    assert!(dockerfile.contains("RUN mkdir -p /jackin/runtime/hooks /jackin/state/hooks"));
    assert!(
        dockerfile
            .contains("COPY --chown=agent:agent hooks/source.sh /jackin/runtime/hooks/source.sh")
    );
    assert!(dockerfile.contains(
        "COPY --chown=agent:agent hooks/preflight.sh /jackin/runtime/hooks/preflight.sh"
    ));
    // Structural shape: the four load-bearing fragments must appear
    // in order — guard test, rc capture, source call, success-only
    // export, file append. A regression that drops the guard, the rc
    // check, or the `fi` terminator breaks this ordering.
    let copy_pos = dockerfile
        .find("COPY --chown=agent:agent hooks/source.sh")
        .unwrap();
    let guard_pos = dockerfile
        .find("if [ -z \"${__JACKIN_ZSHENV_SOURCE_LOADED:-}\"")
        .unwrap();
    let source_pos = dockerfile
        .find("source /jackin/runtime/hooks/source.sh")
        .unwrap();
    let close_fn_pos = dockerfile.find("} || __jackin_rc=$?").unwrap();
    let export_pos = dockerfile
        .find("export __JACKIN_ZSHENV_SOURCE_LOADED=1")
        .unwrap();
    let append_pos = dockerfile.find(">> /home/agent/.zshenv").unwrap();
    assert!(copy_pos < guard_pos);
    assert!(guard_pos < source_pos);
    assert!(source_pos < close_fn_pos);
    assert!(close_fn_pos < export_pos);
    assert!(export_pos < append_pos);
    assert!(dockerfile.contains("trap - ERR"));
    // Role hooks that `set -euo pipefail` must not leak nounset /
    // errexit / pipefail into the zsh that loads `.zshrc` next —
    // the source call runs in an anonymous fn with localized
    // options + traps.
    assert!(dockerfile.contains("setopt local_options local_traps"));
    // Single emission — derived-from-derived rebuilds must not stack
    // duplicate shim blocks in /home/agent/.zshenv.
    assert_eq!(dockerfile.matches(">> /home/agent/.zshenv").count(), 1);
}

#[test]
fn renders_derived_dockerfile_without_runtime_hooks() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude],
        None,
        None,
        &[],
        &[],
    );

    assert!(!dockerfile.contains("setup-once.sh"));
    assert!(!dockerfile.contains("source.sh"));
    assert!(!dockerfile.contains("preflight.sh"));
    assert!(!dockerfile.contains("/jackin/runtime/hooks"));
    assert!(!dockerfile.contains("/jackin/state/hooks"));
    assert!(!dockerfile.contains("/home/agent/.zshenv"));
}

#[test]
fn renders_dockerfile_with_codex_install_when_supported() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Amp, Agent::Claude, Agent::Codex],
        None,
        None,
        &[],
        &[],
    );

    assert_eq!(
        extract_agent_install_block(&dockerfile, Agent::Claude),
        Agent::Claude.install_block(&default_agent_binary_path(Agent::Claude))
    );
    assert_eq!(
        extract_agent_install_block(&dockerfile, Agent::Codex),
        Agent::Codex.install_block(&default_agent_binary_path(Agent::Codex))
    );
    assert_eq!(
        extract_agent_install_block(&dockerfile, Agent::Amp),
        Agent::Amp.install_block(&default_agent_binary_path(Agent::Amp))
    );
    // Stable ordering for deterministic Dockerfile output.
    let claude_pos = dockerfile
        .find(&default_agent_binary_path(Agent::Claude))
        .unwrap();
    let codex_pos = dockerfile
        .find(&default_agent_binary_path(Agent::Codex))
        .unwrap();
    let amp_pos = dockerfile
        .find(&default_agent_binary_path(Agent::Amp))
        .unwrap();
    assert!(claude_pos < codex_pos);
    assert!(codex_pos < amp_pos);
}

#[test]
fn renders_amp_install_as_agent_user() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Amp],
        None,
        None,
        &[],
        &[],
    );

    assert_eq!(
        extract_agent_install_block(&dockerfile, Agent::Amp),
        Agent::Amp.install_block(&default_agent_binary_path(Agent::Amp))
    );
}

#[test]
fn renders_script_fallback_when_agent_binary_prefetch_failed() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Kimi],
        None,
        None,
        &[],
        &[Agent::Kimi],
    );

    assert!(dockerfile.contains("curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash"));
    assert!(dockerfile.contains("kimi --version"));
    assert!(!dockerfile.contains("COPY --chown=agent:agent .jackin-runtime/agent-binaries/kimi"));
}

#[test]
fn renders_codex_install_as_agent_without_extracting_directly_to_bin() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Codex],
        None,
        None,
        &[],
        &[],
    );

    assert_eq!(
        extract_agent_install_block(&dockerfile, Agent::Codex),
        Agent::Codex.install_block(&default_agent_binary_path(Agent::Codex))
    );
}

#[test]
fn renders_codex_only_dockerfile_final_user_is_agent() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Codex],
        None,
        None,
        &[],
        &[],
    );
    let last_user = dockerfile
        .lines()
        .rfind(|l| l.starts_with("USER "))
        .unwrap();
    assert_eq!(last_user, "USER agent");
}

#[test]
fn renders_codex_only_dockerfile_without_claude_install() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Codex],
        None,
        None,
        &[],
        &[],
    );

    assert_eq!(
        extract_agent_install_block(&dockerfile, Agent::Codex),
        Agent::Codex.install_block(&default_agent_binary_path(Agent::Codex))
    );
}

#[test]
fn renders_dockerfile_targets_agent_user_not_claude() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude],
        None,
        None,
        &[],
        &[],
    );

    assert!(dockerfile.contains("/home/agent"));
    assert!(dockerfile.contains("groupmod -o -g \"$JACKIN_HOST_GID\" agent"));
    assert!(dockerfile.contains("mkdir -p /jackin/run /jackin/state"));
    assert!(dockerfile.contains("chown agent:agent /jackin/run /jackin/state"));
    assert!(dockerfile.contains("ENTRYPOINT [\"/jackin/runtime/jackin-capsule\"]"));
}

#[test]
fn renders_dockerfile_does_not_set_jackin_agent_env() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude, Agent::Codex],
        None,
        None,
        &[],
        &[],
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
        &[Agent::Claude, Agent::Codex, Agent::Amp, Agent::Opencode],
        None,
        None,
        &[],
        &[],
    );

    assert!(dockerfile.contains("/jackin/default-home/.claude"));
    assert!(dockerfile.contains("/jackin/default-home/.codex"));
    assert!(dockerfile.contains("/jackin/default-home/.local/share/amp"));
    assert!(dockerfile.contains("/jackin/default-home/.local/share/opencode"));
    assert!(dockerfile.contains("cp -a /home/agent/.claude/. /jackin/default-home/.claude/"));
}

#[test]
fn renders_claude_plugin_installs_after_claude_cli() {
    let config = jackin_core::manifest::ClaudeConfig {
        model: None,
        marketplaces: vec![jackin_core::manifest::ClaudeMarketplaceConfig {
            source: "obra/superpowers-marketplace".to_owned(),
            sparse: vec!["plugins".to_owned(), ".claude-plugin".to_owned()],
        }],
        plugins: vec![
            "superpowers@superpowers-marketplace".to_owned(),
            "quote'plugin@market".to_owned(),
        ],
    };
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude],
        Some(&config),
        None,
        &[],
        &[],
    );

    let block_pos = dockerfile
        .find(&Agent::Claude.install_block(&default_agent_binary_path(Agent::Claude)))
        .unwrap();
    let official_pos = dockerfile
        .find("RUN claude plugin marketplace add anthropics/claude-plugins-official || true")
        .unwrap();
    let custom_pos = dockerfile
            .find("RUN claude plugin marketplace add 'obra/superpowers-marketplace' --sparse 'plugins' '.claude-plugin'")
            .unwrap();
    let plugin_pos = dockerfile
        .find("RUN claude plugin install 'superpowers@superpowers-marketplace'")
        .unwrap();

    assert!(block_pos < official_pos);
    assert!(official_pos < custom_pos);
    assert!(custom_pos < plugin_pos);
    assert!(dockerfile.contains("RUN claude plugin install 'quote'\"'\"'plugin@market'"));
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
    assert!(ENTRYPOINT_SH.contains("/jackin/state/hooks/setup-once.done"));
    assert!(ENTRYPOINT_SH.contains("touch \"$setup_once_marker\""));
}

#[test]
fn entrypoint_delegates_deterministic_setup_to_jackin_capsule() {
    assert!(ENTRYPOINT_SH.contains("/jackin/runtime/jackin-capsule runtime-setup"));
    assert!(!ENTRYPOINT_SH.contains("git config --global user.name"));
    assert!(!ENTRYPOINT_SH.contains("gh auth setup-git"));
    assert!(!ENTRYPOINT_SH.contains("prepare-commit-msg"));
}

fn extract_block<'a>(haystack: &'a str, start: &str, end: &str) -> &'a str {
    haystack
        .split_once(start)
        .unwrap_or_else(|| panic!("missing block start: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing block end: {end}"))
        .0
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
    assert!(helper.contains("\"$path\" || rc=$?"));
    assert!(helper.contains("if [ \"$rc\" -ne 0 ]"));
    assert!(helper.contains("exit \"$rc\""));
}

#[test]
fn entrypoint_source_hook_block_clears_trap_and_restores_pwd_and_xtrace() {
    // The source block must:
    //   - save PWD before sourcing
    //   - suspend xtrace via `case $- in *x*)` to avoid leaking
    //     expanded secrets under JACKIN_DEBUG=1
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
        None,
        &[],
        &[],
    );

    assert!(dockerfile.contains("RUN mkdir -p /jackin/runtime/hooks /jackin/state/hooks"));
    assert!(
        dockerfile
            .contains("COPY --chown=agent:agent hooks/source.sh /jackin/runtime/hooks/source.sh")
    );
    assert!(dockerfile.contains(">> /home/agent/.zshenv"));
    assert!(dockerfile.contains("source /jackin/runtime/hooks/source.sh"));
    assert!(!dockerfile.contains("setup-once.sh"));
    assert!(!dockerfile.contains("preflight.sh"));
    assert_eq!(
        dockerfile
            .matches("COPY --chown=agent:agent hooks/")
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
        None,
        &[],
        &[],
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
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
source = "hooks/source.sh"
"#,
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let build =
        create_derived_build_context(repo.path(), &validated, None, None, &[], &[]).unwrap();
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
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let build =
        create_derived_build_context(repo.path(), &validated, None, None, &[], &[]).unwrap();

    assert!(build.context_dir.join("Dockerfile").is_file());
    assert!(
        build
            .context_dir
            .join(".jackin-runtime/entrypoint.sh")
            .is_file()
    );
    assert!(build.dockerfile_path.is_file());
}

#[test]
fn preserves_runtime_assets_when_repo_dockerignore_excludes_hidden_paths() {
    let repo = tempdir().unwrap();
    std::fs::write(
        repo.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo.path().join(".dockerignore"),
        r".*
.jackin-runtime
",
    )
    .unwrap();
    std::fs::write(
        repo.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let build =
        create_derived_build_context(repo.path(), &validated, None, None, &[], &[]).unwrap();
    let dockerignore = std::fs::read_to_string(build.context_dir.join(".dockerignore")).unwrap();

    assert!(dockerignore.contains("!.jackin-runtime/"));
    assert!(dockerignore.contains("!.jackin-runtime/entrypoint.sh"));
    assert!(dockerignore.contains("!.jackin-runtime/DerivedDockerfile"));
}

#[test]
fn uses_base_image_override_instead_of_workspace_dockerfile() {
    let repo = tempdir().unwrap();
    std::fs::write(
        repo.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let build = create_derived_build_context(
        repo.path(),
        &validated,
        Some("docker.io/myorg/my-role:latest"),
        None,
        &[],
        &[],
    )
    .unwrap();

    let contents = std::fs::read_to_string(&build.dockerfile_path).unwrap();
    assert!(contents.starts_with("FROM docker.io/myorg/my-role:latest\n"));
    assert!(!contents.contains("projectjackin/construct:"));
}

#[test]
fn jackin_construct_image_override_no_alias() {
    let input = "FROM projectjackin/construct:0.1-trixie\nUSER agent\n";
    let result = apply_construct_image_override(input, "jackin-local/construct:trixie");
    assert!(
        result.starts_with("FROM jackin-local/construct:trixie\n"),
        "override without alias must not add trailing space; got:\n{result}"
    );
}

#[test]
fn jackin_construct_image_override_preserves_as_alias() {
    let input = "FROM projectjackin/construct:0.1-trixie AS runtime\nUSER agent\n";
    let result = apply_construct_image_override(input, "jackin-local/construct:trixie");
    assert!(
        result.starts_with("FROM jackin-local/construct:trixie AS runtime\n"),
        "override must replace the image but preserve the AS alias; got:\n{result}"
    );
}

#[test]
fn jackin_construct_image_override_handles_digest_pinned_from() {
    let input = "FROM projectjackin/construct:0.1-trixie@sha256:0b076bfbc53d36794fe54b1a9cab670f85f831af86d78426b1a88a8ac192d445 AS runtime\nUSER agent\n";
    let result = apply_construct_image_override(input, "jackin-local/construct:trixie");
    assert!(
        result.starts_with("FROM jackin-local/construct:trixie AS runtime\n"),
        "override must replace tag+digest and preserve AS alias; got:\n{result}"
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlinks_in_repo_build_context() {
    let repo = tempdir().unwrap();
    std::fs::write(
        repo.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(repo.path().join("shared.txt"), "hello\n").unwrap();
    symlink(
        repo.path().join("shared.txt"),
        repo.path().join("linked.txt"),
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let error = create_derived_build_context(repo.path(), &validated, None, None, &[], &[])
        .expect_err("symlinks should be rejected");

    assert!(error.to_string().contains("symlink"));
    assert!(error.to_string().contains("linked.txt"));
}

#[test]
fn image_ref_validator_accepts_canonical_forms() {
    assert!(looks_like_valid_image_ref("ubuntu"));
    assert!(looks_like_valid_image_ref("ubuntu:24.04"));
    assert!(looks_like_valid_image_ref("ghcr.io/owner/img:1.2.3"));
    assert!(looks_like_valid_image_ref(
        "ghcr.io/owner/img:tag@sha256:abc123"
    ));
    assert!(looks_like_valid_image_ref("localhost:5000/foo/bar"));
}

#[test]
fn image_ref_validator_rejects_injection_vectors() {
    // The threats the allowlist guards against — a poisoned env
    // var must not inject extra Dockerfile instructions.
    assert!(!looks_like_valid_image_ref(""));
    assert!(!looks_like_valid_image_ref("foo bar"));
    assert!(!looks_like_valid_image_ref("foo\nFROM evil"));
    assert!(!looks_like_valid_image_ref("foo;rm -rf /"));
    assert!(!looks_like_valid_image_ref("foo$(whoami)"));
    assert!(!looks_like_valid_image_ref("foo`id`"));
    assert!(!looks_like_valid_image_ref("foo|sh"));
    assert!(!looks_like_valid_image_ref(&"x".repeat(257)));
}
