// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use jackin_core::{
    Agent, CodexAuthMode, Compatibility, EnvName, FileMode, Platform, Provider, Sha256,
    sandbox_layout,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn expected_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Amp => "amp",
        Provider::Antigravity => "antigravity",
        Provider::Claude => "claude",
        Provider::Codex => "codex",
        Provider::Cursor => "cursor",
        Provider::Gemini => "gemini",
        Provider::Grok => "grok",
        Provider::Hermes => "hermes",
        Provider::Kimi => "kimi",
        Provider::Minimax => "minimax",
        Provider::Mimo => "mimo",
        Provider::Muse => "muse",
        Provider::Opencode => "opencode",
        Provider::Openrouter => "openrouter",
        Provider::Zai => "zai",
    }
}

#[test]
fn provider_all_lists_each_provider_once_with_one_spelling() -> TestResult {
    let names = [
        "amp",
        "antigravity",
        "claude",
        "codex",
        "cursor",
        "gemini",
        "grok",
        "hermes",
        "kimi",
        "minimax",
        "mimo",
        "muse",
        "opencode",
        "openrouter",
        "zai",
    ];
    let listed: Vec<&str> = Provider::ALL.iter().map(|p| expected_name(*p)).collect();
    assert_eq!(listed, names);
    for provider in Provider::ALL {
        assert_eq!(provider.as_str(), expected_name(provider));
        assert_eq!(
            serde_json::to_string(&provider)?,
            format!("\"{}\"", provider.as_str())
        );
        if provider == Provider::Codex {
            assert_eq!(
                provider
                    .tuples()
                    .iter()
                    .map(|tuple| tuple.agent)
                    .collect::<Vec<_>>(),
                [Agent::Codex, Agent::Omp]
            );
        }
    }
    Ok(())
}

#[test]
fn agent_all_rows_match_their_provider() -> TestResult {
    for agent in Agent::ALL {
        assert_eq!(agent.spec().agent, agent);
        assert_eq!(agent.spec().provider, agent.provider());
        assert!(
            agent
                .provider()
                .tuples()
                .iter()
                .any(|tuple| tuple.agent == agent)
        );
        assert_eq!(
            serde_json::to_string(&agent)?,
            format!("\"{}\"", agent.as_str())
        );
    }
    Ok(())
}

#[test]
fn codex_row_launches_codex_with_the_d04_command_and_paths() -> TestResult {
    let spec = Agent::Codex.spec();
    assert_eq!(
        spec.launch_argv(),
        [
            "codex",
            "--sandbox",
            "danger-full-access",
            "-c",
            "check_for_update_on_startup=false"
        ]
    );
    assert_eq!(spec.binary.as_str(), "/usr/local/bin/codex");
    assert_eq!(
        spec.codex.code_mode_host.as_str(),
        "/usr/local/bin/codex-code-mode-host"
    );
    assert_eq!(spec.state_dir.as_str(), "/sandbox/.codex");
    assert_eq!(spec.workdir.as_str(), "/sandbox/project");
    assert_eq!(spec.workdir, sandbox_layout::PROJECT);
    assert_eq!(spec.auth_file.path.as_str(), "/sandbox/.codex/auth.json");
    assert_eq!(spec.auth_file.mode, FileMode(0o600));
    assert_eq!(
        spec.user_config.path.as_str(),
        "/sandbox/.codex/config.toml"
    );
    assert_eq!(spec.user_config.mode, FileMode(0o600));
    assert_eq!(
        spec.install.system_config.as_str(),
        "/etc/codex/config.toml"
    );
    let file_name = |path: &str| path.rsplit('/').next().unwrap_or_default().to_owned();
    let members: Vec<String> = spec
        .install
        .archive_members
        .iter()
        .map(|member| file_name(member))
        .collect();
    assert_eq!(
        members,
        [
            file_name(spec.binary.as_str()),
            file_name(spec.codex.code_mode_host.as_str())
        ]
    );
    let paths = spec.paths();
    for expected in [
        &spec.binary,
        &spec.codex.code_mode_host,
        &spec.state_dir,
        &spec.workdir,
        &spec.auth_file.path,
        &spec.user_config.path,
        &spec.install.system_config,
    ] {
        assert!(paths.contains(&expected), "{expected:?}");
    }
    assert_eq!(paths.len(), 7);
    Ok(())
}

