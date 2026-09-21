// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `agent` — tests.
use super::*;

#[test]
fn slug_round_trip() {
    for &h in Agent::ALL {
        assert_eq!(Agent::from_str(h.slug()).unwrap(), h);
    }
}

#[test]
fn display_matches_slug() {
    assert_eq!(format!("{}", Agent::Claude), "claude");
    assert_eq!(format!("{}", Agent::Codex), "codex");
    assert_eq!(format!("{}", Agent::Amp), "amp");
    assert_eq!(format!("{}", Agent::Kimi), "kimi");
    assert_eq!(format!("{}", Agent::Opencode), "opencode");
    assert_eq!(format!("{}", Agent::Grok), "grok");
    assert_eq!(format!("{}", Agent::Antigravity), "antigravity");
    assert_eq!(format!("{}", Agent::Gemini), "gemini");
    assert_eq!(format!("{}", Agent::Cursor), "cursor");
    assert_eq!(format!("{}", Agent::Muse), "muse");
    assert_eq!(format!("{}", Agent::Omp), "omp");
    assert_eq!(format!("{}", Agent::Hermes), "hermes");
}

#[test]
fn rejects_unknown_agent() {
    let err = Agent::from_str("foo").unwrap_err();
    assert!(err.to_string().contains("foo"));
    assert!(err.to_string().contains("claude"));
    assert!(err.to_string().contains("kimi"));
    assert!(err.to_string().contains("opencode"));
}

#[test]
fn serializes_lowercase() {
    let json = serde_json::to_string(&Agent::Claude).unwrap();
    assert_eq!(json, "\"claude\"");
}

#[test]
fn deserializes_lowercase() {
    let h: Agent = serde_json::from_str("\"codex\"").unwrap();
    assert_eq!(h, Agent::Codex);
}

#[test]
fn codex_install_block_installs_cli_as_agent_with_current_archive_layout() {
    assert_eq!(
        Agent::Codex.install_block(".jackin-runtime/agent-binaries/codex"),
        "\
USER agent
COPY --link --chown=agent:0 --chmod=0755 .jackin-runtime/agent-binaries/codex /home/agent/.local/bin/codex
ENV PATH=\"/home/agent/.local/bin:${PATH}\"
RUN set -euxo pipefail && \\
    codex --version
"
    );
}

#[test]
fn claude_install_block_installs_cached_cli() {
    assert_eq!(
        Agent::Claude.install_block(".jackin-runtime/agent-binaries/claude"),
        "\
USER agent
ARG JACKIN_CACHE_BUST=0
ENV XDG_CACHE_HOME=\"/home/agent/.cache\"
COPY --link --chown=agent:0 --chmod=0755 .jackin-runtime/agent-binaries/claude /jackin/agent-binaries/claude
RUN --mount=type=cache,id=jackin-agent-prefetch-claude,target=/home/agent/.cache,uid=1000,gid=1000,sharing=locked \\
    set -euxo pipefail && \\
    : \"${JACKIN_CACHE_BUST}\" && \\
    /jackin/agent-binaries/claude install && \\
    claude --version
"
    );
}

#[test]
fn amp_install_block_installs_cached_cli() {
    assert_eq!(
        Agent::Amp.install_block(".jackin-runtime/agent-binaries/amp"),
        "\
USER agent
COPY --link --chown=agent:0 --chmod=0755 .jackin-runtime/agent-binaries/amp /home/agent/.amp/bin/amp
ENV PATH=\"/home/agent/.local/bin:/home/agent/.amp/bin:${PATH}\"
RUN set -euxo pipefail && \\
    amp --version
"
    );
}

#[test]
fn kimi_install_block_installs_cached_cli() {
    assert_eq!(
        Agent::Kimi.install_block(".jackin-runtime/agent-binaries/kimi"),
        "\
USER agent
COPY --link --chown=agent:0 --chmod=0755 .jackin-runtime/agent-binaries/kimi /home/agent/.kimi-code/bin/kimi
ENV PATH=\"/home/agent/.kimi-code/bin:/home/agent/.local/bin:${PATH}\"
RUN set -euxo pipefail && \\
    kimi --version
"
    );
}

