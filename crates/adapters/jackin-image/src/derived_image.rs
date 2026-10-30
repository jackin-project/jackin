// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::ImageError;
use jackin_core::Agent;
use jackin_core::{ClaudeConfig, HooksConfig};
use jackin_manifest::ValidatedRoleRepo;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const ENTRYPOINT_SH: &str = include_str!("../../../../docker/runtime/entrypoint.sh");

const AGENT_STATUS_ASSETS: &[(&str, &str)] = &[
    (
        "hooks/claude/report-hook.sh",
        include_str!("../../../../docker/runtime/agent-status/hooks/claude/report-hook.sh"),
    ),
    (
        "hooks/codex/report-hook.sh",
        include_str!("../../../../docker/runtime/agent-status/hooks/codex/report-hook.sh"),
    ),
    (
        "hooks/opencode/report-hook.sh",
        include_str!("../../../../docker/runtime/agent-status/hooks/opencode/report-hook.sh"),
    ),
    (
        "hooks/opencode/plugin.js",
        include_str!("../../../../docker/runtime/agent-status/hooks/opencode/plugin.js"),
    ),
    (
        "packs/claude.toml",
        include_str!("../../../services/jackin-agent-status/packs/claude.toml"),
    ),
    (
        "packs/codex.toml",
        include_str!("../../../services/jackin-agent-status/packs/codex.toml"),
    ),
    (
        "packs/amp.toml",
        include_str!("../../../services/jackin-agent-status/packs/amp.toml"),
    ),
    (
        "packs/kimi.toml",
        include_str!("../../../services/jackin-agent-status/packs/kimi.toml"),
    ),
    (
        "packs/opencode.toml",
        include_str!("../../../services/jackin-agent-status/packs/opencode.toml"),
    ),
    (
        "packs/grok.toml",
        include_str!("../../../services/jackin-agent-status/packs/grok.toml"),
    ),
];
const ZSHENV_SOURCE_SHIM_PATH: &str = ".jackin-runtime/zshenv-source-shim";
const ZSH_TITLE_SHIM_PATH: &str = ".jackin-runtime/zsh-title-shim";
const ZSHENV_SOURCE_SHIM: &str = "\
if [ -z \"${__JACKIN_ZSHENV_SOURCE_LOADED:-}\" ] && [ -f /jackin/runtime/hooks/source.sh ]; then
  __jackin_rc=0
  () {
    setopt local_options local_traps
    source /jackin/runtime/hooks/source.sh
  } || __jackin_rc=$?
  trap - ERR
  if [ \"$__jackin_rc\" -ne 0 ]; then
    print -u2 \"[zshenv] jackin source hook returned non-zero (exit $__jackin_rc); environment may be incomplete\"
  else
    export __JACKIN_ZSHENV_SOURCE_LOADED=1
  fi
  unset __jackin_rc
fi
";
const ZSH_TITLE_SHIM: &str = r#"
# jackin: source oh-my-zsh title hook when the active .zshrc did
# not already do so. Brings OSC 0/2 (window title) and OSC 7 (cwd)
# emit on every prompt for the multiplexer pane title.
if [ -z "${__JACKIN_AUTO_TITLE_LOADED:-}" ] && [ -f "$HOME/.oh-my-zsh/lib/termsupport.zsh" ]; then
    [ -f "$HOME/.oh-my-zsh/lib/functions.zsh" ] && source "$HOME/.oh-my-zsh/lib/functions.zsh"
    source "$HOME/.oh-my-zsh/lib/termsupport.zsh"
    export __JACKIN_AUTO_TITLE_LOADED=1
fi
"#;

#[derive(Debug)]
pub struct DerivedBuildContext {
    pub temp_dir: TempDir,
    pub context_dir: PathBuf,
    pub dockerfile_path: PathBuf,
}

#[derive(Debug, Default)]
struct HookRender {
    copy_section: String,
    final_commands: String,
}

fn render_hook_section(hooks: Option<&HooksConfig>) -> HookRender {
    use std::fmt::Write as _;

    let source_hook_declared = hooks.is_some_and(|h| h.source.is_some());
    let entries = hooks
        .into_iter()
        .flat_map(HooksConfig::entries)
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return HookRender::default();
    }

    let mut copy_section = String::new();
    let mut final_commands = String::from(
        "install -d /jackin/runtime/hooks \\\n    && install -d -o agent -g 0 /jackin/state /jackin/state/hooks",
    );
    for entry in &entries {
        let _unused = writeln!(
            copy_section,
            "COPY --link --chown=agent:0 --chmod=0755 {src} /jackin/runtime/hooks/{dst}",
            src = entry.path,
            dst = entry.filename,
        );
    }
    if source_hook_declared {
        let _unused = writeln!(
            copy_section,
            "COPY --link --chown=agent:0 --chmod=0644 {ZSHENV_SOURCE_SHIM_PATH} /jackin/runtime/zshenv-source-shim",
        );
        final_commands.push_str(" \\\n    && ");
        final_commands.push_str(
            "grep -q '__JACKIN_ZSHENV_SOURCE_LOADED' /home/agent/.zshenv 2>/dev/null \\\n    || cat /jackin/runtime/zshenv-source-shim >> /home/agent/.zshenv",
        );
    }
    HookRender {
        copy_section,
        final_commands,
    }
}

