// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::boundary::FileMode;
use crate::paths::{PROJECT_PATH, SandboxPath};
use crate::platform::Platform;
use crate::provider::Provider;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Agent {
    #[serde(rename = "codex")]
    Codex,
    #[serde(rename = "claude")]
    ClaudeCode,
    #[serde(rename = "grok")]
    GrokBuild,
    #[serde(rename = "muse")]
    MuseCode,
    #[serde(rename = "agy")]
    Antigravity,
    #[serde(rename = "minimax-code")]
    MinimaxCode,
    #[serde(rename = "mimo")]
    MimoCode,
    #[serde(rename = "kimi")]
    KimiCode,
    #[serde(rename = "omp")]
    Omp,
}

impl Agent {
    pub const ALL: [Agent; 9] = [
        Agent::Codex,
        Agent::ClaudeCode,
        Agent::GrokBuild,
        Agent::MuseCode,
        Agent::Antigravity,
        Agent::MinimaxCode,
        Agent::MimoCode,
        Agent::KimiCode,
        Agent::Omp,
    ];

    pub fn spec(self) -> &'static AgentSpec {
        match self {
            Agent::Codex => &CODEX_SPEC,
            Agent::ClaudeCode => &CLAUDE_CODE_SPEC,
            Agent::GrokBuild => &GROK_BUILD_SPEC,
            Agent::MuseCode => &MUSE_CODE_SPEC,
            Agent::Antigravity => &ANTIGRAVITY_SPEC,
            Agent::MinimaxCode => &MINIMAX_CODE_SPEC,
            Agent::MimoCode => &MIMO_CODE_SPEC,
            Agent::KimiCode => &KIMI_CODE_SPEC,
            Agent::Omp => &OMP_SPEC,
        }
    }

    pub const fn provider(self) -> Provider {
        match self {
            Agent::Codex => Provider::Codex,
            Agent::ClaudeCode => Provider::Claude,
            Agent::GrokBuild => Provider::Grok,
            Agent::MuseCode => Provider::Muse,
            Agent::Antigravity => Provider::Antigravity,
            Agent::MinimaxCode => Provider::Minimax,
            Agent::MimoCode => Provider::Mimo,
            Agent::KimiCode => Provider::Kimi,
            Agent::Omp => Provider::Zai,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Agent::Codex => "codex",
            Agent::ClaudeCode => "claude",
            Agent::GrokBuild => "grok",
            Agent::MuseCode => "muse",
            Agent::Antigravity => "agy",
            Agent::MinimaxCode => "minimax-code",
            Agent::MimoCode => "mimo",
            Agent::KimiCode => "kimi",
            Agent::Omp => "omp",
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AuthClass {
    StaticKey,
    OAuth,
    BlockedVendor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TupleState {
    Supported,
    Unavailable,
    BlockedVendor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentTuple {
    pub id: &'static str,
    pub agent: Agent,
    pub provider: Provider,
    pub auth_class: AuthClass,
    pub state: TupleState,
    pub reason: Option<&'static str>,
    pub allowed_models: &'static [&'static str],
    pub default_model: &'static str,
}

impl AgentTuple {
    pub fn omp_provider_id(&self) -> Option<&'static str> {
        match self.id {
            "T09" => Some("zai"),
            "T10" => Some("moonshot"),
            "T11" => Some("minimax-code"),
            "T12" => Some("xiaomi-token-plan-sgp"),
            _ => None,
        }
    }

    pub fn omp_credential_env(&self) -> Option<&'static str> {
        match self.id {
            "T09" => Some("ZAI_API_KEY"),
            "T10" => Some("KIMI_API_KEY"),
            "T11" => Some("MINIMAX_CODE_API_KEY"),
            "T12" => Some("XIAOMI_TOKEN_PLAN_SGP_API_KEY"),
            _ => None,
        }
    }

    pub fn omp_base_url(&self) -> Option<&'static str> {
        match self.id {
            "T09" => Some("https://api.z.ai/api/coding/paas/v4"),
            "T10" => Some("https://api.kimi.com/coding/v1"),
            "T12" => Some("https://token-plan-sgp.xiaomimimo.com/v1"),
            _ => None,
        }
    }

    pub fn omp_provider_hosts(&self) -> &'static [&'static str] {
        match self.id {
            "T09" => &["api.z.ai"],
            "T10" => &["api.kimi.com"],
            "T11" => &["api.minimax.io"],
            "T12" => &["token-plan-sgp.xiaomimimo.com"],
            _ => &[],
        }
    }

    pub fn check_model(&'static self, model: &str) -> Result<&'static AgentTuple, CatalogError> {
        if self.allowed_models.contains(&model) {
            Ok(self)
        } else {
            Err(CatalogError::UnknownModel {
                agent: self.agent,
                provider: self.provider,
                model: model.to_owned(),
            })
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    #[error("no catalog tuple for agent {} with provider {}", .agent.as_str(), .provider.as_str())]
    UnknownTuple { agent: Agent, provider: Provider },
    #[error("model {model} is not allowed for agent {} with provider {}", .agent.as_str(), .provider.as_str())]
    UnknownModel {
        agent: Agent,
        provider: Provider,
        model: String,
    },
}

pub fn tuple_profile_asset(tuple: &AgentTuple) -> &'static ProfileAsset {
    match tuple.id {
        "T01" => &ProfileAsset {
            id: "codex",
            yaml: include_str!("assets/t01-codex.yaml"),
        },
        "T02" => &ProfileAsset {
            id: "claude",
            yaml: include_str!("assets/t02-claude.yaml"),
        },
        "T03" => &ProfileAsset {
            id: "grok",
            yaml: include_str!("assets/t03-grok.yaml"),
        },
        "T04" => &ProfileAsset {
            id: "muse",
            yaml: include_str!("assets/t04-muse.yaml"),
        },
        "T05" => &ProfileAsset {
            id: "agy",
            yaml: include_str!("assets/t05-antigravity.yaml"),
        },
        "T06" => &ProfileAsset {
            id: "minimax-code",
            yaml: include_str!("assets/t06-minimax.yaml"),
        },
        "T07" => &ProfileAsset {
            id: "mimo",
            yaml: include_str!("assets/t07-mimo.yaml"),
        },
        "T08" => &ProfileAsset {
            id: "kimi",
            yaml: include_str!("assets/t08-kimi.yaml"),
        },
        "T09" => &ProfileAsset {
            id: "omp-zai",
            yaml: include_str!("assets/t09-omp-zai.yaml"),
        },
        "T10" => &ProfileAsset {
            id: "omp-kimi",
            yaml: include_str!("assets/t10-omp-kimi.yaml"),
        },
        "T11" => &ProfileAsset {
            id: "omp-minimax",
            yaml: include_str!("assets/t11-omp-minimax.yaml"),
        },
        _ => &ProfileAsset {
            id: "omp-mimo",
            yaml: include_str!("assets/t12-omp-mimo.yaml"),
        },
    }
}

pub fn lookup_tuple(agent: Agent, provider: Provider) -> Result<&'static AgentTuple, CatalogError> {
    TUPLES
        .iter()
        .find(|tuple| tuple.agent == agent && tuple.provider == provider)
        .ok_or(CatalogError::UnknownTuple { agent, provider })
}