#[test]
fn codex_install_matches_the_compatibility_lock_and_covers_each_platform_once() -> TestResult {
    let lock_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../compatibility.lock.toml");
    let lock: toml::Table = std::fs::read_to_string(lock_path)?.parse()?;
    let text =
        |section: &str, key: &str| lock[section][key].as_str().unwrap_or_default().to_owned();
    let pinned = Compatibility::pinned()?;
    assert_eq!(pinned.openshell_version, text("openshell", "version"));
    assert_eq!(pinned.openshell_tag, text("openshell", "tag"));
    assert_eq!(pinned.openshell_commit, text("openshell", "commit"));
    assert_eq!(pinned.codex_version, text("codex", "version"));

    let install = &Agent::Codex.spec().install;
    assert_eq!(install.version, pinned.codex_version);
    let platforms: BTreeSet<Platform> = install.targets.iter().map(|t| t.platform).collect();
    assert_eq!(install.targets.len(), 2);
    assert_eq!(
        platforms,
        BTreeSet::from([Platform::LinuxArm64, Platform::LinuxAmd64])
    );
    let spec = Agent::Codex.spec();
    let mut digests = BTreeSet::new();
    for platform in [Platform::LinuxArm64, Platform::LinuxAmd64] {
        let target = spec.install_target(platform).ok_or("missing platform")?;
        assert_eq!(target.platform, platform);
        assert!(Sha256::parse(target.sha256).is_ok(), "{}", target.sha256);
        digests.insert(target.sha256);
        let arch = match platform {
            Platform::LinuxArm64 => "aarch64-",
            Platform::LinuxAmd64 => "x86_64-",
        };
        assert!(target.triple.starts_with(arch), "{}", target.triple);
        assert!(target.triple.contains("linux"), "{}", target.triple);
    }
    assert_eq!(digests.len(), 2);
    assert_eq!(install.url_template.matches("{version}").count(), 1);
    assert_eq!(install.url_template.matches("{target}").count(), 1);
    assert_eq!(Platform::LinuxArm64.docker_arg(), "linux/arm64");
    assert_eq!(Platform::LinuxAmd64.docker_arg(), "linux/amd64");
    assert_eq!(Platform::LinuxArm64.target_arch(), "arm64");
    assert_eq!(Platform::LinuxAmd64.target_arch(), "amd64");
    Ok(())
}

#[derive(Clone, Copy)]
struct TargetPin {
    platform: Platform,
    docker: &'static str,
    arch: &'static str,
    triple: &'static str,
    sha256: &'static str,
    url: &'static str,
}

const fn target_pin(
    platform: Platform,
    docker: &'static str,
    arch: &'static str,
    triple: &'static str,
    sha256: &'static str,
    url: &'static str,
) -> TargetPin {
    TargetPin {
        platform,
        docker,
        arch,
        triple,
        sha256,
        url,
    }
}

type AgentPin = (
    Agent,
    &'static str,
    &'static str,
    &'static [&'static str],
    TargetPin,
    TargetPin,
);

