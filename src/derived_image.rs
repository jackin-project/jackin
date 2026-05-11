use crate::manifest::HooksConfig;
use crate::repo::ValidatedRoleRepo;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const ENTRYPOINT_SH: &str = include_str!("../docker/runtime/entrypoint.sh");

#[derive(Debug)]
pub struct DerivedBuildContext {
    pub temp_dir: TempDir,
    pub context_dir: PathBuf,
    pub dockerfile_path: PathBuf,
}

/// Caller must pass a `HooksConfig` whose paths have already passed
/// `validate_role_repo` — paths are interpolated directly into Dockerfile
/// `COPY` instructions with no further sanitization here.
pub fn render_derived_dockerfile(
    base_dockerfile: &str,
    hooks: Option<&HooksConfig>,
    supported: &[crate::agent::Agent],
    claude_config: Option<&crate::manifest::ClaudeConfig>,
) -> String {
    use std::fmt::Write as _;

    let mut hook_section = String::new();
    let source_hook_declared = hooks.is_some_and(|h| h.source.is_some());
    let mut entries = hooks.into_iter().flat_map(HooksConfig::entries).peekable();
    if entries.peek().is_some() {
        // chown only /jackin/state — agent writes the marker here.
        // /jackin/runtime/hooks gets per-file ownership from
        // `COPY --chown=agent:agent` below; the dir itself stays root.
        hook_section.push_str(
            "\
USER root
RUN mkdir -p /jackin/runtime/hooks /jackin/state/hooks \\
    && chown -R agent:agent /jackin/state
USER agent
",
        );
        for entry in entries {
            write!(
                hook_section,
                "\
COPY --chown=agent:agent {src} /jackin/runtime/hooks/{dst}
RUN chmod +x /jackin/runtime/hooks/{dst}
",
                src = entry.path,
                dst = entry.filename,
            )
            .expect("writing to String is infallible");
        }
        if source_hook_declared {
            // `docker exec zsh` inherits the image ENV but none of PID 1's
            // runtime exports, so operator shells miss the source-hook
            // exports the entrypoint applied to the agent. The marker is
            // namespaced and exported only after a successful source so a
            // failed hook does not leave a sticky guard that hides
            // re-source attempts from nested subshells (mirrors the rc
            // capture + `trap - ERR` clear the entrypoint does at
            // docker/runtime/entrypoint.sh:172-181). The outer
            // `grep -q ... ||` keeps the file single-shimmed across
            // derived-from-derived builds via `base_image_override`.
            #[allow(clippy::literal_string_with_formatting_args)] // shell ${...}, not a Rust format arg
            const ZSHENV_SOURCE_SHIM: &str = "\
RUN grep -q '__JACKIN_ZSHENV_SOURCE_LOADED' /home/agent/.zshenv 2>/dev/null \\
    || printf '%s\\n' \\
    'if [ -z \"${__JACKIN_ZSHENV_SOURCE_LOADED:-}\" ] && [ -f /jackin/runtime/hooks/source.sh ]; then' \\
    '  __jackin_rc=0' \\
    '  source /jackin/runtime/hooks/source.sh || __jackin_rc=$?' \\
    '  trap - ERR' \\
    '  if [ \"$__jackin_rc\" -ne 0 ]; then' \\
    '    print -u2 \"[zshenv] jackin source hook returned non-zero (exit $__jackin_rc); environment may be incomplete\"' \\
    '  else' \\
    '    export __JACKIN_ZSHENV_SOURCE_LOADED=1' \\
    '  fi' \\
    '  unset __jackin_rc' \\
    'fi' >> /home/agent/.zshenv
";
            hook_section.push_str(ZSHENV_SOURCE_SHIM);
        }
    }

    // Concatenate per-agent install blocks in a stable order (Claude
    // first when present, Codex second, Amp third). Each block declares
    // its own `ARG JACKIN_CACHE_BUST=0` (see the per-agent blocks returned
    // by `Agent::install_block`), so layer cache keys advance
    // independently when `--build-arg JACKIN_CACHE_BUST=<ts>` is
    // passed. The stable ordering is for deterministic Dockerfile
    // output (helps `docker build` cache reuse and keeps diffs
    // reviewable).
    let mut install_blocks = String::new();
    let mut sorted: Vec<crate::agent::Agent> = supported.to_vec();
    sorted.sort_by_key(|h| match h {
        crate::agent::Agent::Claude => 0,
        crate::agent::Agent::Codex => 1,
        crate::agent::Agent::Amp => 2,
    });
    for h in sorted {
        install_blocks.push_str(h.install_block());
        if h == crate::agent::Agent::Claude {
            install_blocks.push_str(&render_claude_plugin_install_block(claude_config));
        }
    }

    format!(
        "\
{base_dockerfile}
USER root
ARG JACKIN_HOST_UID=1000
ARG JACKIN_HOST_GID=1000
RUN current_gid=\"$(id -g agent)\" \
    && current_uid=\"$(id -u agent)\" \
    && if [ \"$current_gid\" != \"$JACKIN_HOST_GID\" ]; then \
         groupmod -o -g \"$JACKIN_HOST_GID\" agent \
         && usermod -g \"$JACKIN_HOST_GID\" agent; \
       fi \
    && if [ \"$current_uid\" != \"$JACKIN_HOST_UID\" ]; then \
         usermod -o -u \"$JACKIN_HOST_UID\" agent; \
       fi \
    && chown -R agent:agent /home/agent
{install_blocks}{hook_section}USER root
COPY .jackin-runtime/entrypoint.sh /jackin/runtime/entrypoint.sh
RUN chmod +x /jackin/runtime/entrypoint.sh
USER agent
ENTRYPOINT [\"/jackin/runtime/entrypoint.sh\"]
"
    )
}

fn render_claude_plugin_install_block(
    claude_config: Option<&crate::manifest::ClaudeConfig>,
) -> String {
    let Some(config) = claude_config else {
        return String::new();
    };
    if config.marketplaces.is_empty() && config.plugins.is_empty() {
        return String::new();
    }

    let mut block = String::from(
        "\
# Install Claude plugins declared by jackin.role.toml at image-build time.
RUN claude plugin marketplace add anthropics/claude-plugins-official || true
",
    );

    for marketplace in &config.marketplaces {
        block.push_str("RUN claude plugin marketplace add ");
        block.push_str(&shell_quote(&marketplace.source));
        if !marketplace.sparse.is_empty() {
            block.push_str(" --sparse");
            for path in &marketplace.sparse {
                block.push(' ');
                block.push_str(&shell_quote(path));
            }
        }
        block.push('\n');
    }

    for plugin in &config.plugins {
        block.push_str("RUN claude plugin install ");
        block.push_str(&shell_quote(plugin));
        block.push('\n');
    }

    block
}

/// Single-quote `value` for safe inclusion in a `/bin/sh -c` string. Embedded
/// single quotes are escaped via the POSIX `'"'"'` idiom; an empty string
/// becomes `''` so it survives shell word splitting.
pub fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            quoted.push_str("'\"'\"'");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

pub fn create_derived_build_context(
    repo_dir: &Path,
    validated: &ValidatedRoleRepo,
    // When Some, the DerivedDockerfile starts with `FROM <image>` rather than
    // the workspace Dockerfile contents (pre-built image fast path).
    base_image_override: Option<&str>,
) -> anyhow::Result<DerivedBuildContext> {
    let temp_dir = tempfile::tempdir()?;
    let context_dir = temp_dir.path().join("context");
    copy_dir_all(repo_dir, &context_dir)?;

    let runtime_dir = context_dir.join(".jackin-runtime");
    std::fs::create_dir_all(&runtime_dir)?;
    std::fs::write(runtime_dir.join("entrypoint.sh"), ENTRYPOINT_SH)?;

    let hooks = validated.manifest.hooks.as_ref();

    let base_dockerfile = base_image_override.map_or_else(
        || validated.dockerfile.dockerfile_contents.clone(),
        |image| format!("FROM {image}\n"),
    );

    let supported = validated.manifest.supported_agents();
    let dockerfile_path = context_dir.join(".jackin-runtime/DerivedDockerfile");
    std::fs::write(
        &dockerfile_path,
        render_derived_dockerfile(
            &base_dockerfile,
            hooks,
            &supported,
            validated.manifest.claude.as_ref(),
        ),
    )?;
    ensure_runtime_assets_are_included(&context_dir, hooks)?;

    Ok(DerivedBuildContext {
        temp_dir,
        context_dir,
        dockerfile_path,
    })
}

fn ensure_runtime_assets_are_included(
    context_dir: &Path,
    hooks: Option<&HooksConfig>,
) -> anyhow::Result<()> {
    let dockerignore_path = context_dir.join(".dockerignore");
    let mut dockerignore = if dockerignore_path.exists() {
        std::fs::read_to_string(&dockerignore_path)?
    } else {
        String::new()
    };

    let mut rules = vec![
        "!.jackin-runtime/".to_string(),
        "!.jackin-runtime/entrypoint.sh".to_string(),
        "!.jackin-runtime/DerivedDockerfile".to_string(),
    ];
    for entry in hooks.into_iter().flat_map(HooksConfig::entries) {
        rules.push(format!("!{}", entry.path));
    }

    for rule in &rules {
        if !dockerignore.lines().any(|line| line == rule) {
            if !dockerignore.is_empty() && !dockerignore.ends_with('\n') {
                dockerignore.push('\n');
            }
            dockerignore.push_str(rule);
            dockerignore.push('\n');
        }
    }

    std::fs::write(dockerignore_path, dockerignore)?;
    Ok(())
}

fn copy_dir_all(from: &Path, to: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let destination = to.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), destination)?;
        } else if file_type.is_symlink() {
            anyhow::bail!(
                "invalid role repo: derived build context does not support symlinks: {}",
                entry.path().display()
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    #[test]
    fn renders_derived_dockerfile_with_workspace_and_entrypoint() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Claude],
            None,
        );

        assert!(dockerfile.contains("RUN curl -fsSL https://claude.ai/install.sh | bash"));
        assert!(!dockerfile.contains("WORKDIR"));
        assert!(
            dockerfile.contains("COPY .jackin-runtime/entrypoint.sh /jackin/runtime/entrypoint.sh")
        );
        assert!(dockerfile.contains("ENTRYPOINT [\"/jackin/runtime/entrypoint.sh\"]"));
    }

    #[test]
    fn renders_derived_dockerfile_installs_claude_as_agent_user() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Claude],
            None,
        );

        assert!(dockerfile.contains("USER agent\n"));
        assert!(dockerfile.contains("ARG JACKIN_CACHE_BUST=0"));
        assert!(dockerfile.contains("RUN curl -fsSL https://claude.ai/install.sh | bash"));
        assert!(dockerfile.contains("RUN claude --version"));
        assert!(
            dockerfile.contains("COPY .jackin-runtime/entrypoint.sh /jackin/runtime/entrypoint.sh")
        );
    }

    #[test]
    fn renders_derived_dockerfile_rewrites_agent_uid_and_gid() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Claude],
            None,
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
            "FROM projectjackin/construct:trixie\n",
            Some(&HooksConfig {
                setup_once: Some("hooks/setup-once.sh".to_string()),
                source: Some("hooks/source.sh".to_string()),
                preflight: Some("hooks/preflight.sh".to_string()),
            }),
            &[Agent::Claude],
            None,
        );

        assert!(dockerfile.contains(
            "COPY --chown=agent:agent hooks/setup-once.sh /jackin/runtime/hooks/setup-once.sh"
        ));
        assert!(dockerfile.contains("RUN mkdir -p /jackin/runtime/hooks /jackin/state/hooks"));
        assert!(
            dockerfile.contains(
                "COPY --chown=agent:agent hooks/source.sh /jackin/runtime/hooks/source.sh"
            )
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
            .find("source /jackin/runtime/hooks/source.sh || __jackin_rc=$?")
            .unwrap();
        let export_pos = dockerfile
            .find("export __JACKIN_ZSHENV_SOURCE_LOADED=1")
            .unwrap();
        let append_pos = dockerfile.find(">> /home/agent/.zshenv").unwrap();
        assert!(copy_pos < guard_pos);
        assert!(guard_pos < source_pos);
        assert!(source_pos < export_pos);
        assert!(export_pos < append_pos);
        assert!(dockerfile.contains("trap - ERR"));
        // Single emission — derived-from-derived rebuilds must not stack
        // duplicate shim blocks in /home/agent/.zshenv.
        assert_eq!(dockerfile.matches(">> /home/agent/.zshenv").count(), 1);
    }

    #[test]
    fn renders_derived_dockerfile_without_runtime_hooks() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Claude],
            None,
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
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Amp, Agent::Claude, Agent::Codex],
            None,
        );

        assert!(dockerfile.contains("https://claude.ai/install.sh"));
        assert!(dockerfile.contains("openai/codex/releases"));
        assert!(dockerfile.contains("https://ampcode.com/install.sh"));
        // Stable ordering for deterministic Dockerfile output.
        let claude_pos = dockerfile.find("claude.ai/install.sh").unwrap();
        let codex_pos = dockerfile.find("openai/codex/releases").unwrap();
        let amp_pos = dockerfile.find("ampcode.com/install.sh").unwrap();
        assert!(claude_pos < codex_pos);
        assert!(codex_pos < amp_pos);
    }

    #[test]
    fn renders_amp_install_as_agent_user() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Amp],
            None,
        );

        let amp_block_pos = dockerfile.find("ampcode.com/install.sh").unwrap();
        let agent_pos = dockerfile[..amp_block_pos].rfind("USER agent\n").unwrap();
        assert!(agent_pos < amp_block_pos);
        assert!(dockerfile.contains("RUN amp --version"));
        assert!(!dockerfile.contains("https://claude.ai/install.sh"));
        assert!(!dockerfile.contains("openai/codex/releases"));
    }

    #[test]
    fn renders_codex_install_as_root_without_extracting_directly_to_bin() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Codex],
            None,
        );

        let codex_block_pos = dockerfile.find("ASSET=\"codex-${ARCH}\"").unwrap();
        // rfind finds the most recent USER directive before the Codex install
        // block — must be root, not agent.
        let root_pos = dockerfile[..codex_block_pos].rfind("USER root\n").unwrap();
        assert!(root_pos < codex_block_pos);
        assert!(dockerfile.contains("set -euxo pipefail"));
        assert!(dockerfile.contains("tar -xzf - -O \"${ASSET}\" > /tmp/codex.bin"));
        assert!(dockerfile.contains("mv /tmp/codex.bin /usr/local/bin/codex"));
        assert!(!dockerfile.contains("tar -xz -C /usr/local/bin"));
    }

    #[test]
    fn renders_codex_only_dockerfile_final_user_is_agent() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Codex],
            None,
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
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Codex],
            None,
        );

        assert!(!dockerfile.contains("https://claude.ai/install.sh"));
        assert!(dockerfile.contains("openai/codex/releases"));
    }

    #[test]
    fn renders_dockerfile_targets_agent_user_not_claude() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Claude],
            None,
        );

        assert!(dockerfile.contains("/home/agent"));
        assert!(dockerfile.contains("groupmod -o -g \"$JACKIN_HOST_GID\" agent"));
        assert!(dockerfile.contains("ENTRYPOINT [\"/jackin/runtime/entrypoint.sh\"]"));
    }

    #[test]
    fn renders_dockerfile_does_not_set_jackin_agent_env() {
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Claude, Agent::Codex],
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
            codex_section
                .contains("codex --enable goals --dangerously-bypass-approvals-and-sandbox")
        );
        assert!(codex_section.contains("LAUNCH+=(\"$@\")"));
        assert!(!codex_section.contains("config.toml"));
    }

    #[test]
    fn entrypoint_amp_branch_copies_secrets_and_launches_amp() {
        let amp_section = ENTRYPOINT_SH
            .split_once("\n  amp)")
            .unwrap()
            .1
            .split(";;")
            .next()
            .unwrap();
        assert!(amp_section.contains("/home/agent/.local/share/amp"));
        assert!(amp_section.contains("/jackin/amp/secrets.json"));
        assert!(amp_section.contains("LAUNCH=(amp --dangerously-allow-all)"));
    }

    #[test]
    fn renders_claude_plugin_installs_after_claude_cli() {
        let config = crate::manifest::ClaudeConfig {
            model: None,
            marketplaces: vec![crate::manifest::ClaudeMarketplaceConfig {
                source: "obra/superpowers-marketplace".to_string(),
                sparse: vec!["plugins".to_string(), ".claude-plugin".to_string()],
            }],
            plugins: vec![
                "superpowers@superpowers-marketplace".to_string(),
                "quote'plugin@market".to_string(),
            ],
        };
        let dockerfile = render_derived_dockerfile(
            "FROM projectjackin/construct:trixie\n",
            None,
            &[Agent::Claude],
            Some(&config),
        );

        let version_pos = dockerfile.find("RUN claude --version").unwrap();
        let official_pos = dockerfile
            .find("RUN claude plugin marketplace add anthropics/claude-plugins-official || true")
            .unwrap();
        let custom_pos = dockerfile
            .find("RUN claude plugin marketplace add 'obra/superpowers-marketplace' --sparse 'plugins' '.claude-plugin'")
            .unwrap();
        let plugin_pos = dockerfile
            .find("RUN claude plugin install 'superpowers@superpowers-marketplace'")
            .unwrap();

        assert!(version_pos < official_pos);
        assert!(official_pos < custom_pos);
        assert!(custom_pos < plugin_pos);
        assert!(dockerfile.contains("RUN claude plugin install 'quote'\"'\"'plugin@market'"));
    }

    #[test]
    fn entrypoint_registers_security_tool_mcp_servers() {
        let claude_section = ENTRYPOINT_SH
            .split("claude)")
            .nth(1)
            .unwrap()
            .split(";;")
            .next()
            .unwrap();
        assert!(claude_section.contains("LAUNCH+=(\"$@\")"));
        assert!(claude_section.contains("claude mcp add tirith -- tirith mcp-server"));
        assert!(claude_section.contains("claude mcp add shellfirm -- shellfirm mcp"));
    }

    #[test]
    fn entrypoint_mcp_registration_respects_disable_guards() {
        assert!(ENTRYPOINT_SH.contains("JACKIN_DISABLE_TIRITH"));
        assert!(ENTRYPOINT_SH.contains("JACKIN_DISABLE_SHELLFIRM"));
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
            "FROM projectjackin/construct:trixie\n",
            Some(&HooksConfig {
                setup_once: None,
                source: Some("hooks/source.sh".to_string()),
                preflight: None,
            }),
            &[Agent::Claude],
            None,
        );

        assert!(dockerfile.contains("RUN mkdir -p /jackin/runtime/hooks /jackin/state/hooks"));
        assert!(
            dockerfile.contains(
                "COPY --chown=agent:agent hooks/source.sh /jackin/runtime/hooks/source.sh"
            )
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
            "FROM projectjackin/construct:trixie\n",
            Some(&HooksConfig {
                setup_once: Some("hooks/setup-once.sh".to_string()),
                source: None,
                preflight: Some("hooks/preflight.sh".to_string()),
            }),
            &[Agent::Claude],
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo.path().join("jackin.role.toml"),
            r#"version = "v1alpha2"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
source = "hooks/source.sh"
"#,
        )
        .unwrap();

        let validated = crate::repo::validate_role_repo(repo.path()).unwrap();
        let build = create_derived_build_context(repo.path(), &validated, None).unwrap();
        let dockerignore =
            std::fs::read_to_string(build.context_dir.join(".dockerignore")).unwrap();

        assert!(dockerignore.contains("!hooks/source.sh"));
        assert!(!dockerignore.contains("!hooks/setup-once.sh"));
        assert!(!dockerignore.contains("!hooks/preflight.sh"));
    }

    #[test]
    fn creates_temp_context_with_repo_copy_and_runtime_assets() {
        let repo = tempdir().unwrap();
        std::fs::write(
            repo.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo.path().join("jackin.role.toml"),
            r#"version = "v1alpha2"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let validated = crate::repo::validate_role_repo(repo.path()).unwrap();
        let build = create_derived_build_context(repo.path(), &validated, None).unwrap();

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
            "FROM projectjackin/construct:trixie\n",
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
            r#"version = "v1alpha2"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let validated = crate::repo::validate_role_repo(repo.path()).unwrap();
        let build = create_derived_build_context(repo.path(), &validated, None).unwrap();
        let dockerignore =
            std::fs::read_to_string(build.context_dir.join(".dockerignore")).unwrap();

        assert!(dockerignore.contains("!.jackin-runtime/"));
        assert!(dockerignore.contains("!.jackin-runtime/entrypoint.sh"));
        assert!(dockerignore.contains("!.jackin-runtime/DerivedDockerfile"));
    }

    #[test]
    fn uses_base_image_override_instead_of_workspace_dockerfile() {
        let repo = tempdir().unwrap();
        std::fs::write(
            repo.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo.path().join("jackin.role.toml"),
            r#"version = "v1alpha2"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let validated = crate::repo::validate_role_repo(repo.path()).unwrap();
        let build = create_derived_build_context(
            repo.path(),
            &validated,
            Some("docker.io/myorg/my-role:latest"),
        )
        .unwrap();

        let contents = std::fs::read_to_string(&build.dockerfile_path).unwrap();
        assert!(contents.starts_with("FROM docker.io/myorg/my-role:latest\n"));
        assert!(!contents.contains("projectjackin/construct:trixie"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_in_repo_build_context() {
        let repo = tempdir().unwrap();
        std::fs::write(
            repo.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo.path().join("jackin.role.toml"),
            r#"version = "v1alpha2"
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

        let validated = crate::repo::validate_role_repo(repo.path()).unwrap();
        let error = create_derived_build_context(repo.path(), &validated, None)
            .expect_err("symlinks should be rejected");

        assert!(error.to_string().contains("symlink"));
        assert!(error.to_string().contains("linked.txt"));
    }
}