#[derive(Clone, Debug)]
pub struct CodexAgentSpec {
    pub code_mode_host: SandboxPath,
    pub openshell: OpenShellSpec,
}

impl CodexAgentSpec {
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
}

#[derive(Clone, Debug)]
pub struct AgentSpec {
    pub agent: Agent,
    pub provider: Provider,
    pub binary: SandboxPath,
    pub state_dir: SandboxPath,
    pub workdir: SandboxPath,
    pub launch_command: &'static [&'static str],
    pub install: InstallSpec,
    pub auth_class: AuthClass,
    pub provider_hosts: &'static [&'static str],
    pub credential_env_names: &'static [&'static str],
    pub allowed_models: &'static [&'static str],
    pub default_model: &'static str,
    pub codex: &'static CodexAgentSpec,
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
    pub scan_absent_paths: &'static [&'static str],
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

    pub fn profiles(&'static self) -> Vec<&'static ProfileAsset> {
        if self.agent == Agent::Codex {
            self.codex.profiles()
        } else {
            Vec::new()
        }
    }

    pub fn launch_argv(&self) -> Vec<String> {
        self.launch_command
            .iter()
            .map(|part| (*part).to_owned())
            .collect()
    }

    pub fn paths(&self) -> Vec<&SandboxPath> {
        let mut paths = vec![&self.binary];
        if self.agent == Agent::Codex {
            paths.push(&self.codex.code_mode_host);
        }
        paths.extend([
            &self.state_dir,
            &self.workdir,
            &self.auth_file.path,
            &self.user_config.path,
        ]);
        if !self.install.system_config.as_str().is_empty() {
            paths.push(&self.install.system_config);
        }
        paths
    }
}