fn render_default_home_commands(agents: &[Agent]) -> String {
    let mut dirs: Vec<&'static str> = Vec::new();
    for agent in agents {
        dirs.extend(agent.runtime().state_paths().home_dirs());
    }
    dirs.sort_unstable();
    dirs.dedup();
    let mut exclude_paths: Vec<&'static str> = Vec::new();
    for agent in agents {
        exclude_paths.extend(agent.runtime().default_home_exclude_paths());
    }
    exclude_paths.sort_unstable();
    exclude_paths.dedup();

    let mut commands = String::from("install -d -o agent -g 0 /jackin/default-home");
    if !exclude_paths.is_empty() {
        commands.push_str(" \\\n    && rm -rf");
        for path in &exclude_paths {
            commands.push(' ');
            commands.push_str(&shell_quote(&format!("/home/agent/{path}")));
        }
    }
    if dirs.is_empty() {
        return commands;
    }
    commands.push_str(" \\\n    && for dir in");
    for dir in &dirs {
        commands.push(' ');
        commands.push_str(&shell_quote(dir));
    }
    commands.push_str(
        "; do \\\n        if [ -d \"/home/agent/$dir\" ]; then \\\n            mkdir -p \"$(dirname \"/jackin/default-home/$dir\")\" \\\n            && mv \"/home/agent/$dir\" \"/jackin/default-home/$dir\" \\\n            && install -d -o agent -g 0 -m 0775 \"/home/agent/$dir\"; \\\n        fi; \\\n    done",
    );
    commands.push_str(
        " \\\n    && find /jackin/default-home -type d -exec chmod g+rx {} + \\\n    && find /jackin/default-home -type f -exec chmod g+r {} +",
    );
    commands
}

fn render_default_home_guard() -> &'static str {
    "bad=\"$(find /jackin/default-home \\( -type d ! -perm -0050 -o -type f ! -perm -0040 \\) -print -quit)\" \\\n    && if [ -n \"$bad\" ]; then \\\n        echo \"jackin default-home contains a non-group-readable path: $bad\" >&2; \\\n        exit 1; \\\n    fi"
}

