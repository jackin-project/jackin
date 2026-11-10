// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, AgentTuple, Provider};
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
#[serde(rename_all = "snake_case")]
pub enum ToolObservation {
    DirectoryContains { entries: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBehavior {
    pub argv: Vec<String>,
    pub answer: AnswerMatcher,
    pub kind: String,
    pub workdir: String,
    pub observation: ToolObservation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptBehavior {
    pub argv: Vec<String>,
    pub answer: AnswerMatcher,
    pub tool: ToolBehavior,
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

impl CredentialClassification {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OauthLogin => "oauth_login",
            Self::StaticKey => "static_key",
            Self::ApiKey => "api_key",
            Self::None => "none",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentKind {
    Placeholder,
    Empty,
    Brokered,
}

impl EnvironmentKind {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Placeholder => "placeholder",
            Self::Empty => "empty",
            Self::Brokered => "brokered",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentBehavior {
    pub name: String,
    pub kind: EnvironmentKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthFieldKind {
    Placeholder,
    Empty,
    Absent,
    Literal { value: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthFieldBehavior {
    pub name: String,
    pub kind: AuthFieldKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthFileBehavior {
    pub required_state: String,
    pub path: String,
    pub mode: Option<String>,
    pub fields: Vec<AuthFieldBehavior>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigFileBehavior {
    pub path: String,
    pub required_state: String,
    pub mode: Option<String>,
    pub required_text: Vec<String>,
    pub forbidden_text: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialBehavior {
    pub classification: CredentialClassification,
    pub environment: Vec<EnvironmentBehavior>,
    pub absent_environment: Vec<String>,
    pub auth_file: AuthFileBehavior,
    pub config_files: Vec<ConfigFileBehavior>,
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
    pub accepted_status: Option<u16>,
    pub accepted_body: Option<ProviderBodyMatcher>,
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

fn answer() -> AnswerMatcher {
    AnswerMatcher::WordCaseInsensitive {
        word: "ready".to_owned(),
    }
}

fn project_listing() -> ToolObservation {
    ToolObservation::DirectoryContains {
        entries: [
            "AGENTS.md",
            "Dockerfile",
            "instructions",
            "jackin.role.toml",
            "probe",
            "tests",
        ]
        .map(str::to_owned)
        .to_vec(),
    }
}

fn codex_prompt(binary: &str, provider: Provider, model: &str) -> PromptBehavior {
    let provider = provider.as_str();
    let base = vec![
        binary.to_owned(),
        "exec".to_owned(),
        "--json".to_owned(),
        "--skip-git-repo-check".to_owned(),
        "--sandbox".to_owned(),
        "danger-full-access".to_owned(),
        "-c".to_owned(),
        "check_for_update_on_startup=false".to_owned(),
    ];
    let mut prompt = base.clone();
    prompt.push("Reply with exactly ready".to_owned());
    let mut tool = base;
    tool.push(format!(
        "Use one shell tool to run ls -A in /sandbox/project with provider {provider} and model {model}, then reply with exactly ready"
    ));
    PromptBehavior {
        argv: prompt,
        answer: answer(),
        tool: ToolBehavior {
            argv: tool,
            answer: answer(),
            kind: "shell".to_owned(),
            workdir: "/sandbox/project".to_owned(),
            observation: project_listing(),
        },
    }
}

fn omp_prompt(tuple: &AgentTuple) -> PromptBehavior {
    let binary = tuple.agent.spec().binary.to_string();
    let base = vec![
        binary,
        "-p".to_owned(),
        "--mode".to_owned(),
        "json".to_owned(),
        "--no-session".to_owned(),
        "--model".to_owned(),
        format!("zai/{}", tuple.default_model),
    ];
    let mut prompt = base.clone();
    prompt.push("Reply with exactly ready".to_owned());
    let mut tool = base;
    tool.push(
        "Use the bash tool once with command ls -A in /sandbox/project, then reply with exactly ready"
            .to_owned(),
    );
    PromptBehavior {
        argv: prompt,
        answer: answer(),
        tool: ToolBehavior {
            argv: tool,
            answer: answer(),
            kind: "bash".to_owned(),
            workdir: "/sandbox/project".to_owned(),
            observation: project_listing(),
        },
    }
}

fn claude_prompt(tuple: &AgentTuple) -> PromptBehavior {
    let binary = tuple.agent.spec().binary.to_string();
    let base = vec![
        binary,
        "-p".to_owned(),
        "--output-format".to_owned(),
        "stream-json".to_owned(),
        "--verbose".to_owned(),
        "--model".to_owned(),
        tuple.default_model.to_owned(),
        "--allowedTools".to_owned(),
        "Bash".to_owned(),
    ];
    let mut prompt = base.clone();
    prompt.push(format!(
        "Reply with exactly ready using model {}",
        tuple.default_model
    ));
    let mut tool = base;
    tool.push(format!(
        "Use the Bash tool once with command ls -A in /sandbox/project using model {}, then reply with exactly ready",
        tuple.default_model
    ));
    PromptBehavior {
        argv: prompt,
        answer: answer(),
        tool: ToolBehavior {
            argv: tool,
            answer: answer(),
            kind: "Bash".to_owned(),
            workdir: "/sandbox/project".to_owned(),
            observation: project_listing(),
        },
    }
}

fn auth_field(name: &str, kind: AuthFieldKind) -> AuthFieldBehavior {
    AuthFieldBehavior {
        name: name.to_owned(),
        kind,
    }
}

fn config(path: &str, required: &[&str], forbidden: &[&str]) -> ConfigFileBehavior {
    ConfigFileBehavior {
        path: path.to_owned(),
        required_state: "present".to_owned(),
        mode: Some("600".to_owned()),
        required_text: required.iter().map(|text| (*text).to_owned()).collect(),
        forbidden_text: forbidden.iter().map(|text| (*text).to_owned()).collect(),
    }
}

fn zai_usage() -> Option<UsageBehavior> {
    Some(UsageBehavior::Windows {
        required: vec![UsageWindow::FiveHour, UsageWindow::Weekly],
        optional: vec![UsageWindow::Monthly],
    })
}

fn zai_provider(url: &str) -> Option<ProviderBehavior> {
    Some(ProviderBehavior {
        probe_url: url.to_owned(),
        accepted_status: None,
        accepted_body: None,
        policy_hosts: vec!["api.z.ai".to_owned(), "*.api.z.ai".to_owned()],
    })
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
        prompt: Some(codex_prompt(
            agent.binary.as_str(),
            Provider::Codex,
            agent.default_model,
        )),
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
                .map(|name| EnvironmentBehavior {
                    name: (*name).to_owned(),
                    kind: EnvironmentKind::Placeholder,
                })
                .collect(),
            absent_environment: vec![],
            auth_file: AuthFileBehavior {
                required_state: "present".to_owned(),
                path: agent.auth_file.path.to_string(),
                mode: Some("600".to_owned()),
                fields: vec![
                    auth_field("/tokens/access_token", AuthFieldKind::Placeholder),
                    auth_field("/tokens/refresh_token", AuthFieldKind::Empty),
                    auth_field("/tokens/id_token", AuthFieldKind::Placeholder),
                ],
            },
            config_files: vec![],
        }),
        reflection: Some(ReflectionBehavior {
            url: "https://chatgpt.com/backend-api/e2e-reflect".to_owned(),
            descendant_comm: "codex".to_owned(),
        }),
        provider: Some(ProviderBehavior {
            probe_url: "https://chatgpt.com/backend-api/models".to_owned(),
            accepted_status: Some(401),
            accepted_body: Some(ProviderBodyMatcher::JsonErrorCode {
                code: "invalid_api_key".to_owned(),
            }),
            policy_hosts: vec!["chatgpt.com".to_owned(), "*.chatgpt.com".to_owned()],
        }),
        usage: Some(UsageBehavior::Windows {
            required: vec![UsageWindow::Weekly, UsageWindow::Credits],
            optional: vec![],
        }),
    }
}

#[must_use]
pub fn t09_behavior() -> AgentBehaviorSpec {
    let Ok(tuple) = jackin_core::lookup_tuple(Agent::Omp, Provider::Zai) else {
        return AgentBehaviorSpec::default();
    };
    let agent = tuple.agent.spec();
    let Some(url) = tuple.omp_base_url() else {
        return AgentBehaviorSpec::default();
    };
    AgentBehaviorSpec {
        version: Some(VersionBehavior {
            binary: agent.binary.to_string(),
            exact_output: format!("omp/{}", agent.install.version),
        }),
        process: Some(ProcessBehavior {
            comm: "omp".to_owned(),
            minimum_age_seconds: 15,
        }),
        prompt: Some(omp_prompt(tuple)),
        terminal: Some(TerminalBehavior {
            ready_text: format!("omp {}", agent.install.version),
            interrupt: 3,
            quit_interrupts: 2,
            exit_notice: "omp exited with code".to_owned(),
        }),
        credential: Some(CredentialBehavior {
            classification: CredentialClassification::StaticKey,
            environment: vec![EnvironmentBehavior {
                name: "ZAI_API_KEY".to_owned(),
                kind: EnvironmentKind::Brokered,
            }],
            absent_environment: vec![
                "ANTHROPIC_AUTH_TOKEN".to_owned(),
                "ANTHROPIC_API_KEY".to_owned(),
                "OPENAI_API_KEY".to_owned(),
            ],
            auth_file: AuthFileBehavior {
                required_state: "present".to_owned(),
                path: agent.auth_file.path.to_string(),
                mode: Some("600".to_owned()),
                fields: vec![
                    auth_field(
                        "/zai/type",
                        AuthFieldKind::Literal {
                            value: "api_key".to_owned(),
                        },
                    ),
                    auth_field("/zai/key", AuthFieldKind::Placeholder),
                ],
            },
            config_files: vec![
                config(
                    "/sandbox/.omp/config.toml",
                    &[
                        "baseUrl: https://api.z.ai/api/anthropic",
                        "credentialEnv: ZAI_API_KEY",
                        "default: zai/glm-5.3",
                    ],
                    &["https://api.z.ai/api/coding/paas/v4"],
                ),
                config(
                    "/sandbox/.omp/agent/models.yml",
                    &[
                        "baseUrl: https://api.z.ai/api/anthropic",
                        "api: anthropic-messages",
                        "apiKey: ZAI_API_KEY",
                    ],
                    &["https://api.z.ai/api/coding/paas/v4"],
                ),
            ],
        }),
        reflection: Some(ReflectionBehavior {
            url: url.to_owned(),
            descendant_comm: "omp".to_owned(),
        }),
        provider: zai_provider(url),
        usage: zai_usage(),
    }
}

#[must_use]
pub fn t14_behavior() -> AgentBehaviorSpec {
    let Ok(tuple) = jackin_core::lookup_tuple(Agent::ClaudeCode, Provider::Zai) else {
        return AgentBehaviorSpec::default();
    };
    let agent = tuple.agent.spec();
    let Some(url) = tuple.claude_base_url() else {
        return AgentBehaviorSpec::default();
    };
    AgentBehaviorSpec {
        version: Some(VersionBehavior {
            binary: agent.binary.to_string(),
            exact_output: format!("{} (Claude Code)", agent.install.version),
        }),
        process: Some(ProcessBehavior {
            comm: "claude".to_owned(),
            minimum_age_seconds: 15,
        }),
        prompt: Some(claude_prompt(tuple)),
        terminal: Some(TerminalBehavior {
            ready_text: "Claude Code".to_owned(),
            interrupt: 3,
            quit_interrupts: 2,
            exit_notice: "claude exited with code".to_owned(),
        }),
        credential: Some(CredentialBehavior {
            classification: CredentialClassification::StaticKey,
            environment: vec![EnvironmentBehavior {
                name: "ANTHROPIC_AUTH_TOKEN".to_owned(),
                kind: EnvironmentKind::Brokered,
            }],
            absent_environment: vec![
                "ZAI_API_KEY".to_owned(),
                "ANTHROPIC_API_KEY".to_owned(),
                "OPENAI_API_KEY".to_owned(),
            ],
            auth_file: AuthFileBehavior {
                required_state: "present".to_owned(),
                path: agent.auth_file.path.to_string(),
                mode: Some("600".to_owned()),
                fields: vec![auth_field("/ANTHROPIC_API_KEY", AuthFieldKind::Placeholder)],
            },
            config_files: vec![config(
                "/sandbox/.claude/settings.json",
                &["{\"env\":{\"ANTHROPIC_BASE_URL\":\"https://api.z.ai/api/anthropic\"}}"],
                &["ANTHROPIC_AUTH_TOKEN", "ZAI_API_KEY"],
            )],
        }),
        reflection: Some(ReflectionBehavior {
            url: url.to_owned(),
            descendant_comm: "claude".to_owned(),
        }),
        provider: zai_provider(url),
        usage: zai_usage(),
    }
}

#[must_use]
pub fn t18_behavior() -> AgentBehaviorSpec {
    let Ok(tuple) = jackin_core::lookup_tuple(Agent::Codex, Provider::Zai) else {
        return AgentBehaviorSpec::default();
    };
    let agent = tuple.agent.spec();
    let Some(url) = tuple.codex_base_url() else {
        return AgentBehaviorSpec::default();
    };
    AgentBehaviorSpec {
        version: Some(VersionBehavior {
            binary: agent.binary.to_string(),
            exact_output: format!("codex-cli {}", agent.install.version),
        }),
        process: Some(ProcessBehavior {
            comm: "codex".to_owned(),
            minimum_age_seconds: 15,
        }),
        prompt: Some(codex_prompt(
            agent.binary.as_str(),
            Provider::Zai,
            tuple.default_model,
        )),
        terminal: Some(TerminalBehavior {
            ready_text: format!("OpenAI Codex (v{})", agent.install.version),
            interrupt: 3,
            quit_interrupts: 2,
            exit_notice: "codex exited with code".to_owned(),
        }),
        credential: Some(CredentialBehavior {
            classification: CredentialClassification::StaticKey,
            environment: vec![EnvironmentBehavior {
                name: "ZAI_API_KEY".to_owned(),
                kind: EnvironmentKind::Brokered,
            }],
            absent_environment: vec![
                "ANTHROPIC_AUTH_TOKEN".to_owned(),
                "ANTHROPIC_API_KEY".to_owned(),
                "OPENAI_API_KEY".to_owned(),
            ],
            auth_file: AuthFileBehavior {
                required_state: "absent".to_owned(),
                path: agent.auth_file.path.to_string(),
                mode: None,
                fields: vec![],
            },
            config_files: vec![config(
                "/sandbox/.codex/config.toml",
                &[
                    "model = \"glm-5.3\"",
                    "model_provider = \"zai\"",
                    "name = \"Z.AI\"",
                    "base_url = \"https://api.z.ai/api/v1\"",
                    "env_key = \"ZAI_API_KEY\"",
                    "wire_api = \"responses\"",
                ],
                &["ANTHROPIC_AUTH_TOKEN"],
            )],
        }),
        reflection: Some(ReflectionBehavior {
            url: url.to_owned(),
            descendant_comm: "codex".to_owned(),
        }),
        provider: zai_provider(url),
        usage: zai_usage(),
    }
}

pub(crate) fn specify(tuple: &AgentTuple) -> Option<AgentBehaviorSpec> {
    match tuple.id {
        "T01" => Some(t01_behavior()),
        "T09" => Some(t09_behavior()),
        "T14" => Some(t14_behavior()),
        "T18" => Some(t18_behavior()),
        _ => None,
    }
}

pub(crate) fn adapter_id(tuple: &AgentTuple) -> Option<&'static str> {
    match tuple.id {
        "T01" => Some("t01-codex-v1"),
        "T09" => Some("t09-omp-zai-v1"),
        "T14" => Some("t14-claude-zai-v1"),
        "T18" => Some("t18-codex-zai-v1"),
        _ => None,
    }
}