pub const CODEX_PLACEHOLDER_ID_TOKEN: &str = concat!(
    "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0",
    ".",
    "eyJpc3MiOiJodHRwczovL2F1dGgub3BlbmFpLmNvbSIsImF1ZCI6ImNvZGV4Iiwic3ViIjoib3BlbnNoZWxsLWFwcC1zZXJ2ZXIifQ",
    ".placeholder"
);

static CODEX_EXTRA: CodexAgentSpec = CodexAgentSpec {
    code_mode_host: SandboxPath::from_static("/usr/local/bin/codex-code-mode-host"),
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
};

static EMPTY_CODEX_SPEC: CodexAgentSpec = CodexAgentSpec {
    code_mode_host: SandboxPath::from_static("/usr/local/bin/codex-code-mode-host"),
    openshell: OpenShellSpec {
        chatgpt_profile: ProfileAsset { id: "", yaml: "" },
        api_key_profile: None,
        keys: CredentialKeys {
            access_token: "",
            refresh_token: "",
            account_id: "",
            id_token: "",
            token_url: "",
        },
    },
};

static CODEX_SPEC: AgentSpec = AgentSpec {
    agent: Agent::Codex,
    provider: Provider::Codex,
    binary: SandboxPath::from_static("/usr/local/bin/codex"),
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
        scan_absent_paths: &[
            "/etc/codex/requirements.toml",
            "/etc/codex/managed_config.toml",
            "/etc/codex/hooks.json",
            "/etc/codex/rules",
            "/etc/codex/agents",
            "/etc/codex/skills",
        ],
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
    auth_class: AuthClass::OAuth,
    provider_hosts: &["auth.openai.com", "api.openai.com"],
    credential_env_names: &[
        "CODEX_AUTH_ACCESS_TOKEN",
        "CODEX_AUTH_REFRESH_TOKEN",
        "CODEX_AUTH_ACCOUNT_ID",
        "CODEX_AUTH_ID_TOKEN",
    ],
    allowed_models: &["gpt-5.1-codex"],
    default_model: "gpt-5.1-codex",
    codex: &CODEX_EXTRA,
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

macro_rules! agent_spec {
    ($agent:expr, $provider:expr, $binary:expr, $state:expr, $auth:expr, $config:expr, $launch:expr, $version:expr, $url:expr, $targets:expr, $class:expr, $hosts:expr, $envs:expr, $models:expr, $default:expr, $reserved:expr) => {
        AgentSpec {
            agent: $agent,
            provider: $provider,
            binary: SandboxPath::from_static($binary),
            state_dir: SandboxPath::from_static($state),
            workdir: SandboxPath::from_static(PROJECT_PATH),
            launch_command: $launch,
            install: InstallSpec {
                version: $version,
                url_template: $url,
                archive_members: &[],
                version_output: "",
                system_config: SandboxPath::from_static(""),
                system_config_content: "",
                scan_absent_paths: &[],
                targets: $targets,
            },
            auth_class: $class,
            provider_hosts: $hosts,
            credential_env_names: $envs,
            allowed_models: $models,
            default_model: $default,
            codex: &EMPTY_CODEX_SPEC,
            auth_file: AuthFileSpec {
                path: SandboxPath::from_static($auth),
                mode: FileMode(0o600),
            },
            user_config: ConfigFileSpec {
                path: SandboxPath::from_static($config),
                mode: FileMode(0o600),
            },
            reserved_env: $reserved,
        }
    };
}

static CLAUDE_CODE_SPEC: AgentSpec = agent_spec!(
    Agent::ClaudeCode,
    Provider::Claude,
    "/usr/local/bin/claude",
    "/sandbox/.claude",
    "/sandbox/.claude/.credentials.json",
    "/sandbox/.claude/settings.json",
    &["claude"],
    "2.1.311",
    "https://downloads.claude.ai/claude-code-releases/{version}/linux-{target}/claude",
    &[
        InstallTarget {
            platform: Platform::LinuxArm64,
            triple: "linux-arm64",
            sha256: "2e2496c9a8065df93acb0fca2d341f59a6b8777649b2e025ab8c9bcd5eef61f3"
        },
        InstallTarget {
            platform: Platform::LinuxAmd64,
            triple: "linux-x64",
            sha256: "86dc1fcc69498159eaf15f446613e847010f9036bfbbdffd4c49350be8489ada"
        }
    ],
    AuthClass::BlockedVendor,
    &["api.anthropic.com"],
    &[],
    &["haiku"],
    "haiku",
    &["CLAUDE_CONFIG_DIR"]
);
static GROK_BUILD_SPEC: AgentSpec = agent_spec!(
    Agent::GrokBuild,
    Provider::Grok,
    "/usr/local/bin/grok",
    "/sandbox/.grok",
    "/sandbox/.grok/auth.json",
    "/sandbox/.grok/config.toml",
    &["grok"],
    "1.0.77",
    "https://x.ai/cli/grok-{version}-linux-{target}",
    &[],
    AuthClass::OAuth,
    &["x.ai"],
    &[],
    &["grok-4.7"],
    "grok-4.7",
    &["GROK_HOME"]
);
static MUSE_CODE_SPEC: AgentSpec = agent_spec!(
    Agent::MuseCode,
    Provider::Muse,
    "/usr/local/bin/muse",
    "/sandbox/.config/muse",
    "/sandbox/.config/muse/auth.json",
    "/sandbox/.config/muse/settings.json",
    &["muse"],
    "1.4.14-R7309.1",
    "https://lookaside.facebook.com/lookaside/muse/download/?channel=muse&version={version}&file=muse-{target}",
    &[
        InstallTarget {
            platform: Platform::LinuxArm64,
            triple: "aarch64-linux",
            sha256: "3dc39b8fa26f9963cc2038f2966c0932ed5a39f0f9efb39cc0266a261cbbcf76"
        },
        InstallTarget {
            platform: Platform::LinuxAmd64,
            triple: "x86-linux",
            sha256: "0809aac7cb58d18f5022b6d05e02aa1974683c0ec430e5f04d25b60ed17de2da"
        }
    ],
    AuthClass::OAuth,
    &["api.meta.ai"],
    &[],
    &["muse-spark-1.2"],
    "muse-spark-1.2",
    &["MUSE_AUTH_PATH"]
);
static ANTIGRAVITY_SPEC: AgentSpec = agent_spec!(
    Agent::Antigravity,
    Provider::Antigravity,
    "/usr/local/bin/agy",
    "/sandbox/.gemini/antigravity-cli",
    "/sandbox/.gemini/antigravity-cli/settings.json",
    "/sandbox/.gemini/antigravity-cli/settings.json",
    &["agy"],
    "1.5.1",
    "https://storage.googleapis.com/antigravity-public/antigravity-cli/{version}/linux-{target}",
    &[],
    AuthClass::BlockedVendor,
    &["antigravity.google"],
    &[],
    &["gemini-4-argon"],
    "gemini-4-argon",
    &[]
);
static MINIMAX_CODE_SPEC: AgentSpec = agent_spec!(
    Agent::MinimaxCode,
    Provider::Minimax,
    "/usr/local/bin/mcode",
    "/sandbox/.minimax",
    "/sandbox/.minimax/auth.json",
    "/sandbox/.minimax/config.yaml",
    &["mcode"],
    "0.7.6",
    "",
    &[],
    AuthClass::OAuth,
    &["api.minimax.io"],
    &[],
    &["minimax-m3"],
    "minimax-m3",
    &["MINIMAX_DATA_DIR"]
);
static MIMO_CODE_SPEC: AgentSpec = agent_spec!(
    Agent::MimoCode,
    Provider::Mimo,
    "/usr/local/bin/mimo",
    "/sandbox/.mimo",
    "/sandbox/.mimo/auth.json",
    "/sandbox/.mimo/config.toml",
    &["mimo"],
    "0.1.16",
    "https://github.com/XiaomiMiMo/MiMo-Code/releases/download/v{version}/mimocode-linux-{target}.tar.gz",
    &[
        InstallTarget {
            platform: Platform::LinuxArm64,
            triple: "arm64",
            sha256: "220c621eb61a175ba0bc8e9c45c217abfed8dace1f17111c3717bceb95ad2ab3"
        },
        InstallTarget {
            platform: Platform::LinuxAmd64,
            triple: "x64",
            sha256: "3793d8b08ae7c1c6cee707645da1b1c827e0f13faa5d40d076f08ea4ee1d9a5e"
        }
    ],
    AuthClass::StaticKey,
    &["mimo.mi.com"],
    &["MIMO_API_KEY"],
    &["mimo-v2.6-pro"],
    "mimo-v2.6-pro",
    &[]
);
static KIMI_CODE_SPEC: AgentSpec = agent_spec!(
    Agent::KimiCode,
    Provider::Kimi,
    "/usr/local/bin/kimi",
    "/sandbox/.kimi-code",
    "/sandbox/.kimi-code/credentials/kimi-code.json",
    "/sandbox/.kimi-code/config.toml",
    &["kimi"],
    "2.5.1",
    "https://github.com/MoonshotAI/kimi-code/releases/download/%40moonshot-ai%2Fkimi-code%40{version}/kimi-code-linux-{target}.tar.gz",
    &[
        InstallTarget {
            platform: Platform::LinuxArm64,
            triple: "arm64",
            sha256: "70c3a09506f7d3d0f211077eb100d34186fef1017347c73ed0d944bb2b474cf3"
        },
        InstallTarget {
            platform: Platform::LinuxAmd64,
            triple: "x64",
            sha256: "d3fe0696e2ed244200c6c42344cbd71429eff40dc9761fdcb5f994057b78bee9"
        }
    ],
    AuthClass::StaticKey,
    &["api.kimi.com"],
    &["KIMI_API_KEY"],
    &["kimi-for-coding"],
    "kimi-for-coding",
    &["KIMI_CODE_HOME"]
);
static OMP_SPEC: AgentSpec = agent_spec!(
    Agent::Omp,
    Provider::Zai,
    "/usr/local/bin/omp",
    "/sandbox/.omp",
    "/sandbox/.omp/auth.json",
    "/sandbox/.omp/config.toml",
    &["omp"],
    "18.16.0",
    "https://github.com/can1357/oh-my-pi/releases/download/v{version}/omp-linux-{target}",
    &[
        InstallTarget {
            platform: Platform::LinuxArm64,
            triple: "arm64",
            sha256: "c2be1ea85ce17c4270854b57efff2a7ea1d5a03c7557fd08721f2b9bb32698de"
        },
        InstallTarget {
            platform: Platform::LinuxAmd64,
            triple: "x64",
            sha256: "6498ed785bab00d2926e0387e0ec02362d6d45f1c3d0d1b57c1618b7f95c9264"
        }
    ],
    AuthClass::StaticKey,
    &["api.z.ai"],
    &["ZAI_API_KEY"],
    &["glm-5.3"],
    "glm-5.3",
    &[]
);

macro_rules! tuple {
    ($id:expr, $agent:expr, $provider:expr, $class:expr, $state:expr, $reason:expr, $models:expr, $default:expr) => {
        AgentTuple {
            id: $id,
            agent: $agent,
            provider: $provider,
            auth_class: $class,
            state: $state,
            reason: $reason,
            allowed_models: $models,
            default_model: $default,
        }
    };
}

pub static TUPLES: [AgentTuple; 12] = [
    tuple!(
        "T01",
        Agent::Codex,
        Provider::Codex,
        AuthClass::OAuth,
        TupleState::Supported,
        None,
        &["gpt-5.1-codex"],
        "gpt-5.1-codex"
    ),
    tuple!(
        "T02",
        Agent::ClaudeCode,
        Provider::Claude,
        AuthClass::BlockedVendor,
        TupleState::BlockedVendor,
        Some("vendor clarification pending"),
        &["haiku"],
        "haiku"
    ),
    tuple!(
        "T03",
        Agent::GrokBuild,
        Provider::Grok,
        AuthClass::OAuth,
        TupleState::Unavailable,
        Some("not yet proven on a real gateway"),
        &["grok-4.7"],
        "grok-4.7"
    ),
    tuple!(
        "T04",
        Agent::MuseCode,
        Provider::Muse,
        AuthClass::OAuth,
        TupleState::Unavailable,
        Some("not yet proven on a real gateway"),
        &["muse-spark-1.2"],
        "muse-spark-1.2"
    ),
    tuple!(
        "T05",
        Agent::Antigravity,
        Provider::Antigravity,
        AuthClass::BlockedVendor,
        TupleState::BlockedVendor,
        Some("vendor clarification pending"),
        &["gemini-4-argon"],
        "gemini-4-argon"
    ),
    tuple!(
        "T06",
        Agent::MinimaxCode,
        Provider::Minimax,
        AuthClass::OAuth,
        TupleState::Unavailable,
        Some("no stable Linux artifact"),
        &["minimax-m3"],
        "minimax-m3"
    ),
    tuple!(
        "T07",
        Agent::MimoCode,
        Provider::Mimo,
        AuthClass::StaticKey,
        TupleState::Unavailable,
        Some("not yet proven on a real gateway"),
        &["mimo-v2.6-pro"],
        "mimo-v2.6-pro"
    ),
    tuple!(
        "T08",
        Agent::KimiCode,
        Provider::Kimi,
        AuthClass::StaticKey,
        TupleState::Unavailable,
        Some("not yet proven on a real gateway"),
        &["kimi-for-coding"],
        "kimi-for-coding"
    ),
    tuple!(
        "T09",
        Agent::Omp,
        Provider::Zai,
        AuthClass::StaticKey,
        TupleState::Supported,
        None,
        &["glm-5.3"],
        "glm-5.3"
    ),
    tuple!(
        "T10",
        Agent::Omp,
        Provider::Kimi,
        AuthClass::StaticKey,
        TupleState::Unavailable,
        Some("not yet proven on a real gateway"),
        &["kimi-for-coding"],
        "kimi-for-coding"
    ),
    tuple!(
        "T11",
        Agent::Omp,
        Provider::Minimax,
        AuthClass::StaticKey,
        TupleState::Unavailable,
        Some("not yet proven on a real gateway"),
        &["minimax-m3"],
        "minimax-m3"
    ),
    tuple!(
        "T12",
        Agent::Omp,
        Provider::Mimo,
        AuthClass::StaticKey,
        TupleState::Unavailable,
        Some("not yet proven on a real gateway"),
        &["mimo-v2.6-pro"],
        "mimo-v2.6-pro"
    ),
];