#[test]
fn opencode_install_block_installs_cached_cli() {
    assert_eq!(
        Agent::Opencode.install_block(".jackin-runtime/agent-binaries/opencode"),
        "\
USER agent
COPY --link --chown=agent:0 --chmod=0755 .jackin-runtime/agent-binaries/opencode /home/agent/.opencode/bin/opencode
ENV PATH=\"/home/agent/.opencode/bin:${PATH}\"
RUN set -euxo pipefail && \\
    opencode --version
"
    );
}

#[test]
fn grok_install_block_installs_cached_cli() {
    assert_eq!(
        Agent::Grok.install_block(".jackin-runtime/agent-binaries/grok"),
        "\
USER agent
ARG JACKIN_CACHE_BUST=0
COPY --link --chown=agent:0 --chmod=0755 .jackin-runtime/agent-binaries/grok /home/agent/.grok/bin/grok
COPY --link --chown=agent:0 --chmod=0755 .jackin-runtime/agent-binaries/grok /home/agent/.grok/bin/agent
ENV PATH=\"/home/agent/.grok/bin:/home/agent/.local/bin:${PATH}\"
RUN set -euxo pipefail && \\
    : \"${JACKIN_CACHE_BUST}\" && \\
    grok --version
"
    );
}

#[test]
fn fallback_install_blocks_use_official_installers() {
    let curl_flags =
        "--connect-timeout 15 --max-time 120 --retry 2 --retry-delay 2 --retry-connrefused";
    // (agent, installer command, smoke-check binary): the binary differs from
    // the slug where the CLI name differs (`agy`, `cursor-agent`).
    let cases = [
        (
            Agent::Claude,
            format!("curl -fsSL {curl_flags} https://claude.ai/install.sh | bash"),
            "claude",
        ),
        (
            Agent::Codex,
            format!(
                "curl -fsSL {curl_flags} https://chatgpt.com/codex/install.sh | CODEX_NON_INTERACTIVE=1 bash"
            ),
            "codex",
        ),
        (
            Agent::Amp,
            format!("curl -fsSL {curl_flags} https://ampcode.com/install.sh | bash"),
            "amp",
        ),
        (
            Agent::Kimi,
            format!("curl -fsSL {curl_flags} https://code.kimi.com/kimi-code/install.sh | bash"),
            "kimi",
        ),
        (
            Agent::Opencode,
            format!("curl -fsSL {curl_flags} https://opencode.ai/install | bash"),
            "opencode",
        ),
        (
            Agent::Grok,
            format!("curl -fsSL {curl_flags} https://x.ai/cli/install.sh | bash"),
            "grok",
        ),
        (
            Agent::Antigravity,
            format!("curl -fsSL {curl_flags} https://antigravity.google/cli/install.sh | bash"),
            "agy",
        ),
        (
            Agent::Gemini,
            "npm install -g @google/gemini-cli".to_owned(),
            "gemini",
        ),
        (
            Agent::Cursor,
            format!("curl -fsSL {curl_flags} https://cursor.com/install | bash"),
            "cursor-agent",
        ),
        (
            Agent::Omp,
            "npm install -g @oh-my-pi/pi-coding-agent@18.2.4".to_owned(),
            "omp",
        ),
        (
            Agent::Hermes,
            format!(
                "curl -fsSL {curl_flags} https://hermes-agent.nousresearch.com/install.sh | bash"
            ),
            "hermes",
        ),
    ];

    for (agent, command, version_bin) in cases {
        assert_eq!(agent.fallback_install_command(), command);
        let block = agent.fallback_install_block();
        assert!(block.contains(&command), "{agent} fallback block: {block}");
        assert!(
            block.contains("ENV XDG_CACHE_HOME=\"/home/agent/.cache\""),
            "{agent} fallback block should point installers at jackin-owned cache dir: {block}"
        );
        assert!(
            block.contains(&format!(
                "RUN --mount=type=cache,id=jackin-agent-fallback-{version_bin},target=/home/agent/.cache"
            )),
            "{agent} fallback block should use an agent-scoped BuildKit cache mount: {block}"
        );
        assert!(
            block.contains(&format!("{version_bin} --version")),
            "{agent} fallback block must verify install: {block}"
        );
    }
}