fn render_runtime_home_writable_commands(source_hook_declared: bool) -> String {
    let mutable_home_leaf_dirs = [
        "/home/agent",
        "/home/agent/.cache",
        "/home/agent/.cache/mise",
        "/home/agent/.config",
        "/home/agent/.config/git",
        "/home/agent/.config/fish",
        "/home/agent/.config/mise",
        "/home/agent/.local",
        "/home/agent/.local/bin",
        "/home/agent/.local/share",
        "/home/agent/.local/share/mise",
        "/home/agent/.local/share/mise/installs",
        "/home/agent/.local/share/mise/plugins",
        "/home/agent/.local/share/mise/shims",
        "/home/agent/.local/state",
        "/home/agent/.local/state/mise",
        "/home/agent/.local/state/mise/tracked-configs",
    ]
    .join(" ");
    let mut mutable_shell_files = vec!["/home/agent/.zshrc"];
    if source_hook_declared {
        mutable_shell_files.push("/home/agent/.zshenv");
    }
    mutable_shell_files.push("/home/agent/.config/fish/config.fish");
    let mutable_shell_files = mutable_shell_files.join(" ");
    format!(
        "install -d -o agent -g 0 -m 0775 {mutable_home_leaf_dirs} \\\n    && chown -R ${{JACKIN_RUN_UID}}:0 /home/agent \\\n    && chmod -R g+rwX /home/agent \\\n    && touch /home/agent/.gitconfig /home/agent/.config/git/config \\\n    && chown ${{JACKIN_RUN_UID}}:0 /home/agent/.gitconfig /home/agent/.config/git/config \\\n    && chmod 0664 /home/agent/.gitconfig /home/agent/.config/git/config \\\n    && for path in {mutable_shell_files}; do \\\n        if [ -e \"$path\" ]; then chown ${{JACKIN_RUN_UID}}:0 \"$path\" && chmod 0664 \"$path\"; fi; \\\n    done \\\n    && bad=\"$(find /home/agent \\( -type d -o -type f \\) \\( ! -uid ${{JACKIN_RUN_UID}} -o ! -group 0 -o ! -perm -0020 \\) -print -quit)\" \\\n    && if [ -n \"$bad\" ]; then \\\n        echo \"jackin runtime home contains a non-runtime-UID, non-group-0, or non-group-writable mutable path: $bad\" >&2; \\\n        exit 1; \\\n    fi"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentInstall<P> {
    Prefetched(P),
    ScriptFallback,
}

fn render_claude_plugin_section(claude: Option<&ClaudeConfig>) -> String {
    let Some(config) = claude else {
        return String::new();
    };
    if config.marketplaces.is_empty() && config.plugins.is_empty() {
        return String::new();
    }

    let mut out = String::from(
        "\n# ── Claude plugins (D2: baked at build, captured in default-home) ──\n\
         # One RUN keeps the node-heavy plugin store in a single layer while\n\
         # preserving one readable line per marketplace/plugin.\n\
         USER agent\n",
    );
    out.push_str("RUN set -eu; \\\n");
    out.push_str("    (claude plugin marketplace add anthropics/claude-plugins-official || true)");
    for marketplace in &config.marketplaces {
        out.push_str("; \\\n    claude plugin marketplace add ");
        out.push_str(&shell_quote(&marketplace.source));
        if !marketplace.sparse.is_empty() {
            out.push_str(" --sparse");
            for path in &marketplace.sparse {
                out.push(' ');
                out.push_str(&shell_quote(path));
            }
        }
    }
    for plugin in &config.plugins {
        out.push_str("; \\\n    claude plugin install ");
        out.push_str(&shell_quote(plugin));
    }
    out.push_str("; \\\n    rm -rf /home/agent/.claude/backups");
    out.push('\n');
    out.push_str("USER root\n");
    out
}

pub fn render_derived_dockerfile(
    base_dockerfile: &str,
    hooks: Option<&HooksConfig>,
    supported: &[Agent],
    jackin_capsule_bin: Option<&str>,
    agent_installs: &BTreeMap<Agent, AgentInstall<String>>,
    claude_config: Option<&ClaudeConfig>,
) -> String {
    let source_hook_declared = hooks.is_some_and(|h| h.source.is_some());
    let hook_section = render_hook_section(hooks);
    let default_home_commands = render_default_home_commands(supported);
    let default_home_guard = render_default_home_guard();
    let hook_final_commands = (!hook_section.final_commands.is_empty())
        .then(|| format!("{} \\\n    && ", hook_section.final_commands.trim_end()));
    let runtime_home_writable_commands =
        render_runtime_home_writable_commands(source_hook_declared);

    let agent_install_sections = {
        let mut s = String::new();
        for agent in supported {
            if let Some(install) = agent_installs.get(agent) {
                let block = match install {
                    AgentInstall::Prefetched(ctx_path) => agent.runtime().install_block(ctx_path),
                    AgentInstall::ScriptFallback => agent.runtime().fallback_install_block(),
                };
                s.push_str(&block);
            }
        }
        if !s.is_empty() {
            s.push_str("USER root\n");
        }
        s
    };

    let claude_plugin_section = if supported.contains(&Agent::Claude) {
        render_claude_plugin_section(claude_config)
    } else {
        String::new()
    };

    let mut agent_bin_dirs: Vec<&str> = Vec::new();
    for agent in Agent::ALL {
        for path in agent.runtime().container_binary_paths() {
            if let Some((dir, _)) = path.rsplit_once('/')
                && !agent_bin_dirs.contains(&dir)
            {
                agent_bin_dirs.push(dir);
            }
        }
    }
    let agent_path_segment = agent_bin_dirs.join(":");

    let jackin_capsule_section = jackin_capsule_bin.map_or_else(String::new, |src| {
        format!("COPY --link --chmod=0755 {src} /jackin/runtime/jackin-capsule\n")
    });

    #[expect(
        clippy::items_after_statements,
        reason = "documented residual allow; prefer expect when site is lint-true"
    )]
    const SHELL_TITLE_AND_RUNTIME_DIR_COMMANDS: &str = "\
( grep -q '__JACKIN_AUTO_TITLE_LOADED' /home/agent/.zshrc 2>/dev/null \\
      || cat /jackin/runtime/zsh-title-shim >> /home/agent/.zshrc ) \\
    && install -d -o agent -g 0 /jackin/run /jackin/state /jackin/account-credentials \\
    && install -d -o agent -g 0 -m 0700 /jackin/usage-relay-v2 \\
    && install -o agent -g 0 -m 0600 /dev/null /jackin/usage-relay-v2/owner.lock
