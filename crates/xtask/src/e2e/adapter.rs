// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Agent;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionBehavior {
    pub binary: String,
    pub exact_output: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessBehavior {
    pub comm: String,
    pub minimum_age_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerMatcher {
    WordCaseInsensitive { word: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptBehavior {
    pub argv: Vec<String>,
    pub answer: AnswerMatcher,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalBehavior {
    pub ready_text: String,
    pub interrupt: u8,
    pub quit_interrupts: u8,
    pub exit_notice: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialClassification {
    OauthLogin,
    StaticKey,
    ApiKey,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthFileBehavior {
    pub required_state: String,
    pub path: String,
    pub mode: String,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialBehavior {
    pub classification: CredentialClassification,
    pub environment: Vec<String>,
    pub auth_file: AuthFileBehavior,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReflectionBehavior {
    pub url: String,
    pub descendant_comm: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderBodyMatcher {
    JsonErrorCode { code: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderBehavior {
    pub probe_url: String,
    pub accepted_status: u16,
    pub accepted_body: ProviderBodyMatcher,
    pub policy_hosts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageWindow {
    FiveHour,
    Weekly,
    Monthly,
    Credits,
    Spend,
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageBehavior {
    Windows {
        required: Vec<UsageWindow>,
        optional: Vec<UsageWindow>,
    },
    Unsupported {
        reason: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentBehaviorSpec {
    pub version: Option<VersionBehavior>,
    pub process: Option<ProcessBehavior>,
    pub prompt: Option<PromptBehavior>,
    pub terminal: Option<TerminalBehavior>,
    pub credential: Option<CredentialBehavior>,
    pub reflection: Option<ReflectionBehavior>,
    pub provider: Option<ProviderBehavior>,
    pub usage: Option<UsageBehavior>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteBehaviorSpec {
    pub version: VersionBehavior,
    pub process: ProcessBehavior,
    pub prompt: PromptBehavior,
    pub terminal: TerminalBehavior,
    pub credential: CredentialBehavior,
    pub reflection: ReflectionBehavior,
    pub provider: ProviderBehavior,
    pub usage: UsageBehavior,
}

#[must_use]
pub fn t01_behavior() -> AgentBehaviorSpec {
    let agent = Agent::Codex.spec();
    AgentBehaviorSpec {
        version: Some(VersionBehavior {
            binary: agent.binary.to_string(),
            exact_output: format!("codex-cli {}", agent.install.version),
        }),
        process: Some(ProcessBehavior {
            comm: "codex".to_owned(),
            minimum_age_seconds: 15,
        }),
        prompt: Some(PromptBehavior {
            argv: vec![
                agent.binary.to_string(),
                "exec".to_owned(),
                "--skip-git-repo-check".to_owned(),
                "Reply with the word ready".to_owned(),
            ],
            answer: AnswerMatcher::WordCaseInsensitive {
                word: "ready".to_owned(),
            },
        }),
        terminal: Some(TerminalBehavior {
            ready_text: format!("OpenAI Codex (v{})", agent.install.version),
            interrupt: 3,
            quit_interrupts: 2,
            exit_notice: "codex exited with code".to_owned(),
        }),
        credential: Some(CredentialBehavior {
            classification: CredentialClassification::OauthLogin,
            environment: agent
                .credential_env_names
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
            auth_file: AuthFileBehavior {
                required_state: "present".to_owned(),
                path: agent.auth_file.path.to_string(),
                mode: "600".to_owned(),
                fields: vec![
                    "access_token".to_owned(),
                    "refresh_token".to_owned(),
                    "id_token".to_owned(),
                ],
            },
        }),
        reflection: Some(ReflectionBehavior {
            url: "https://chatgpt.com/backend-api/e2e-reflect".to_owned(),
            descendant_comm: "codex".to_owned(),
        }),
        provider: Some(ProviderBehavior {
            probe_url: "https://chatgpt.com/backend-api/models".to_owned(),
            accepted_status: 401,
            accepted_body: ProviderBodyMatcher::JsonErrorCode {
                code: "invalid_api_key".to_owned(),
            },
            policy_hosts: vec!["chatgpt.com".to_owned(), "*.chatgpt.com".to_owned()],
        }),
        usage: Some(UsageBehavior::Windows {
            required: vec![UsageWindow::Weekly, UsageWindow::Credits],
            optional: vec![],
        }),
    }
}

pub(crate) fn specify(agent: Agent) -> Option<AgentBehaviorSpec> {
    match agent {
        Agent::Codex => Some(t01_behavior()),
        Agent::ClaudeCode
        | Agent::GrokBuild
        | Agent::MuseCode
        | Agent::Antigravity
        | Agent::MinimaxCode
        | Agent::MimoCode
        | Agent::KimiCode
        | Agent::Omp => None,
    }
}
