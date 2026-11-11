// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use jackin_core::{
    AccountName, Agent, AgentInstance, AgentInstances, AgentTuple, AuthClass, CatalogError,
    EnvName, ModelName, OpReference, Provider, ProviderName, TupleState, lookup_tuple, plain,
    tuple_profile_asset,
};
use jackin_openshell::{PolicyError, ProviderProfile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::adapter::{
    AgentBehaviorSpec, CompleteBehaviorSpec, CredentialClassification, adapter_id, specify,
};
use super::args::E2eArgs;
use super::redact::hex;

pub const PROBE_SPEC_PATH: &str = "/sandbox/.jackin/e2e/probe-spec.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorCategory {
    Version,
    Process,
    Prompt,
    Terminal,
    Credential,
    Reflection,
    Provider,
    Usage,
}

impl std::fmt::Display for BehaviorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Version => "version",
                Self::Process => "process",
                Self::Prompt => "prompt",
                Self::Terminal => "terminal",
                Self::Credential => "credential",
                Self::Reflection => "reflection",
                Self::Provider => "provider",
                Self::Usage => "usage",
            }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum E2eCredential {
    CodexLogin { home: PathBuf },
    HostLogin { home: PathBuf },
    OmpHost,
    Env { name: EnvName },
    OnePassword { reference: String },
    File { path: PathBuf },
}

impl E2eCredential {
    pub fn parse(text: &str) -> Result<E2eCredential, TargetError> {
        const MAX_ENV_NAME: usize = 128;
        if let Some(name) = text.strip_prefix("env:") {
            if name.len() > MAX_ENV_NAME {
                return Err(TargetError::CredentialDescriptor);
            }
            return EnvName::parse(name)
                .map(|name| Self::Env { name })
                .map_err(|_| TargetError::CredentialDescriptor);
        }
        if text == "omp-host" {
            return Ok(Self::OmpHost);
        }
        if text.starts_with("op://") {
            return OpReference::parse(text)
                .map(|reference| Self::OnePassword {
                    reference: reference.to_string(),
                })
                .map_err(|_| TargetError::CredentialDescriptor);
        }
        match text.strip_prefix("file:") {
            Some(path) if !path.is_empty() => Ok(Self::File {
                path: PathBuf::from(path),
            }),
            _ => Err(TargetError::CredentialDescriptor),
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::CodexLogin { .. } => "codex-login",
            Self::HostLogin { .. } => "host-login",
            Self::OmpHost => "omp-host",
            Self::Env { .. } => "env",
            Self::OnePassword { .. } => "1password",
            Self::File { .. } => "file",
        }
    }
}

