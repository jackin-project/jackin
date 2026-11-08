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
