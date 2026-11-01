// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::Serialize;

use crate::boundary::FileMode;
use crate::paths::SandboxPath;
use crate::platform::Platform;
use crate::provider::Provider;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Agent {
    Codex,
}

impl Agent {
    pub const ALL: [Agent; 1] = [Agent::Codex];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CodexAuthMode {
    ApiKey,
    ChatgptLogin,
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