";
    let shell_title_and_runtime_dir_commands = SHELL_TITLE_AND_RUNTIME_DIR_COMMANDS;

    format!(
        "\
# syntax=docker/dockerfile:1.7
{base_dockerfile}
USER root
# ─────────────────────────────────────────────────────────────────────────────
# Derived role image, generated by jackin. Layered for readability and reuse:
# each agent install, the Claude plugin bundle, the default-home snapshot, and
# runtime finalization are separate, individually cached steps. Editing one step
# only rebuilds it and the steps after it.
# ─────────────────────────────────────────────────────────────────────────────
ARG JACKIN_RUN_UID=1000

# ── Agent CLIs (D1: each agent's binary baked from its install_block) ──
{agent_install_sections}{claude_plugin_section}
# ── Default-home snapshot (D4): move each baked agent home into the
#    /jackin/default-home seed, leaving an empty home for the runtime mount.
#    runtime-setup copies the seed into an empty durable home on first launch. ──
RUN {default_home_commands}
RUN {default_home_guard}

# ── Volatile launcher runtime payload last: upgrades rebuild only cheap layers. ──
{hook_copy_section}COPY --link --chmod=0755 .jackin-runtime/entrypoint.sh /jackin/runtime/entrypoint.sh
COPY --link --chmod=0755 .jackin-runtime/agent-status /jackin/runtime/agent-status
COPY --link --chown=agent:0 --chmod=0644 {zsh_title_shim_path} /jackin/runtime/zsh-title-shim
{jackin_capsule_section}
# ── Runtime finalization: shell-title shim into .zshrc + jackin runtime dirs ──
RUN {hook_final_commands}{shell_title_and_runtime_dir_commands}

# ── Runtime home mutability: PID 1 runs as host UID/GID but resolves as agent. ──
RUN {runtime_home_writable_commands}

# /jackin/runtime for jackin-capsule; agent bin dirs so binaries baked by
# install_block() resolve for sibling tabs that share the same container.
ENV PATH=\"/jackin/runtime:{agent_path_segment}:${{PATH}}\"
USER agent
ENTRYPOINT [\"/jackin/runtime/jackin-capsule\"]
",
        hook_copy_section = hook_section.copy_section,
        hook_final_commands = hook_final_commands.unwrap_or_default(),
        zsh_title_shim_path = ZSH_TITLE_SHIM_PATH,
    )
}

pub fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_owned();
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
                line.to_owned()
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
    base_image_override: Option<&str>,
    jackin_capsule_host_path: Option<&str>,
) -> anyhow::Result<DerivedBuildContext> {
    let supported = validated.manifest.supported_agents();
    create_derived_build_context_for_agents(
        repo_dir,
        validated,
        base_image_override,
        jackin_capsule_host_path,
        &supported,
        &BTreeMap::new(),
    )
}

fn stage_prefetched_agent_binary(
    agent_bin_dir: &Path,
    agent: Agent,
    host_path: &Path,
) -> Result<String, ImageError> {
    let ctx_path = format!(".jackin-runtime/agent-binaries/{}", agent.slug());
    let dst = agent_bin_dir.join(agent.slug());
    std::fs::copy(host_path, &dst).map_err(|e| {
        ImageError::msg(format!(
            "failed to copy {} binary into build context: {e}",
            agent.slug()
        ))
    })?;

    if agent == Agent::Codex {
        let sibling = host_path.with_file_name(jackin_core::CODEX_CODE_MODE_HOST_BINARY);
        if sibling.is_file() {
            let sibling_dst = agent_bin_dir.join(jackin_core::CODEX_CODE_MODE_HOST_BINARY);
            std::fs::copy(&sibling, &sibling_dst).map_err(|e| {
                ImageError::msg(format!(
                    "failed to copy codex sibling binary into build context: {e}"
                ))
            })?;
        }
    }
    Ok(ctx_path)
}