const RELEASE_MATRIX: [AgentPin; 8] = [
    (
        Agent::Codex,
        "0.167.0",
        "https://github.com/openai/codex/releases/download/rust-v{version}/codex-package-{target}.tar.gz",
        &["bin/codex", "bin/codex-code-mode-host"],
        target_pin(
            Platform::LinuxArm64,
            "linux/arm64",
            "arm64",
            "aarch64-unknown-linux-musl",
            "0de261ecf1e7f3028a7952f03fca2093f749277d38590b78bb32f8567756bbb7",
            "https://github.com/openai/codex/releases/download/rust-v0.167.0/codex-package-aarch64-unknown-linux-musl.tar.gz",
        ),
        target_pin(
            Platform::LinuxAmd64,
            "linux/amd64",
            "amd64",
            "x86_64-unknown-linux-musl",
            "3bbf75e83de6b58aed28f2cf6e33ad423ec31fd4700a1457b94be557ea5a7c6e",
            "https://github.com/openai/codex/releases/download/rust-v0.167.0/codex-package-x86_64-unknown-linux-musl.tar.gz",
        ),
    ),
    (
        Agent::ClaudeCode,
        "2.1.311",
        "https://downloads.claude.ai/claude-code-releases/{version}/linux-{target}/claude",
        &[],
        target_pin(
            Platform::LinuxArm64,
            "linux/arm64",
            "arm64",
            "arm64",
            "2e2496c9a8065df93acb0fca2d341f59a6b8777649b2e025ab8c9bcd5eef61f3",
            "https://downloads.claude.ai/claude-code-releases/2.1.311/linux-arm64/claude",
        ),
        target_pin(
            Platform::LinuxAmd64,
            "linux/amd64",
            "amd64",
            "x64",
            "86dc1fcc69498159eaf15f446613e847010f9036bfbbdffd4c49350be8489ada",
            "https://downloads.claude.ai/claude-code-releases/2.1.311/linux-x64/claude",
        ),
    ),
    (
        Agent::GrokBuild,
        "1.0.77",
        "https://x.ai/cli/grok-{version}-linux-{target}",
        &[],
        target_pin(
            Platform::LinuxArm64,
            "linux/arm64",
            "arm64",
            "aarch64",
            "b30eee14a656124ef3c9da8577b5e783ef2cffabeb2b679c5c49633e77eed574",
            "https://x.ai/cli/grok-1.0.77-linux-aarch64",
        ),
        target_pin(
            Platform::LinuxAmd64,
            "linux/amd64",
            "amd64",
            "x86_64",
            "b702950aa5827d855e493278ab6215a4e0f5bceb5bba6c6243a9f55cdd143b0a",
            "https://x.ai/cli/grok-1.0.77-linux-x86_64",
        ),
    ),
    (
        Agent::MuseCode,
        "1.4.14-R7309.1",
        "https://lookaside.facebook.com/lookaside/muse/download/?channel=muse&version={version}&file=muse-{target}",
        &[],
        target_pin(
            Platform::LinuxArm64,
            "linux/arm64",
            "arm64",
            "aarch64-linux",
            "3dc39b8fa26f9963cc2038f2966c0932ed5a39f0f9efb39cc0266a261cbbcf76",
            "https://lookaside.facebook.com/lookaside/muse/download/?channel=muse&version=1.4.14-R7309.1&file=muse-aarch64-linux",
        ),
        target_pin(
            Platform::LinuxAmd64,
            "linux/amd64",
            "amd64",
            "x86-linux",
            "0809aac7cb58d18f5022b6d05e02aa1974683c0ec430e5f04d25b60ed17de2da",
            "https://lookaside.facebook.com/lookaside/muse/download/?channel=muse&version=1.4.14-R7309.1&file=muse-x86-linux",
        ),
    ),
    (
        Agent::Antigravity,
        "1.5.3-6604829099950080",
        "https://storage.googleapis.com/antigravity-public/antigravity-cli/{version}/linux-{target}",
        &["antigravity"],
        target_pin(
            Platform::LinuxArm64,
            "linux/arm64",
            "arm64",
            "arm/cli_linux_arm64.tar.gz",
            "2d50060c7b1327995e5c81ec58829c91c2564ebb9fec165d3f9f1325ec188675",
            "https://storage.googleapis.com/antigravity-public/antigravity-cli/1.5.3-6604829099950080/linux-arm/cli_linux_arm64.tar.gz",
        ),
        target_pin(
            Platform::LinuxAmd64,
            "linux/amd64",
            "amd64",
            "x64/cli_linux_x64.tar.gz",
            "1b4850599cce73a17f94ffb422df3bf9531531b77dbddc40c1a435e0c2c16807",
            "https://storage.googleapis.com/antigravity-public/antigravity-cli/1.5.3-6604829099950080/linux-x64/cli_linux_x64.tar.gz",
        ),
    ),
    (
        Agent::MimoCode,
        "0.1.16",
        "https://github.com/XiaomiMiMo/MiMo-Code/releases/download/v{version}/mimocode-linux-{target}.tar.gz",
        &["mimo"],
        target_pin(
            Platform::LinuxArm64,
            "linux/arm64",
            "arm64",
            "arm64",
            "220c621eb61a175ba0bc8e9c45c217abfed8dace1f17111c3717bceb95ad2ab3",
            "https://github.com/XiaomiMiMo/MiMo-Code/releases/download/v0.1.16/mimocode-linux-arm64.tar.gz",
        ),
        target_pin(
            Platform::LinuxAmd64,
            "linux/amd64",
            "amd64",
            "x64",
            "3793d8b08ae7c1c6cee707645da1b1c827e0f13faa5d40d076f08ea4ee1d9a5e",
            "https://github.com/XiaomiMiMo/MiMo-Code/releases/download/v0.1.16/mimocode-linux-x64.tar.gz",
        ),
    ),
    (
        Agent::KimiCode,
        "2.5.2",
        "https://github.com/MoonshotAI/kimi-code/releases/download/%40moonshot-ai%2Fkimi-code%40{version}/kimi-code-linux-{target}.tar.gz",
        &["kimi"],
        target_pin(
            Platform::LinuxArm64,
            "linux/arm64",
            "arm64",
            "arm64",
            "ac941c80c8b2b34451432323397a8e8c2e376948bc6d878fc7d2630be235b678",
            "https://github.com/MoonshotAI/kimi-code/releases/download/%40moonshot-ai%2Fkimi-code%402.5.2/kimi-code-linux-arm64.tar.gz",
        ),
        target_pin(
            Platform::LinuxAmd64,
            "linux/amd64",
            "amd64",
            "x64",
            "6614a511048219d53996454cac8594c9541bbb6cf6e8f0a081d4e878be511ce2",
            "https://github.com/MoonshotAI/kimi-code/releases/download/%40moonshot-ai%2Fkimi-code%402.5.2/kimi-code-linux-x64.tar.gz",
        ),
    ),
    (
        Agent::Omp,
        "18.18.3",
        "https://github.com/can1357/oh-my-pi/releases/download/v{version}/omp-linux-{target}",
        &[],
        target_pin(
            Platform::LinuxArm64,
            "linux/arm64",
            "arm64",
            "arm64",
            "c49a84875d915c7d0de8bb69fe180b3bf53c16600bc4d5a61ea22ac2b3ac45c9",
            "https://github.com/can1357/oh-my-pi/releases/download/v18.18.3/omp-linux-arm64",
        ),
        target_pin(
            Platform::LinuxAmd64,
            "linux/amd64",
            "amd64",
            "x64",
            "8c9a178fed74ec1da21aafb7d28cd9839b3b0c9e7d607c86cc3b6177355729f4",
            "https://github.com/can1357/oh-my-pi/releases/download/v18.18.3/omp-linux-x64",
        ),
    ),
];