#[test]
fn muse_fallback_install_fails_closed_without_invented_url() {
    let command = Agent::Muse.fallback_install_command();
    assert!(command.contains("exit 1"), "{command}");
    assert!(
        !command.contains("https://"),
        "muse fallback must not invent a download URL: {command}"
    );
    let block = Agent::Muse.fallback_install_block();
    assert!(block.contains(command), "{block}");
}

/// CLI binary name per agent: differs from the slug where upstream named the
/// binary differently (`agy`, `cursor-agent`).
fn cli_binary(agent: Agent) -> &'static str {
    match agent {
        Agent::Antigravity => "agy",
        Agent::Cursor => "cursor-agent",
        _ => agent.slug(),
    }
}

#[test]
fn prefetched_install_blocks_verify_installed_cli_versions() {
    for agent in Agent::ALL {
        let block =
            agent.install_block(&format!(".jackin-runtime/agent-binaries/{}", agent.slug()));
        assert!(
            block.contains(&format!("{} --version", cli_binary(*agent))),
            "{agent} prefetched install block must print the baked CLI version: {block}"
        );
    }
}

// Tests for `agent` — auth table tests.
use crate::auth::AuthForwardMode;
use crate::env_model;

#[test]
fn required_env_var_table() {
    assert_eq!(Agent::Claude.required_env_var(AuthForwardMode::Sync), None);
    assert_eq!(
        Agent::Claude.required_env_var(AuthForwardMode::ApiKey),
        Some(env_model::ANTHROPIC_API_KEY_ENV_NAME)
    );
    assert_eq!(
        Agent::Claude.required_env_var(AuthForwardMode::OAuthToken),
        Some(env_model::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME)
    );
    assert_eq!(
        Agent::Claude.required_env_var(AuthForwardMode::Ignore),
        None
    );

    assert_eq!(Agent::Codex.required_env_var(AuthForwardMode::Sync), None);
    assert_eq!(
        Agent::Codex.required_env_var(AuthForwardMode::ApiKey),
        Some(env_model::OPENAI_API_KEY_ENV_NAME)
    );
    assert_eq!(
        Agent::Codex.required_env_var(AuthForwardMode::OAuthToken),
        None
    );
    assert_eq!(Agent::Codex.required_env_var(AuthForwardMode::Ignore), None);

    assert_eq!(Agent::Amp.required_env_var(AuthForwardMode::Sync), None);
    assert_eq!(
        Agent::Amp.required_env_var(AuthForwardMode::ApiKey),
        Some(env_model::AMP_API_KEY_ENV_NAME)
    );
    assert_eq!(
        Agent::Amp.required_env_var(AuthForwardMode::OAuthToken),
        None
    );
    assert_eq!(Agent::Amp.required_env_var(AuthForwardMode::Ignore), None);

    assert_eq!(Agent::Kimi.required_env_var(AuthForwardMode::Sync), None);
    assert_eq!(
        Agent::Kimi.required_env_var(AuthForwardMode::ApiKey),
        Some(env_model::KIMI_API_KEY_ENV_NAME)
    );
    assert_eq!(
        Agent::Kimi.required_env_var(AuthForwardMode::OAuthToken),
        None
    );
    assert_eq!(Agent::Kimi.required_env_var(AuthForwardMode::Ignore), None);

    assert_eq!(
        Agent::Opencode.required_env_var(AuthForwardMode::Sync),
        None
    );
    assert_eq!(
        Agent::Opencode.required_env_var(AuthForwardMode::ApiKey),
        Some(env_model::OPENCODE_API_KEY_ENV_NAME)
    );
    assert_eq!(
        Agent::Opencode.required_env_var(AuthForwardMode::OAuthToken),
        None
    );
    assert_eq!(
        Agent::Opencode.required_env_var(AuthForwardMode::Ignore),
        None
    );

    assert_eq!(Agent::Grok.required_env_var(AuthForwardMode::Sync), None);
    assert_eq!(
        Agent::Grok.required_env_var(AuthForwardMode::ApiKey),
        Some(env_model::XAI_API_KEY_ENV_NAME)
    );
    assert_eq!(
        Agent::Grok.required_env_var(AuthForwardMode::OAuthToken),
        None
    );
    assert_eq!(Agent::Grok.required_env_var(AuthForwardMode::Ignore), None);

    for agent in [Agent::Antigravity, Agent::Gemini] {
        assert_eq!(agent.required_env_var(AuthForwardMode::Sync), None);
        assert_eq!(
            agent.required_env_var(AuthForwardMode::ApiKey),
            Some(env_model::GEMINI_API_KEY_ENV_NAME)
        );
        assert_eq!(agent.required_env_var(AuthForwardMode::OAuthToken), None);
        assert_eq!(agent.required_env_var(AuthForwardMode::Ignore), None);
    }

    assert_eq!(
        Agent::Cursor.required_env_var(AuthForwardMode::ApiKey),
        Some(env_model::CURSOR_API_KEY_ENV_NAME)
    );
    assert_eq!(
        Agent::Muse.required_env_var(AuthForwardMode::ApiKey),
        Some(env_model::META_API_KEY_ENV_NAME)
    );

    // Multi-provider clients have no single agent-level variable.
    for agent in [Agent::Omp, Agent::Hermes] {
        for mode in [
            AuthForwardMode::Sync,
            AuthForwardMode::ApiKey,
            AuthForwardMode::OAuthToken,
            AuthForwardMode::Ignore,
        ] {
            assert_eq!(agent.required_env_var(mode), None);
        }
    }
}

