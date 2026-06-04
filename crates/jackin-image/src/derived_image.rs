//! Derived-image Dockerfile generation: renders the hook-copy section,
//! UID/GID remapping, and other build-time additions layered on top of a
//! role's base image.
//!
//! The caller (`runtime/image.rs`) provides a validated `RoleRepo` and an
//! optional `HooksConfig`. This module writes a temporary build context
//! (`DerivedBuildContext`) and returns the paths for `docker build`.
//!
//! Not responsible for: running `docker build` (`runtime/image.rs`), or the
//! base-image Dockerfile authored by the role (lives in the role repo).

use jackin_core::Agent;
use jackin_core::manifest::HooksConfig;
use jackin_manifest::ValidatedRoleRepo;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const ENTRYPOINT_SH: &str = include_str!("../../../docker/runtime/entrypoint.sh");

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
    supported: &[Agent],
    claude_config: Option<&jackin_core::manifest::ClaudeConfig>,
    jackin_capsule_bin: Option<&str>,
    agent_binaries: &[(Agent, String)],
) -> String {
    let hook_section = render_hook_section(hooks);

    // Concatenate per-agent install blocks in a stable order (Claude
    // first when present, Codex second, Amp third, Kimi fourth,
    // OpenCode fifth). Each block declares its own `ARG JACKIN_CACHE_BUST=0`
    // (see the per-agent blocks returned by `Agent::install_block`), so layer
    // cache keys advance independently when `--build-arg JACKIN_CACHE_BUST=<ts>`
    // is passed. Stable ordering keeps diffs reviewable.
    let mut install_blocks = String::new();
    let mut sorted: Vec<Agent> = supported.to_vec();
    // Stable ordering (Agent derives Ord in declaration order: Claude, Codex, Amp, Kimi, Opencode)
    // so cache-bust diffs are reviewable. No explicit sort_by_key needed.
    sorted.sort();
    for h in sorted {
        let source = agent_binaries
            .iter()
            .find(|(agent, _)| *agent == h)
            .map_or_else(
                || format!(".jackin-runtime/agent-binaries/{}", h.slug()),
                |(_, path)| path.clone(),
            );
        // Phase 2: route through the AgentRuntime adapter instead of enum method.
        install_blocks.push_str(&h.runtime().install_block(&source));
        if h == Agent::Claude {
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
# Make jackin-capsule available as a plain shell command from any session.
ENV PATH=\"/jackin/runtime:${{PATH}}\"
USER agent
ENTRYPOINT [\"/jackin/runtime/jackin-capsule\"]
"
    )
}

fn render_claude_plugin_install_block(
    claude_config: Option<&jackin_core::manifest::ClaudeConfig>,
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
    let construct_from_prefix = format!("FROM {}:", jackin_manifest::CONSTRUCT_REGISTRY_IMAGE);
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
    agent_binary_host_paths: &[(Agent, PathBuf)],
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
            jackin_diagnostics::emit_compact_line(
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
    host_paths: &[(Agent, PathBuf)],
) -> anyhow::Result<Vec<(Agent, String)>> {
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
mod tests;