#[test]
fn public_agent_pins_match_the_current_stable_release_matrix() -> TestResult {
    for (agent, version, url_template, members, arm, amd) in RELEASE_MATRIX {
        let spec = agent.spec();
        assert_eq!(spec.install.version, version, "{agent:?}");
        assert_eq!(spec.install.url_template, url_template, "{agent:?}");
        assert_eq!(spec.install.archive_members, members, "{agent:?}");
        for pin in [arm, amd] {
            let target = spec
                .install_target(pin.platform)
                .ok_or("missing platform")?;
            assert_eq!(target.platform, pin.platform, "{agent:?}");
            assert_eq!(target.platform.docker_arg(), pin.docker, "{agent:?}");
            assert_eq!(target.platform.target_arch(), pin.arch, "{agent:?}");
            assert_eq!(target.triple, pin.triple, "{agent:?}");
            assert_eq!(target.sha256, pin.sha256, "{agent:?}");
            let url = spec
                .install
                .url_template
                .replace("{version}", spec.install.version)
                .replace("{target}", target.triple);
            assert_eq!(url, pin.url, "{agent:?}");
        }
    }
    Ok(())
}

fn env_var_names(yaml: &str) -> BTreeSet<String> {
    yaml.lines()
        .filter_map(|line| line.trim().strip_prefix("env_vars: ["))
        .filter_map(|rest| rest.strip_suffix(']'))
        .flat_map(|list| list.split(',').map(|name| name.trim().to_owned()))
        .collect()
}

