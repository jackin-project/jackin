// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::fmt;
use std::num::NonZeroU16;

use jackin_core::{
    Agent, ENGINE_HOST, HOST_GATEWAY_ALIASES, ProfileAsset, ProfileId, ProviderName, SandboxPath,
    plain,
};
use openshell_core::host_pattern;
use openshell_sdk::raw::proto::SandboxPolicy;

use crate::error::{ClosedPolicyError, PolicyError};

mod yaml;

#[derive(Clone, Debug, PartialEq)]
pub struct PolicyDocument {
    value: noyalib::Value,
    proto: SandboxPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HostPattern(String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleKey(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkRule {
    pub key: RuleKey,
    pub endpoints: Vec<RuleEndpoint>,
    pub binaries: Vec<SandboxPath>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleEndpoint {
    pub host: HostPattern,
    pub port: u16,
    pub protocol: EndpointProtocol,
    pub tls: TlsMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EndpointProtocol {
    Tcp,
    Rest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TlsMode {
    Default,
    Skip,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderProfile {
    pub id: ProfileId,
    pub yaml: &'static str,
    pub endpoints: Vec<ProfileEndpoint>,
    pub binaries: Vec<SandboxPath>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileEndpoint {
    pub host: HostPattern,
    pub port: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClosureViolation {
    Overlap {
        rule: RuleKey,
        host: HostPattern,
        provider_host: HostPattern,
    },
    UnparsableHost {
        rule: RuleKey,
        host: String,
    },
    InvalidPort {
        rule: RuleKey,
        port: u32,
    },
}

impl fmt::Display for ClosureViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClosureViolation::Overlap {
                rule,
                host,
                provider_host,
            } => write!(
                f,
                "rule {rule}: {host} overlaps the provider host {provider_host}"
            ),
            ClosureViolation::UnparsableHost { rule, host } => {
                write!(f, "rule {rule}: the host {} cannot be read", plain(host))
            }
            ClosureViolation::InvalidPort { rule, port } => {
                write!(f, "rule {rule}: the port {port} is invalid")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EngineHostViolation {
    pub rule: RuleKey,
    pub aliases: Vec<&'static str>,
}

impl fmt::Display for EngineHostViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.rule, self.aliases.join(", "))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AuthoredPolicy(PolicyDocument);

#[derive(Clone, Debug)]
pub struct ClosedPolicy(PolicyDocument);

impl PolicyDocument {
    pub fn parse(text: &str) -> Result<PolicyDocument, PolicyError> {
        yaml::build(text, yaml::Source::Live)
    }

    pub fn network_rules(&self) -> Vec<NetworkRule> {
        yaml::scan(&self.proto)
            .into_iter()
            .map(|scanned| scanned.rule)
            .collect()
    }

    pub fn closure_violations(
        &self,
        profile: &ProviderProfile,
        generated: &[RuleKey],
    ) -> Vec<ClosureViolation> {
        let found: BTreeSet<ClosureViolation> = yaml::scan(&self.proto)
            .into_iter()
            .filter(|scanned| !generated.contains(&scanned.rule.key))
            .flat_map(|scanned| {
                let overlaps = scanned.rule.endpoints.iter().flat_map(|endpoint| {
                    profile
                        .endpoints
                        .iter()
                        .filter(|provider| endpoint.host.overlaps(&provider.host))
                        .map(|provider| ClosureViolation::Overlap {
                            rule: scanned.rule.key.clone(),
                            host: endpoint.host.clone(),
                            provider_host: provider.host.clone(),
                        })
                });
                overlaps
                    .chain(scanned.unreadable.iter().cloned())
                    .collect::<Vec<_>>()
            })
            .collect();
        found.into_iter().collect()
    }

    pub fn engine_host_violations(&self) -> Vec<EngineHostViolation> {
        yaml::scan(&self.proto)
            .into_iter()
            .filter_map(|scanned| {
                let aliases: Vec<&'static str> = HOST_GATEWAY_ALIASES
                    .into_iter()
                    .filter(|alias| {
                        let alias = HostPattern((*alias).to_owned());
                        scanned
                            .rule
                            .endpoints
                            .iter()
                            .any(|endpoint| endpoint.host.overlaps(&alias))
                    })
                    .collect();
                (!aliases.is_empty()).then_some(EngineHostViolation {
                    rule: scanned.rule.key,
                    aliases,
                })
            })
            .collect()
    }

    pub fn with_rule(&self, rule: NetworkRule) -> Result<PolicyDocument, PolicyError> {
        yaml::with_rule(&self.value, &rule)
    }

    pub fn with_read_only(&self, path: &str) -> Result<PolicyDocument, PolicyError> {
        yaml::with_read_only(&self.value, path)
    }

    pub fn render(&self) -> Result<String, PolicyError> {
        yaml::render(&self.value)
    }

    pub(crate) fn proto(&self) -> &SandboxPolicy {
        &self.proto
    }

    pub(crate) fn from_proto(proto: &SandboxPolicy) -> Result<PolicyDocument, PolicyError> {
        yaml::from_proto(proto)
    }
}

impl AuthoredPolicy {
    pub fn parse(text: &str) -> Result<AuthoredPolicy, PolicyError> {
        yaml::build(text, yaml::Source::Authored).map(AuthoredPolicy)
    }

    pub fn with_rule(&self, rule: NetworkRule) -> Result<AuthoredPolicy, PolicyError> {
        yaml::check_added(&rule)?;
        self.0.with_rule(rule).map(AuthoredPolicy)
    }

    pub fn with_read_only(self, path: &str) -> Result<AuthoredPolicy, PolicyError> {
        self.0.with_read_only(path).map(AuthoredPolicy)
    }

    pub fn document(&self) -> &PolicyDocument {
        &self.0
    }

    pub fn close(self) -> Result<ClosedPolicy, ClosedPolicyError> {
        let tuple = jackin_core::lookup_tuple(Agent::Codex, jackin_core::Provider::Codex)
            .map_err(|_| PolicyError::InvalidHost("codex tuple".to_owned()))?;
        self.close_for_tuple(tuple)
    }

    pub fn close_for_tuple(
        self,
        tuple: &jackin_core::AgentTuple,
    ) -> Result<ClosedPolicy, ClosedPolicyError> {
        let profile = ProviderProfile::parse(jackin_core::tuple_profile_asset(tuple))?;
        let violations = self.0.closure_violations(&profile, &[]);
        if violations.is_empty() {
            Ok(ClosedPolicy(self.0))
        } else {
            Err(ClosedPolicyError::Violations(violations))
        }
    }

    pub fn close_for(
        self,
        agent: Agent,
        provider: jackin_core::Provider,
    ) -> Result<ClosedPolicy, ClosedPolicyError> {
        let tuple = jackin_core::lookup_tuple(agent, provider)
            .map_err(|_| PolicyError::InvalidHost("unknown tuple".to_owned()))?;
        self.close_for_tuple(tuple)
    }

    pub fn close_for_tuples(
        self,
        tuples: &[&jackin_core::AgentTuple],
    ) -> Result<ClosedPolicy, ClosedPolicyError> {
        let mut violations = Vec::new();
        for tuple in tuples {
            let profile = ProviderProfile::parse(jackin_core::tuple_profile_asset(tuple))?;
            violations.extend(self.0.closure_violations(&profile, &[]));
        }
        violations.sort();
        violations.dedup();
        if violations.is_empty() {
            Ok(ClosedPolicy(self.0))
        } else {
            Err(ClosedPolicyError::Violations(violations))
        }
    }

    pub fn close_for_instances(
        self,
        instances: &jackin_core::AgentInstances,
    ) -> Result<ClosedPolicy, ClosedPolicyError> {
        let mut tuples = Vec::new();
        for instance in instances.iter() {
            let tuple = jackin_core::lookup_tuple(instance.agent, instance.provider)
                .map_err(|_| PolicyError::InvalidHost("unknown tuple".to_owned()))?;
            tuples.push(tuple);
        }
        self.close_for_tuples(&tuples)
    }
}

impl ClosedPolicy {
    pub fn parse(text: &str) -> Result<ClosedPolicy, ClosedPolicyError> {
        AuthoredPolicy::parse(text)?.close()
    }

    pub fn document(&self) -> &PolicyDocument {
        &self.0
    }
}

impl ProviderProfile {
    pub fn parse(asset: &'static ProfileAsset) -> Result<ProviderProfile, PolicyError> {
        yaml::parse_profile(asset)
    }

    pub(crate) fn to_proto(
        &self,
    ) -> Result<openshell_sdk::raw::proto::ProviderProfile, PolicyError> {
        yaml::profile_to_proto(self)
    }
}

const DOCKER_ENGINE_RULE: &str = "docker_engine";

impl RuleKey {
    pub fn provider_rule(provider: &ProviderName) -> RuleKey {
        RuleKey(yaml::provider_rule_name(provider.as_str()))
    }

    pub fn is_reserved(&self) -> bool {
        yaml::is_reserved(&self.0)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn new(text: String) -> RuleKey {
        RuleKey(text)
    }
}

impl fmt::Display for RuleKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&plain(&self.0))
    }
}

impl HostPattern {
    pub fn parse(text: &str) -> Result<HostPattern, PolicyError> {
        let invalid = || PolicyError::InvalidHost(text.to_owned());
        if !text.split('.').all(label_in_alphabet) || gateway_refuses_shape(text) {
            return Err(invalid());
        }
        host_pattern::HostPattern::new(text).map_err(|_| invalid())?;
        Ok(HostPattern(text.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn overlaps(&self, other: &HostPattern) -> bool {
        host_pattern::host_patterns_overlap(&self.0, &other.0).unwrap_or(true)
    }
}

fn label_in_alphabet(label: &str) -> bool {
    label == "**"
        || label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'*'))
}

fn gateway_refuses_shape(host: &str) -> bool {
    if host == "*" || host == "**" {
        return true;
    }
    if !host.contains('*') {
        return false;
    }
    let labels: Vec<&str> = host.split('.').collect();
    let tld_wildcard = (host.starts_with("*.") || host.starts_with("**.")) && labels.len() <= 2;
    let first = labels.first().copied().unwrap_or_default();
    let first_invalid = first.contains("**") && first != "**";
    let later_invalid = labels
        .iter()
        .skip(1)
        .any(|label| label.contains("**") || (label.contains('*') && *label != "*"));
    tld_wildcard || first_invalid || later_invalid
}

impl fmt::Display for HostPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl NetworkRule {
    pub fn docker_engine(port: NonZeroU16) -> NetworkRule {
        NetworkRule {
            key: RuleKey(DOCKER_ENGINE_RULE.to_owned()),
            endpoints: vec![RuleEndpoint {
                host: HostPattern(ENGINE_HOST.to_owned()),
                port: port.get(),
                protocol: EndpointProtocol::Tcp,
                tls: TlsMode::Skip,
            }],
            binaries: vec![SandboxPath::from_static("/usr/local/bin/docker")],
        }
    }
}