#[test]
fn supported_modes_claude_includes_oauth_token() {
    let modes = Agent::Claude.supported_modes();
    assert!(modes.contains(&AuthForwardMode::Sync));
    assert!(modes.contains(&AuthForwardMode::ApiKey));
    assert!(modes.contains(&AuthForwardMode::OAuthToken));
    assert!(modes.contains(&AuthForwardMode::Ignore));
}

#[test]
fn supported_modes_codex_excludes_oauth_token() {
    let modes = Agent::Codex.supported_modes();
    assert!(modes.contains(&AuthForwardMode::Sync));
    assert!(modes.contains(&AuthForwardMode::ApiKey));
    assert!(!modes.contains(&AuthForwardMode::OAuthToken));
    assert!(modes.contains(&AuthForwardMode::Ignore));
}

#[test]
fn supported_modes_amp_excludes_oauth_token() {
    let modes = Agent::Amp.supported_modes();
    assert!(modes.contains(&AuthForwardMode::Sync));
    assert!(modes.contains(&AuthForwardMode::ApiKey));
    assert!(!modes.contains(&AuthForwardMode::OAuthToken));
    assert!(modes.contains(&AuthForwardMode::Ignore));
}

#[test]
fn supported_modes_kimi_excludes_oauth_token() {
    let modes = Agent::Kimi.supported_modes();
    assert!(modes.contains(&AuthForwardMode::Sync));
    assert!(modes.contains(&AuthForwardMode::ApiKey));
    assert!(!modes.contains(&AuthForwardMode::OAuthToken));
    assert!(modes.contains(&AuthForwardMode::Ignore));
}

#[test]
fn supported_modes_opencode_excludes_oauth_token() {
    let modes = Agent::Opencode.supported_modes();
    assert!(modes.contains(&AuthForwardMode::Sync));
    assert!(modes.contains(&AuthForwardMode::ApiKey));
    assert!(!modes.contains(&AuthForwardMode::OAuthToken));
    assert!(modes.contains(&AuthForwardMode::Ignore));
}

#[test]
fn supported_modes_new_agents_are_sync_api_key_ignore() {
    for agent in [
        Agent::Antigravity,
        Agent::Gemini,
        Agent::Cursor,
        Agent::Muse,
        Agent::Omp,
        Agent::Hermes,
    ] {
        assert_eq!(
            agent.supported_modes(),
            &[
                AuthForwardMode::Sync,
                AuthForwardMode::ApiKey,
                AuthForwardMode::Ignore
            ],
            "{agent:?}",
        );
    }
}
