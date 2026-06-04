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
fn render_hook_section(hooks: Option<&HooksConfig>) -> String {
    use std::fmt::Write as _;

    let source_hook_declared = hooks.is_some_and(|h| h.source.is_some());
    let mut entries = hooks.into_iter().flat_map(HooksConfig::entries).peekable();
    if entries.peek().is_none() {
        return String::new();
    }

    let mut section = String::new();
    // chown only /jackin/state — agent writes the marker here.
    // /jackin/runtime/hooks gets per-file ownership from
    // `COPY --chown=agent:agent` below; the dir itself stays root.
    section.push_str(
        "\
USER root
RUN mkdir -p /jackin/runtime/hooks /jackin/state/hooks \\
    && chown -R agent:agent /jackin/state
USER agent
",
    );
    for entry in entries {
        write!(
            section,
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
        //
        // The `source` runs inside an anonymous zsh function with
        // `setopt local_options local_traps`: role hooks routinely
        // ship `set -euo pipefail` (POSIX-sh idiom), which in zsh maps
        // to `nounset`/`errexit`/`pipefail`. Without the local scope
        // those flags leak into the same zsh that then loads
        // `.zshrc` — `oh-my-zsh/lib/termsupport.zsh` and tirith's
        // `zsh-hook.zsh` both read variables without `:-` defaults and
        // error out under `nounset`, breaking every interactive Shell
        // pane. The anonymous fn keeps option/trap changes scoped to
        // the source call while still letting `export VAR=...` inside
        // `source.sh` leak into the caller's env (which is the whole
        // point of the shim).
        #[allow(clippy::literal_string_with_formatting_args)] // shell ${...}, not a Rust format arg
        const ZSHENV_SOURCE_SHIM: &str = "\
RUN grep -q '__JACKIN_ZSHENV_SOURCE_LOADED' /home/agent/.zshenv 2>/dev/null \\
    || printf '%s\\n' \\
    'if [ -z \"${__JACKIN_ZSHENV_SOURCE_LOADED:-}\" ] && [ -f /jackin/runtime/hooks/source.sh ]; then' \\
    '  __jackin_rc=0' \\
    '  () {' \\
    '    setopt local_options local_traps' \\
    '    source /jackin/runtime/hooks/source.sh' \\
    '  } || __jackin_rc=$?' \\
    '  trap - ERR' \\
    '  if [ \"$__jackin_rc\" -ne 0 ]; then' \\
    '    print -u2 \"[zshenv] jackin source hook returned non-zero (exit $__jackin_rc); environment may be incomplete\"' \\
    '  else' \\
    '    export __JACKIN_ZSHENV_SOURCE_LOADED=1' \\
    '  fi' \\
    '  unset __jackin_rc' \\
    'fi' >> /home/agent/.zshenv
";
        section.push_str(ZSHENV_SOURCE_SHIM);
    }
    section
}

pub fn render_derived_dockerfile(
    base_dockerfile: &str,
    hooks: Option<&HooksConfig>,
    supported: &[crate::agent::Agent],
    claude_config: Option<&crate::manifest::ClaudeConfig>,
    jackin_capsule_bin: Option<&str>,
    agent_binaries: &[(crate::agent::Agent, String)],
) -> String {
    let hook_section = render_hook_section(hooks);

    // Concatenate per-agent install blocks in a stable order (Claude
    // first when present, Codex second, Amp third, Kimi fourth,
    // OpenCode fifth). Each block declares its own `ARG JACKIN_CACHE_BUST=0`
    // (see the per-agent blocks returned by `Agent::install_block`), so layer
    // cache keys advance independently when `--build-arg JACKIN_CACHE_BUST=<ts>`
    // is passed. Stable ordering keeps diffs reviewable.
    let mut install_blocks = String::new();
    let mut sorted: Vec<crate::agent::Agent> = supported.to_vec();
    sorted.sort_by_key(|h| match h {
        crate::agent::Agent::Claude => 0,
        crate::agent::Agent::Codex => 1,
        crate::agent::Agent::Amp => 2,
        crate::agent::Agent::Kimi => 3,
        crate::agent::Agent::Opencode => 4,
    });
    for h in sorted {
        let source = agent_binaries
            .iter()
            .find(|(agent, _)| *agent == h)
            .map_or_else(
                || format!(".jackin-runtime/agent-binaries/{}", h.slug()),
                |(_, path)| path.clone(),
            );
        install_blocks.push_str(&h.install_block(&source));
        if h == crate::agent::Agent::Claude {
            install_blocks.push_str(&render_claude_plugin_install_block(claude_config));
        }
    }

    // jackin-capsule binary (pre-downloaded by host, placed in .jackin-runtime/).
    let jackin_capsule_section = jackin_capsule_bin.map_or_else(String::new, |src| {
        format!(
            "\
COPY {src} /jackin/runtime/jackin-capsule
RUN chmod +x /jackin/runtime/jackin-capsule
"
        )
    });

    // Append an oh-my-zsh title-hook source to /home/agent/.zshrc when
    // the construct image's zshrc did not already do so. The hook emits
    // OSC 0/2 (`user@host:cwd`) and OSC 7 on every prompt — the
    // jackin-capsule multiplexer reads both and renders the pane
    // border title from them (matches zellij convention).
    //
    // Idempotent via the `__JACKIN_AUTO_TITLE_LOADED` marker: new
    // construct images source oh-my-zsh natively and export the
    // marker, so this fallback no-ops once the operator rebuilds
    // construct. Derived-from-derived builds (`base_image_override`)
    // also skip the second append because the first build added the
    // marker line to /home/agent/.zshrc.
    #[allow(clippy::literal_string_with_formatting_args)] // shell ${...}, not a Rust format arg
    #[allow(clippy::items_after_statements)]
    const SHELL_TITLE_HOOK_SECTION: &str = "\
RUN grep -q '__JACKIN_AUTO_TITLE_LOADED' /home/agent/.zshrc 2>/dev/null \\
    || printf '%s\\n' \\
    '' \\
    '# jackin: source oh-my-zsh title hook when the active .zshrc did' \\
    '# not already do so. Brings OSC 0/2 (window title) and OSC 7 (cwd)' \\
    '# emit on every prompt for the multiplexer pane title.' \\
    'if [ -z \"${__JACKIN_AUTO_TITLE_LOADED:-}\" ] && [ -f \"$HOME/.oh-my-zsh/lib/termsupport.zsh\" ]; then' \\
    '    [ -f \"$HOME/.oh-my-zsh/lib/functions.zsh\" ] && source \"$HOME/.oh-my-zsh/lib/functions.zsh\"' \\
    '    source \"$HOME/.oh-my-zsh/lib/termsupport.zsh\"' \\
    '    export __JACKIN_AUTO_TITLE_LOADED=1' \\
    'fi' >> /home/agent/.zshrc
";
    let shell_title_hook_section = SHELL_TITLE_HOOK_SECTION;

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
RUN mkdir -p /jackin/default-home/.claude /jackin/default-home/.codex /jackin/default-home/.local/share/amp /jackin/default-home/.kimi-code /jackin/default-home/.local/share/opencode \
    && ( cp -a /home/agent/.claude/. /jackin/default-home/.claude/ 2>/dev/null || true ) \
    && ( cp -a /home/agent/.codex/. /jackin/default-home/.codex/ 2>/dev/null || true ) \
    && ( cp -a /home/agent/.local/share/amp/. /jackin/default-home/.local/share/amp/ 2>/dev/null || true ) \
    && ( cp -a /home/agent/.kimi-code/. /jackin/default-home/.kimi-code/ 2>/dev/null || true ) \
    && ( cp -a /home/agent/.local/share/opencode/. /jackin/default-home/.local/share/opencode/ 2>/dev/null || true ) \
    && chown -R agent:agent /jackin/default-home
COPY .jackin-runtime/entrypoint.sh /jackin/runtime/entrypoint.sh
RUN chmod +x /jackin/runtime/entrypoint.sh
{shell_title_hook_section}{jackin_capsule_section}RUN mkdir -p /jackin/run /jackin/state && chown agent:agent /jackin/run /jackin/state
USER agent
ENTRYPOINT [\"/jackin/runtime/jackin-capsule\"]
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

/// Validate that `value` looks like a Docker image reference and not
/// arbitrary text. Operator-set `JACKIN_CONSTRUCT_IMAGE` flows through
/// here before being interpolated into a `FROM` line; without this
/// check a newline-containing value (e.g. from a poisoned `.envrc`)
/// would inject arbitrary RUN instructions executed at image-build
/// time. The accepted alphabet is the conservative subset that Docker
/// itself accepts in references plus colons, slashes, `@`, and dots —
/// everything else is rejected.
fn looks_like_valid_image_ref(value: &str) -> bool {
    if value.is_empty() || value.len() > 256 {
        return false;
    }
    value.chars().all(|c| {
        matches!(
            c,
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '-' | '/' | ':' | '@' | '+'
        )
    })
}

/// Replace `FROM projectjackin/construct:<tag>[@<digest>] [AS alias]` lines in
/// `contents` with `FROM <override_image> [AS alias]`. Digest pins are dropped
/// because a local override image has no matching digest.
fn apply_construct_image_override(contents: &str, override_image: &str) -> String {
    let construct_from_prefix = format!("FROM {}:", crate::repo_contract::CONSTRUCT_REGISTRY_IMAGE);
    let from_override = format!("FROM {override_image}");
    let mut result = contents
        .lines()
        .map(|line| {
            if line.starts_with(&construct_from_prefix) {
                let after_prefix = &line[construct_from_prefix.len()..];
                let alias = after_prefix
                    .split_once(' ')
                    .map_or(String::new(), |(_, rest)| format!(" {rest}"));
                format!("{from_override}{alias}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if contents.ends_with('\n') {
        result.push('\n');
    }
    result
}

pub fn create_derived_build_context(
    repo_dir: &Path,
    validated: &ValidatedRoleRepo,
    // When Some, the DerivedDockerfile starts with `FROM <image>` rather than
    // the workspace Dockerfile contents (pre-built image fast path).
    base_image_override: Option<&str>,
    // Path to the pre-downloaded jackin-capsule binary on the host.
    // When Some, the binary is copied into the build context and baked into
    // the derived image at /jackin/runtime/jackin-capsule.
    jackin_capsule_host_path: Option<&str>,
    agent_binary_host_paths: &[(crate::agent::Agent, PathBuf)],
) -> anyhow::Result<DerivedBuildContext> {
    let temp_dir = tempfile::tempdir()?;
    let context_dir = temp_dir.path().join("context");
    copy_dir_all(repo_dir, &context_dir)?;

    let runtime_dir = context_dir.join(".jackin-runtime");
    std::fs::create_dir_all(&runtime_dir)?;
    std::fs::write(runtime_dir.join("entrypoint.sh"), ENTRYPOINT_SH)?;

    // Copy jackin-capsule binary into the build context so the Dockerfile
    // can COPY it into the image without a network fetch at build time.
    let jackin_capsule_ctx_path = if let Some(host_path) = jackin_capsule_host_path {
        let dst = runtime_dir.join("jackin-capsule");
        std::fs::copy(host_path, &dst).map_err(|e| {
            anyhow::anyhow!("failed to copy jackin-capsule binary into build context: {e}")
        })?;
        Some(".jackin-runtime/jackin-capsule".to_string())
    } else {
        None
    };

    let agent_binary_ctx_paths = copy_agent_binaries(&runtime_dir, agent_binary_host_paths)?;

    let hooks = validated.manifest.hooks.as_ref();

    // Validation policy by ingress channel — intentionally asymmetric:
    //
    // - `base_image_override` argument: hard error on invalid input.
    //   The caller is jackin's own runtime code (or a future CLI flag
    //   the operator typed explicitly). A typo / programmer bug is
    //   worth failing the build loudly.
    //
    // - `JACKIN_CONSTRUCT_IMAGE` env var: warn to stderr and fall
    //   back to the role's pinned image. The env var is operator-side
    //   UX (often set in a shell rc / direnv); failing the build for
    //   a stale value would surprise. Both paths share the same
    //   `looks_like_valid_image_ref` allowlist so the bytes that
    //   reach the Dockerfile FROM line are character-set-bounded
    //   regardless of ingress.
    let base_dockerfile = if let Some(image) = base_image_override {
        anyhow::ensure!(
            looks_like_valid_image_ref(image),
            "base_image_override {image:?} is not a valid Docker image reference; refusing to interpolate into Dockerfile FROM line",
        );
        format!("FROM {image}\n")
    } else {
        let override_image = std::env::var("JACKIN_CONSTRUCT_IMAGE").unwrap_or_default();
        let override_trimmed = override_image.trim();
        if override_trimmed.is_empty() {
            validated.dockerfile.dockerfile_contents.clone()
        } else if looks_like_valid_image_ref(override_trimmed) {
            apply_construct_image_override(
                &validated.dockerfile.dockerfile_contents,
                override_trimmed,
            )
        } else {
            crate::tui::emit_compact_line(
                "warning",
                &format!(
                    "[jackin] ignoring invalid JACKIN_CONSTRUCT_IMAGE={override_image:?}; using role's pinned base image"
                ),
            );
            validated.dockerfile.dockerfile_contents.clone()
        }
    };

    let supported = validated.manifest.supported_agents();
    let dockerfile_path = context_dir.join(".jackin-runtime/DerivedDockerfile");
    std::fs::write(
        &dockerfile_path,
        render_derived_dockerfile(
            &base_dockerfile,
            hooks,
            &supported,
            validated.manifest.claude.as_ref(),
            jackin_capsule_ctx_path.as_deref(),
            &agent_binary_ctx_paths,
        ),
    )?;
    ensure_runtime_assets_are_included(&context_dir, hooks)?;

    Ok(DerivedBuildContext {
        temp_dir,
        context_dir,
        dockerfile_path,
    })
}

fn copy_agent_binaries(
    runtime_dir: &Path,
    host_paths: &[(crate::agent::Agent, PathBuf)],
) -> anyhow::Result<Vec<(crate::agent::Agent, String)>> {
    let dst_dir = runtime_dir.join("agent-binaries");
    std::fs::create_dir_all(&dst_dir)?;
    let mut copied = Vec::new();
    for (agent, host_path) in host_paths {
        let dst = dst_dir.join(agent.slug());
        std::fs::copy(host_path, &dst).map_err(|e| {
            anyhow::anyhow!(
                "failed to copy {} binary into build context from {}: {e}",
                agent.slug(),
                host_path.display()
            )
        })?;
        copied.push((
            *agent,
            format!(".jackin-runtime/agent-binaries/{}", agent.slug()),
        ));
    }
    Ok(copied)
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
        "!.jackin-runtime/jackin-capsule".to_string(),
        "!.jackin-runtime/agent-binaries/".to_string(),
        "!.jackin-runtime/agent-binaries/*".to_string(),
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
                setup_once: Some("hooks/setup-once.sh".to_string()),
                source: Some("hooks/source.sh".to_string()),
                preflight: Some("hooks/preflight.sh".to_string()),
            }),
            &[Agent::Claude],
            None,
            None,
            &[],
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
        );

        assert_eq!(
            extract_agent_install_block(&dockerfile, Agent::Amp),
            Agent::Amp.install_block(&default_agent_binary_path(Agent::Amp))
        );
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
            codex_section
                .contains("codex --enable goals --dangerously-bypass-approvals-and-sandbox")
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
        assert!(kimi_section.contains("LAUNCH=(kimi --yolo --auto)"));
        assert!(kimi_section.contains("LAUNCH+=(\"$@\")"));
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
            opencode_section
                .contains("export OPENCODE_CONFIG_CONTENT='{\"permission\":\"allow\"}'")
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
        );

        assert!(dockerfile.contains("/jackin/default-home/.claude"));
        assert!(dockerfile.contains("/jackin/default-home/.codex"));
        assert!(dockerfile.contains("/jackin/default-home/.local/share/amp"));
        assert!(dockerfile.contains("/jackin/default-home/.local/share/opencode"));
        assert!(dockerfile.contains("cp -a /home/agent/.claude/. /jackin/default-home/.claude/"));
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
            "FROM projectjackin/construct:0.1-trixie\n",
            None,
            &[Agent::Claude],
            Some(&config),
            None,
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
                source: Some("hooks/source.sh".to_string()),
                preflight: None,
            }),
            &[Agent::Claude],
            None,
            None,
            &[],
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
            "FROM projectjackin/construct:0.1-trixie\n",
            Some(&HooksConfig {
                setup_once: Some("hooks/setup-once.sh".to_string()),
                source: None,
                preflight: Some("hooks/preflight.sh".to_string()),
            }),
            &[Agent::Claude],
            None,
            None,
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

        let validated = crate::repo::validate_role_repo(repo.path()).unwrap();
        let build = create_derived_build_context(repo.path(), &validated, None, None, &[]).unwrap();
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

        let validated = crate::repo::validate_role_repo(repo.path()).unwrap();
        let build = create_derived_build_context(repo.path(), &validated, None, None, &[]).unwrap();

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

        let validated = crate::repo::validate_role_repo(repo.path()).unwrap();
        let build = create_derived_build_context(repo.path(), &validated, None, None, &[]).unwrap();
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

        let validated = crate::repo::validate_role_repo(repo.path()).unwrap();
        let build = create_derived_build_context(
            repo.path(),
            &validated,
            Some("docker.io/myorg/my-role:latest"),
            None,
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

        let validated = crate::repo::validate_role_repo(repo.path()).unwrap();
        let error = create_derived_build_context(repo.path(), &validated, None, None, &[])
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
}