#[derive(Debug, Error)]
pub enum TargetError {
    #[error("unknown agent or provider name `{}`", plain(.0))]
    SelectionName(String),
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error("tuple {} is unavailable: {}", plain(.id), plain(.reason))]
    TupleUnavailable { id: String, reason: String },
    #[error("tuple {} has no complete E2E adapter", plain(.id))]
    AdapterUnavailable { id: String },
    #[error("tuple {} has no {category} behavior evidence", plain(.id))]
    IncompleteBehavior {
        id: String,
        category: BehaviorCategory,
    },
    #[error("credential source {} is not valid for tuple {}", plain(.kind), plain(.id))]
    CredentialSource { id: String, kind: String },
    #[error("credential classification does not match tuple {}", plain(.0))]
    CredentialClass(String),
    #[error("--credential expects env:NAME, an op:// reference, file:PATH, or omp-host")]
    CredentialDescriptor,
    #[error("T01 needs --codex-home or HOME for its ChatGPT login")]
    CodexHome,
    #[error(transparent)]
    Model(#[from] jackin_core::IdError),
    #[error("cannot parse the selected provider profile")]
    Profile(#[from] PolicyError),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error("{kind} URL {} is outside the selected provider profile hosts", plain(.url))]
    OutsideProfile { kind: &'static str, url: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetIdentity {
    pub tuple: String,
    pub agent: String,
    pub provider: String,
    pub model: String,
    pub adapter: String,
    pub spec_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct CanonicalSpec<'a> {
    tuple: &'a str,
    agent: &'a str,
    provider: &'a str,
    model: &'a str,
    adapter: &'a str,
    behavior: &'a CompleteBehaviorSpec,
    provider_hosts: &'a [String],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeSpec {
    pub target: TargetIdentity,
    pub behavior: CompleteBehaviorSpec,
    pub provider_hosts: Vec<String>,
    pub credential_sha256: Option<String>,
}

#[derive(Clone, Debug)]
pub struct E2eTarget {
    tuple: &'static AgentTuple,
    model: ModelName,
    credential: E2eCredential,
    behavior: CompleteBehaviorSpec,
    provider_hosts: Vec<String>,
    identity: TargetIdentity,
    discovery: bool,
}

impl E2eTarget {
    pub fn t01() -> Result<E2eTarget, TargetError> {
        Self::resolve(&E2eArgs::default(), Some(Path::new("/tmp")))
    }

    pub fn resolve(args: &E2eArgs, home: Option<&Path>) -> Result<E2eTarget, TargetError> {
        let agent = args.agent.as_deref().map(parse_agent).transpose()?;
        let provider = args.provider.as_deref().map(parse_provider).transpose()?;
        let agent = agent.unwrap_or(Agent::Codex);
        let provider = provider.unwrap_or_else(|| agent.provider());
        let behavior = specify(lookup_tuple(agent, provider)?);
        Self::resolve_with_behavior(args, home, behavior)
    }

    pub fn resolve_with_behavior(
        args: &E2eArgs,
        home: Option<&Path>,
        behavior: Option<AgentBehaviorSpec>,
    ) -> Result<E2eTarget, TargetError> {
        Self::resolve_with_behavior_and_hosts(args, home, behavior, None)
    }

    pub fn resolve_with_behavior_and_hosts(
        args: &E2eArgs,
        home: Option<&Path>,
        behavior: Option<AgentBehaviorSpec>,
        parsed_profile_hosts: Option<Vec<String>>,
    ) -> Result<E2eTarget, TargetError> {
        let agent = args.agent.as_deref().map(parse_agent).transpose()?;
        let provider = args.provider.as_deref().map(parse_provider).transpose()?;
        let agent = agent.unwrap_or(Agent::Codex);
        let provider = provider.unwrap_or_else(|| agent.provider());
        let tuple = lookup_tuple(agent, provider)?;
        if tuple.state != TupleState::Supported {
            return Err(TargetError::TupleUnavailable {
                id: tuple.id.to_owned(),
                reason: tuple.reason.unwrap_or("not supported").to_owned(),
            });
        }
        let model = ModelName::parse(args.model.as_deref().unwrap_or(tuple.default_model))?;
        tuple.check_model(model.as_str())?;
        let adapter = adapter_id(tuple).ok_or_else(|| TargetError::AdapterUnavailable {
            id: tuple.id.to_owned(),
        })?;
        let behavior = behavior.ok_or_else(|| TargetError::AdapterUnavailable {
            id: tuple.id.to_owned(),
        })?;
        let behavior = complete(tuple, behavior)?;
        let expected_class = match tuple.auth_class {
            AuthClass::OAuth => CredentialClassification::OauthLogin,
            AuthClass::StaticKey => CredentialClassification::StaticKey,
            AuthClass::ApiKey => CredentialClassification::ApiKey,
            AuthClass::BlockedVendor => CredentialClassification::None,
        };
        if behavior.credential.classification != expected_class {
            return Err(TargetError::CredentialClass(tuple.id.to_owned()));
        }
        let profile = ProviderProfile::parse(tuple_profile_asset(tuple))?;
        let provider_hosts = parsed_profile_hosts.unwrap_or_else(|| {
            profile
                .endpoints
                .iter()
                .map(|endpoint| endpoint.host.as_str().to_owned())
                .collect()
        });
        for (kind, url) in [
            ("provider", behavior.provider.probe_url.as_str()),
            ("reflection", behavior.reflection.url.as_str()),
        ] {
            let host = url_host(url).ok_or_else(|| TargetError::OutsideProfile {
                kind,
                url: url.to_owned(),
            })?;
            if !provider_hosts.iter().any(|allowed| allowed == host) {
                return Err(TargetError::OutsideProfile {
                    kind,
                    url: url.to_owned(),
                });
            }
        }
        let credential = credential_for(args, home, tuple)?;
        let mut target = E2eTarget {
            tuple,
            credential,
            behavior,
            provider_hosts,
            identity: TargetIdentity {
                tuple: tuple.id.to_owned(),
                agent: agent.as_str().to_owned(),
                provider: provider.as_str().to_owned(),
                model: model.to_string(),
                adapter: adapter.to_owned(),
                spec_sha256: String::new(),
            },
            model,
            discovery: args.credential.is_none() && tuple.id != "T01",
        };
        target.identity.spec_sha256 = target.digest()?;
        Ok(target)
    }

    #[must_use]
    pub fn tuple(&self) -> &'static AgentTuple {
        self.tuple
    }

    #[must_use]
    pub fn model(&self) -> &ModelName {
        &self.model
    }

    #[must_use]
    pub fn credential(&self) -> &E2eCredential {
        &self.credential
    }

    #[must_use]
    pub fn discovers_credential(&self) -> bool {
        self.discovery
    }

    #[cfg(test)]
    pub(crate) fn set_credential_for_test(&mut self, credential: E2eCredential) {
        self.credential = credential;
    }

    #[must_use]
    pub fn behavior(&self) -> &CompleteBehaviorSpec {
        &self.behavior
    }

    #[must_use]
    pub fn identity(&self) -> &TargetIdentity {
        &self.identity
    }

    #[must_use]
    pub fn provider_hosts(&self) -> &[String] {
        &self.provider_hosts
    }

    fn digest(&self) -> Result<String, serde_json::Error> {
        let canonical = CanonicalSpec {
            tuple: &self.identity.tuple,
            agent: &self.identity.agent,
            provider: &self.identity.provider,
            model: &self.identity.model,
            adapter: &self.identity.adapter,
            behavior: &self.behavior,
            provider_hosts: &self.provider_hosts,
        };
        let bytes = serde_json::to_vec(&canonical)?;
        Ok(hex(&Sha256::digest(bytes)))
    }

    #[must_use]
    pub fn probe_spec(&self) -> ProbeSpec {
        ProbeSpec {
            target: self.identity.clone(),
            behavior: self.behavior.clone(),
            provider_hosts: self.provider_hosts.clone(),
            credential_sha256: None,
        }
    }

    pub fn probe_spec_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut bytes = serde_json::to_vec_pretty(&self.probe_spec())?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    #[must_use]
    pub fn selection_args(&self) -> Vec<String> {
        vec![
            "--agent".to_owned(),
            self.tuple.agent.as_str().to_owned(),
            "--provider".to_owned(),
            self.tuple.provider.as_str().to_owned(),
            "--model".to_owned(),
            self.model.to_string(),
        ]
    }

    pub fn instances(
        &self,
        account: &AccountName,
    ) -> Result<AgentInstances, jackin_core::AgentInstanceError> {
        AgentInstances::new(vec![AgentInstance::new(
            self.tuple.agent,
            self.tuple.provider,
            account.clone(),
            self.model.to_string(),
        )])
    }

    #[must_use]
    pub fn provider_name(&self, account: &AccountName) -> ProviderName {
        ProviderName::for_account(account, self.tuple.agent)
    }
}

fn complete(
    tuple: &'static AgentTuple,
    raw: AgentBehaviorSpec,
) -> Result<CompleteBehaviorSpec, TargetError> {
    let missing = |category| TargetError::IncompleteBehavior {
        id: tuple.id.to_owned(),
        category,
    };
    Ok(CompleteBehaviorSpec {
        version: raw
            .version
            .ok_or_else(|| missing(BehaviorCategory::Version))?,
        process: raw
            .process
            .ok_or_else(|| missing(BehaviorCategory::Process))?,
        prompt: raw
            .prompt
            .ok_or_else(|| missing(BehaviorCategory::Prompt))?,
        terminal: raw
            .terminal
            .ok_or_else(|| missing(BehaviorCategory::Terminal))?,
        credential: raw
            .credential
            .ok_or_else(|| missing(BehaviorCategory::Credential))?,
        reflection: raw
            .reflection
            .ok_or_else(|| missing(BehaviorCategory::Reflection))?,
        provider: raw
            .provider
            .ok_or_else(|| missing(BehaviorCategory::Provider))?,
        usage: raw.usage.ok_or_else(|| missing(BehaviorCategory::Usage))?,
    })
}

fn credential_for(
    args: &E2eArgs,
    home: Option<&Path>,
    tuple: &'static AgentTuple,
) -> Result<E2eCredential, TargetError> {
    if tuple.id == "T01" {
        if let Some(source) = &args.credential {
            let parsed = E2eCredential::parse(source)?;
            return Err(TargetError::CredentialSource {
                id: tuple.id.to_owned(),
                kind: parsed.label().to_owned(),
            });
        }
        let home = args
            .codex_home
            .clone()
            .or_else(|| home.map(|path| path.join(".codex")))
            .ok_or(TargetError::CodexHome)?;
        return Ok(E2eCredential::CodexLogin { home });
    }
    if let Some(source) = &args.credential {
        let parsed = E2eCredential::parse(source)?;
        let valid = matches!(
            parsed,
            E2eCredential::Env { .. }
                | E2eCredential::OnePassword { .. }
                | E2eCredential::File { .. }
        ) || tuple.id == "T09" && matches!(parsed, E2eCredential::OmpHost);
        if !valid {
            return Err(TargetError::CredentialSource {
                id: tuple.id.to_owned(),
                kind: parsed.label().to_owned(),
            });
        }
        return Ok(parsed);
    }
    if tuple.id == "T09" {
        return Ok(E2eCredential::OmpHost);
    }
    Ok(E2eCredential::Env {
        name: EnvName::parse("ZAI_API_KEY")?,
    })
}

fn parse_agent(text: &str) -> Result<Agent, TargetError> {
    Agent::ALL
        .into_iter()
        .find(|agent| agent.as_str() == text)
        .ok_or_else(|| TargetError::SelectionName(text.to_owned()))
}

fn parse_provider(text: &str) -> Result<Provider, TargetError> {
    Provider::ALL
        .into_iter()
        .find(|provider| provider.as_str() == text)
        .ok_or_else(|| TargetError::SelectionName(text.to_owned()))
}

fn url_host(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    let authority = rest.split('/').next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    authority.split(':').next().filter(|host| !host.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_credential_and_runtime_value_is_outside_the_canonical_digest() {
        let target = E2eTarget::t01().expect("T01 target");
        let expected = target.identity().spec_sha256.clone();
        let variants = [
            E2eCredential::CodexLogin {
                home: PathBuf::from("/other/codex"),
            },
            E2eCredential::HostLogin {
                home: PathBuf::from("/other/oauth"),
            },
            E2eCredential::OmpHost,
            E2eCredential::Env {
                name: EnvName::parse("ZAI_API_KEY").expect("env name"),
            },
            E2eCredential::OnePassword {
                reference: "op://vault/other-secret-reference".to_owned(),
            },
            E2eCredential::File {
                path: PathBuf::from("/other/secret-file"),
            },
        ];
        for credential in variants {
            let mut changed = target.clone();
            changed.credential = credential;
            assert_eq!(changed.digest().expect("digest"), expected);
        }
        for account in ["a1", "fxr", "fxw", "actual-usage-value"] {
            let name = AccountName::parse(account).expect("account");
            assert_ne!(
                target.instances(&name).expect("instances").agents_label(),
                ""
            );
            assert_eq!(target.identity().spec_sha256, expected);
        }
        let serialized = String::from_utf8(target.probe_spec_bytes().expect("spec")).expect("text");
        for excluded in [
            "actual-secret-value",
            "op://vault/other-secret-reference",
            "actual-usage-value",
            "/other/secret-file",
        ] {
            assert!(!serialized.contains(excluded));
        }
    }

    #[test]
    fn every_target_identity_input_changes_the_digest() {
        let target = E2eTarget::t01().expect("T01 target");
        let expected = target.identity().spec_sha256.clone();
        for field in ["tuple", "agent", "provider", "model", "adapter"] {
            let mut changed = target.clone();
            match field {
                "tuple" => changed.identity.tuple = "other".to_owned(),
                "agent" => changed.identity.agent = "other".to_owned(),
                "provider" => changed.identity.provider = "other".to_owned(),
                "model" => changed.identity.model = "other".to_owned(),
                "adapter" => changed.identity.adapter = "other".to_owned(),
                _ => unreachable!(),
            }
            assert_ne!(changed.digest().expect("digest"), expected, "{field}");
        }
    }
}