pub fn create_derived_build_context_for_agents(
    repo_dir: &Path,
    validated: &ValidatedRoleRepo,
    base_image_override: Option<&str>,
    jackin_capsule_host_path: Option<&str>,
    agents_to_install: &[Agent],
    agent_installs: &BTreeMap<Agent, AgentInstall<PathBuf>>,
) -> anyhow::Result<DerivedBuildContext> {
    let temp_dir = tempfile::tempdir()?;
    let context_dir = temp_dir.path().join("context");
    if base_image_override.is_some() {
        std::fs::create_dir_all(&context_dir)?;
        copy_declared_hook_files(repo_dir, &context_dir, validated.manifest.hooks.as_ref())?;
    } else {
        copy_dir_all(repo_dir, &context_dir)?;
    }

    let runtime_dir = context_dir.join(".jackin-runtime");
    std::fs::create_dir_all(&runtime_dir)?;
    std::fs::write(runtime_dir.join("entrypoint.sh"), ENTRYPOINT_SH)?;
    std::fs::write(runtime_dir.join("zsh-title-shim"), ZSH_TITLE_SHIM)?;

    let agent_status_dir = runtime_dir.join("agent-status");
    for (rel, content) in AGENT_STATUS_ASSETS {
        let dst = agent_status_dir.join(rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(dst, content)?;
    }
    if validated
        .manifest
        .hooks
        .as_ref()
        .is_some_and(|hooks| hooks.source.is_some())
    {
        std::fs::write(runtime_dir.join("zshenv-source-shim"), ZSHENV_SOURCE_SHIM)?;
    }

    let jackin_capsule_ctx_path = if let Some(host_path) = jackin_capsule_host_path {
        let dst = runtime_dir.join("jackin-capsule");
        std::fs::copy(host_path, &dst).map_err(|e| {
            ImageError::msg(format!(
                "failed to copy jackin-capsule binary into build context: {e}"
            ))
        })?;
        Some(".jackin-runtime/jackin-capsule".to_owned())
    } else {
        None
    };

    let agent_bin_dir = runtime_dir.join("agent-binaries");
    let mut ctx_agent_installs: BTreeMap<Agent, AgentInstall<String>> = BTreeMap::new();
    let mut agent_bin_dir_created = false;
    for agent in agents_to_install {
        if let Some(install) = agent_installs.get(agent) {
            let ctx_install = match install {
                AgentInstall::Prefetched(host_path) => {
                    if !agent_bin_dir_created {
                        std::fs::create_dir_all(&agent_bin_dir)?;
                        agent_bin_dir_created = true;
                    }
                    let ctx_path =
                        stage_prefetched_agent_binary(&agent_bin_dir, *agent, host_path)?;
                    AgentInstall::Prefetched(ctx_path)
                }
                AgentInstall::ScriptFallback => AgentInstall::ScriptFallback,
            };
            ctx_agent_installs.insert(*agent, ctx_install);
        }
    }

    let hooks = validated.manifest.hooks.as_ref();

    let base_dockerfile = if let Some(image) = base_image_override {
        if !looks_like_valid_image_ref(image) {
            return Err(ImageError::msg(format!(
                "base_image_override {image:?} is not a valid Docker image reference; refusing to interpolate into Dockerfile FROM line"
            ))
            .into());
        }
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

    let dockerfile_path = context_dir.join(".jackin-runtime/DerivedDockerfile");
    let claude_config = validated.manifest.claude.as_ref();
    std::fs::write(
        &dockerfile_path,
        render_derived_dockerfile(
            &base_dockerfile,
            hooks,
            agents_to_install,
            jackin_capsule_ctx_path.as_deref(),
            &ctx_agent_installs,
            claude_config,
        ),
    )?;
    ensure_runtime_assets_are_included(&context_dir, hooks)?;

    Ok(DerivedBuildContext {
        temp_dir,
        context_dir,
        dockerfile_path,
    })
}

pub fn create_role_base_build_context(
    repo_dir: &Path,
    validated: &ValidatedRoleRepo,
    published_base: Option<&str>,
) -> anyhow::Result<DerivedBuildContext> {
    let temp_dir = tempfile::tempdir()?;
    let context_dir = temp_dir.path().join("context");
    let base_dockerfile = if let Some(image) = published_base {
        if !looks_like_valid_image_ref(image) {
            return Err(ImageError::msg(format!(
                "published base {image:?} is not a valid Docker image reference; refusing to interpolate into Dockerfile FROM line"
            ))
            .into());
        }
        std::fs::create_dir_all(&context_dir)?;
        format!("FROM {image}\n")
    } else {
        copy_dir_all(repo_dir, &context_dir)?;
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
    let runtime_dir = context_dir.join(".jackin-runtime");
    std::fs::create_dir_all(&runtime_dir)?;
    let dockerfile_path = runtime_dir.join("BaseDockerfile");
    std::fs::write(&dockerfile_path, base_dockerfile)?;
    Ok(DerivedBuildContext {
        temp_dir,
        context_dir,
        dockerfile_path,
    })
}

fn copy_declared_hook_files(
    repo_dir: &Path,
    context_dir: &Path,
    hooks: Option<&HooksConfig>,
) -> anyhow::Result<()> {
    for entry in hooks.into_iter().flat_map(HooksConfig::entries) {
        let src = repo_dir.join(entry.path);
        let metadata = std::fs::symlink_metadata(&src).map_err(|e| {
            ImageError::msg(format!(
                "failed to inspect hook {} for derived build context: {e}",
                entry.path
            ))
        })?;
        let effective_src = if metadata.file_type().is_symlink() {
            let canonical_root = std::fs::canonicalize(repo_dir).map_err(|e| {
                ImageError::msg(format!(
                    "failed to inspect hook {} for derived build context: {e}",
                    entry.path
                ))
            })?;
            contained_symlink_target(&src, &canonical_root).ok_or_else(|| {
                ImageError::SymlinkInBuildContext {
                    path: entry.path.to_owned(),
                }
            })?
        } else {
            src
        };
        if !std::fs::metadata(&effective_src).is_ok_and(|meta| meta.is_file()) {
            return Err(ImageError::HookNotRegularFile {
                path: entry.path.to_owned(),
            }
            .into());
        }
        let dst = context_dir.join(entry.path);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&effective_src, &dst).map_err(|e| {
            ImageError::msg(format!(
                "failed to copy hook {} into derived build context: {e}",
                entry.path
            ))
        })?;
    }
    Ok(())
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
        "!.jackin-runtime/".to_owned(),
        "!.jackin-runtime/entrypoint.sh".to_owned(),
        "!.jackin-runtime/zsh-title-shim".to_owned(),
        "!.jackin-runtime/DerivedDockerfile".to_owned(),
        "!.jackin-runtime/agent-status/".to_owned(),
        "!.jackin-runtime/agent-status/**".to_owned(),
    ];
    if context_dir.join(ZSHENV_SOURCE_SHIM_PATH).exists() {
        rules.push(format!("!{ZSHENV_SOURCE_SHIM_PATH}"));
    }
    if context_dir.join(".jackin-runtime/jackin-capsule").exists() {
        rules.push("!.jackin-runtime/jackin-capsule".to_owned());
    }
    let staged_agent_binary_dir = context_dir.join(".jackin-runtime/agent-binaries");
    if staged_agent_binary_dir.exists() {
        rules.push("!.jackin-runtime/agent-binaries/".to_owned());
        let mut staged_binaries = Vec::new();
        for entry in std::fs::read_dir(&staged_agent_binary_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                staged_binaries.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        staged_binaries.sort();
        for binary in staged_binaries {
            rules.push(format!("!.jackin-runtime/agent-binaries/{binary}"));
        }
    }
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

fn contained_symlink_target(link: &Path, canonical_root: &Path) -> Option<PathBuf> {
    let target = std::fs::canonicalize(link).ok()?;
    target.starts_with(canonical_root).then_some(target)
}

fn copy_dir_all(from: &Path, to: &Path) -> anyhow::Result<()> {
    let canonical_root = std::fs::canonicalize(from)?;
    copy_dir_all_under_root(from, to, &canonical_root)
}

fn copy_dir_all_under_root(from: &Path, to: &Path, canonical_root: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" || name == ".jackin-runtime" {
            continue;
        }
        let file_type = entry.file_type()?;
        let destination = to.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_all_under_root(&entry.path(), &destination, canonical_root)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), destination)?;
        } else if file_type.is_symlink() {
            let target =
                contained_symlink_target(&entry.path(), canonical_root).ok_or_else(|| {
                    ImageError::RoleRepoSymlink {
                        path: entry.path().display().to_string(),
                    }
                })?;
            if target.is_dir() {
                copy_dir_all_under_root(&target, &destination, canonical_root)?;
            } else if target.is_file() {
                std::fs::copy(&target, destination)?;
            } else {
                return Err(ImageError::RoleRepoSymlink {
                    path: entry.path().display().to_string(),
                }
                .into());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;