#[test]
fn codex_row_reserves_every_credential_env_name() -> TestResult {
    let spec = Agent::Codex.spec();
    assert_eq!(spec.reserved_env, ["CODEX_HOME"]);
    let keys = &spec.codex.openshell.keys;
    assert_eq!(keys.access_token, "CODEX_AUTH_ACCESS_TOKEN");
    assert_eq!(keys.refresh_token, "CODEX_AUTH_REFRESH_TOKEN");
    assert_eq!(keys.account_id, "CODEX_AUTH_ACCOUNT_ID");
    assert_eq!(keys.token_url, "https://auth.openai.com/oauth/token");
    let found = env_var_names(spec.codex.openshell.chatgpt_profile.yaml);
    for name in [
        "CODEX_AUTH_ACCESS_TOKEN",
        "CODEX_AUTH_REFRESH_TOKEN",
        "CODEX_AUTH_ACCOUNT_ID",
        "CODEX_AUTH_ID_TOKEN",
    ] {
        assert!(found.contains(name), "{name}");
    }
    let mut reserved: Vec<&str> = found.iter().map(String::as_str).collect();
    reserved.extend([
        "CODEX_HOME",
        keys.access_token,
        keys.refresh_token,
        keys.account_id,
    ]);
    for name in reserved {
        assert!(EnvName::parse(name)?.is_reserved(), "{name}");
    }
    Ok(())
}

#[test]
fn grok_row_uses_the_session_alias_and_trusted_launch_without_flipping_availability() -> TestResult
{
    let spec = Agent::GrokBuild.spec();
    assert_eq!(spec.launch_argv(), ["grok", "--trust"]);
    assert_eq!(spec.default_model, "grok-4.7");
    assert_eq!(spec.allowed_models, ["grok-4.7"]);
    assert_eq!(spec.credential_env_names, ["GROK_SESSION_TOKEN"]);
    assert_eq!(
        spec.provider_hosts,
        ["cli-chat-proxy.grok.com", "auth.x.ai"]
    );
    let tuple = jackin_core::lookup_tuple(Agent::GrokBuild, Provider::Grok)?;
    assert_eq!(tuple.default_model, "grok-4.7");
    assert_eq!(tuple.state, jackin_core::TupleState::Unavailable);
    assert!(!spec.credential_env_names.contains(&"XAI_API_KEY"));
    assert!(!spec.provider_hosts.contains(&"api.x.ai"));
    Ok(())
}

#[test]
fn codex_profiles_are_chatgpt_only_and_match_their_asset() -> TestResult {
    let spec = Agent::Codex.spec();
    let profile = spec
        .codex
        .profile_for(CodexAuthMode::ChatgptLogin)
        .ok_or("missing chatgpt profile")?;
    assert_eq!(profile.id, "codex");
    assert!(spec.codex.profile_for(CodexAuthMode::ApiKey).is_none());
    assert_eq!(spec.profiles().len(), 1);
    let yaml_id = profile
        .yaml
        .lines()
        .find_map(|line| line.strip_prefix("id:"))
        .ok_or("missing id line")?
        .trim();
    assert_eq!(yaml_id, profile.id);
    assert!(profile.yaml.contains("CODEX_AUTH_ACCESS_TOKEN"));
    Ok(())
}
