// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::agent::Agent;
use crate::constants::current_manifest_version;
use crate::{DindGrant, DockerSecurityProfile};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleManifest {
    #[serde(default = "current_manifest_version", rename = "version")]
    pub version: String,

    pub dockerfile: String,

    #[serde(default)]
    pub published_image: Option<String>,

    #[serde(default)]
    pub identity: Option<IdentityConfig>,

    #[serde(default)]
    pub agents: Option<Vec<Agent>>,

    #[serde(default)]
    pub claude: Option<ClaudeConfig>,

    #[serde(default)]
    pub codex: Option<CodexConfig>,

    #[serde(default)]
    pub amp: Option<AmpConfig>,

    #[serde(default)]
    pub kimi: Option<KimiConfig>,

    #[serde(default)]
    pub opencode: Option<OpencodeConfig>,

    #[serde(default)]
    pub grok: Option<GrokConfig>,

    #[serde(default)]
    pub hooks: Option<HooksConfig>,

    #[serde(default)]
    pub env: BTreeMap<String, EnvVarDecl>,

    #[serde(default)]
    pub docker: Option<ManifestDockerConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestDockerConfig {
    #[serde(default)]
    pub min_profile: Option<DockerSecurityProfile>,

    #[serde(default)]
    pub dind: Option<DindGrant>,

    #[serde(default)]
    pub allowed_hosts: Vec<String>,

    #[serde(default)]
    pub capabilities_add: Vec<String>,
}

impl RoleManifest {
    pub fn display_name(&self, fallback: &str) -> String {
        self.identity
            .as_ref()
            .map_or_else(|| fallback.to_owned(), |id| id.name.clone())
    }

    pub fn supported_agents(&self) -> Vec<Agent> {
        self.agents.clone().unwrap_or_else(|| vec![Agent::Claude])
    }

    pub const fn has_agent_config(&self, agent: Agent) -> bool {
        match agent {
            Agent::Claude => self.claude.is_some(),
            Agent::Codex => self.codex.is_some(),
            Agent::Amp => self.amp.is_some(),
            Agent::Kimi => self.kimi.is_some(),
            Agent::Opencode => self.opencode.is_some(),
            Agent::Grok => self.grok.is_some(),

            Agent::Antigravity
            | Agent::Gemini
            | Agent::Cursor
            | Agent::Muse
            | Agent::Omp
            | Agent::Hermes => false,
        }
    }

    pub fn agent_model(&self, agent: Agent) -> Option<&str> {
        match agent {
            Agent::Claude => self.claude.as_ref().and_then(|c| c.model.as_deref()),
            Agent::Codex => self.codex.as_ref().and_then(|c| c.model.as_deref()),
            Agent::Amp => None,
            Agent::Kimi => self.kimi.as_ref().and_then(|c| c.model.as_deref()),
            Agent::Opencode => self.opencode.as_ref().and_then(|c| c.model.as_deref()),
            Agent::Grok => self.grok.as_ref().and_then(|c| c.model.as_deref()),

            Agent::Antigravity
            | Agent::Gemini
            | Agent::Cursor
            | Agent::Muse
            | Agent::Omp
            | Agent::Hermes => None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexConfig {
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmpConfig {}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KimiConfig {
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpencodeConfig {
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrokConfig {
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvVarDecl {
    #[serde(rename = "default")]
    pub default_value: Option<String>,

    #[serde(default)]
    pub interactive: bool,

    #[serde(default)]
    pub skippable: bool,

    pub prompt: Option<String>,

    #[serde(default)]
    pub options: Vec<String>,

    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HooksConfig {
    #[serde(default)]
    pub setup_once: Option<String>,

    #[serde(default)]
    pub source: Option<String>,

    #[serde(default)]
    pub preflight: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct HookEntry<'a> {
    pub label: &'static str,

    pub filename: &'static str,

    pub path: &'a str,
}

impl HooksConfig {
    pub fn entries(&self) -> impl Iterator<Item = HookEntry<'_>> {
        [
            (
                "setup_once hook",
                "setup-once.sh",
                self.setup_once.as_deref(),
            ),
            ("source hook", "source.sh", self.source.as_deref()),
            ("preflight hook", "preflight.sh", self.preflight.as_deref()),
        ]
        .into_iter()
        .filter_map(|(label, filename, path)| {
            path.map(|path| HookEntry {
                label,
                filename,
                path,
            })
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfig {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClaudeMarketplaceConfig {
    pub source: String,

    #[serde(default)]
    pub sparse: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaudeConfig {
    #[serde(default)]
    pub model: Option<String>,

    #[serde(default)]
    pub marketplaces: Vec<ClaudeMarketplaceConfig>,

    #[serde(default)]
    pub plugins: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ManifestWarning {
    pub message: String,
}

impl ManifestWarning {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
