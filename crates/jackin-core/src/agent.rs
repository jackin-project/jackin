// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::boundary::FileMode;
use crate::paths::{PROJECT_PATH, SandboxPath};
use crate::platform::Platform;
use crate::provider::Provider;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Codex,
}

impl Agent {
    pub const ALL: [Agent; 1] = [Agent::Codex];

    pub fn spec(self) -> &'static AgentSpec {
        match self {
            Agent::Codex => &CODEX_SPEC,
        }
    }

    pub const fn provider(self) -> Provider {
        match self {
            Agent::Codex => Provider::Codex,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Agent::Codex => "codex",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CodexAuthMode {
    ApiKey,
    ChatgptLogin,
}

impl fmt::Display for CodexAuthMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CodexAuthMode::ApiKey => "api key",
            CodexAuthMode::ChatgptLogin => "chatgpt login",
        })
    }
}

#[derive(Clone, Debug)]
pub struct AgentSpec {
    pub agent: Agent,
    pub provider: Provider,
    pub binary: SandboxPath,
    pub code_mode_host: SandboxPath,
    pub state_dir: SandboxPath,
    pub workdir: SandboxPath,
    pub launch_command: &'static [&'static str],
    pub install: InstallSpec,
    pub openshell: OpenShellSpec,
    pub auth_file: AuthFileSpec,
    pub user_config: ConfigFileSpec,
    pub reserved_env: &'static [&'static str],
}

#[derive(Clone, Debug)]
pub struct InstallSpec {
    pub version: &'static str,
    pub url_template: &'static str,
    pub archive_members: &'static [&'static str],
    pub version_output: &'static str,
    pub system_config: SandboxPath,
    pub system_config_content: &'static str,
    pub targets: &'static [InstallTarget],
}

#[derive(Clone, Copy, Debug)]
pub struct InstallTarget {
    pub platform: Platform,
    pub triple: &'static str,
    pub sha256: &'static str,
}

#[derive(Clone, Debug)]
pub struct OpenShellSpec {
    pub chatgpt_profile: ProfileAsset,
    pub api_key_profile: Option<ProfileAsset>,
    pub keys: CredentialKeys,
}

#[derive(Clone, Copy, Debug)]
pub struct CredentialKeys {
    pub access_token: &'static str,
    pub refresh_token: &'static str,
    pub account_id: &'static str,
    pub id_token: &'static str,
    pub token_url: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct ProfileAsset {
    pub id: &'static str,
    pub yaml: &'static str,
}

#[derive(Clone, Debug)]
pub struct AuthFileSpec {
    pub path: SandboxPath,
    pub mode: FileMode,
}

#[derive(Clone, Debug)]
pub struct ConfigFileSpec {
    pub path: SandboxPath,
    pub mode: FileMode,
}

impl AgentSpec {
    pub fn install_target(&'static self, platform: Platform) -> Option<&'static InstallTarget> {
        self.install
            .targets
            .iter()
            .find(|target| target.platform == platform)
    }

    pub fn profile_for(&'static self, mode: CodexAuthMode) -> Option<&'static ProfileAsset> {
        match mode {
            CodexAuthMode::ChatgptLogin => Some(&self.openshell.chatgpt_profile),
            CodexAuthMode::ApiKey => self.openshell.api_key_profile.as_ref(),
        }
    }

    pub fn profiles(&'static self) -> Vec<&'static ProfileAsset> {
        let mut profiles = vec![&self.openshell.chatgpt_profile];
        profiles.extend(self.openshell.api_key_profile.as_ref());
        profiles
    }

    pub fn launch_argv(&self) -> Vec<String> {
        self.launch_command
            .iter()
            .map(|part| (*part).to_owned())
            .collect()
    }

    pub fn paths(&self) -> Vec<&SandboxPath> {
        vec![
            &self.binary,
            &self.code_mode_host,
            &self.state_dir,
            &self.workdir,
            &self.auth_file.path,
            &self.user_config.path,
            &self.install.system_config,
        ]
    }
}

pub const CODEX_PLACEHOLDER_ID_TOKEN: &str = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2F1dGgub3BlbmFpLmNvbSIsImF1ZCI6ImNvZGV4Iiwic3ViIjoib3BlbnNoZWxsLWFwcC1zZXJ2ZXIifQ.placeholder";

static CODEX_SPEC: AgentSpec = AgentSpec {
    agent: Agent::Codex,
    provider: Provider::Codex,
    binary: SandboxPath::from_static("/usr/local/bin/codex"),
    code_mode_host: SandboxPath::from_static("/usr/local/bin/codex-code-mode-host"),
    state_dir: SandboxPath::from_static("/sandbox/.codex"),
    workdir: SandboxPath::from_static(PROJECT_PATH),
    launch_command: &[
        "codex",
        "--sandbox",
        "danger-full-access",
        "-c",
        "check_for_update_on_startup=false",
    ],
    install: InstallSpec {
        version: "0.166.0",
        url_template: "https://github.com/openai/codex/releases/download/rust-v{version}/codex-package-{target}.tar.gz",
        archive_members: &["bin/codex", "bin/codex-code-mode-host"],
        version_output: "codex-cli {version}",
        system_config: SandboxPath::from_static("/etc/codex/config.toml"),
        system_config_content: "check_for_update_on_startup = false\n",
        targets: &[
            InstallTarget {
                platform: Platform::LinuxArm64,
                triple: "aarch64-unknown-linux-musl",
                sha256: "7821b77dda2e6283fb1f63f3165177a271da8833e857cf673c6ca1c9db3bfcc6",
            },
            InstallTarget {
                platform: Platform::LinuxAmd64,
                triple: "x86_64-unknown-linux-musl",
                sha256: "1d44b28bc4ac9937d7d3532264baa68a507ce364956a0d4ebb856f331cdee254",
            },
        ],
    },
    openshell: OpenShellSpec {
        chatgpt_profile: ProfileAsset {
            id: "codex",
            yaml: include_str!("assets/codex-provider.yaml"),
        },
        api_key_profile: None,
        keys: CredentialKeys {
            access_token: "CODEX_AUTH_ACCESS_TOKEN",
            refresh_token: "CODEX_AUTH_REFRESH_TOKEN",
            account_id: "CODEX_AUTH_ACCOUNT_ID",
            id_token: "CODEX_AUTH_ID_TOKEN",
            token_url: "https://auth.openai.com/oauth/token",
        },
    },
    auth_file: AuthFileSpec {
        path: SandboxPath::from_static("/sandbox/.codex/auth.json"),
        mode: FileMode(0o600),
    },
    user_config: ConfigFileSpec {
        path: SandboxPath::from_static("/sandbox/.codex/config.toml"),
        mode: FileMode(0o600),
    },
    reserved_env: &["CODEX_HOME"],
};
