// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::fmt;
use std::num::NonZeroU16;

use jackin_core::{ProfileAsset, ProfileId, ProviderName, SandboxPath};
use openshell_sdk::raw::proto::SandboxPolicy;

use crate::error::PolicyError;

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
pub struct ClosureViolation {
    pub rule: RuleKey,
    pub host: HostPattern,
    pub provider_host: HostPattern,
}

impl PolicyDocument {
    pub fn parse(text: &str) -> Result<PolicyDocument, PolicyError> {
        yaml::build(text, yaml::Source::Live)
    }

    pub fn parse_authored(text: &str) -> Result<PolicyDocument, PolicyError> {
        yaml::build(text, yaml::Source::Authored)
    }

    pub fn network_rules(&self) -> Vec<NetworkRule> {
        yaml::network_rules(&self.proto)
    }

    pub fn closure_violations(
        &self,
        profile: &ProviderProfile,
        generated: &[RuleKey],
    ) -> Vec<ClosureViolation> {
        let found: BTreeSet<ClosureViolation> = self
            .network_rules()
            .into_iter()
            .filter(|rule| !generated.contains(&rule.key))
            .flat_map(|rule| {
                rule.endpoints
                    .into_iter()
                    .flat_map(move |endpoint| {
                        profile
                            .endpoints
                            .iter()
                            .filter(|provider| endpoint.host.overlaps(&provider.host))
                            .map(|provider| ClosureViolation {
                                rule: rule.key.clone(),
                                host: endpoint.host.clone(),
                                provider_host: provider.host.clone(),
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        found.into_iter().collect()
    }

    pub fn with_rule(&self, rule: NetworkRule) -> Result<PolicyDocument, PolicyError> {
        yaml::with_rule(&self.value, &rule)
    }

    pub fn render(&self) -> String {
        yaml::render(&self.value)
    }

    pub(crate) fn proto(&self) -> &SandboxPolicy {
        &self.proto
    }

    pub(crate) fn from_proto(proto: &SandboxPolicy) -> Result<PolicyDocument, PolicyError> {
        yaml::from_proto(proto)
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
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Label<'a> {
    Exact(&'a str),
    One,
    Many,
}

fn labels(pattern: &str) -> Vec<Label<'_>> {
    pattern
        .split('.')
        .map(|label| {
            if label == "**" {
                Label::Many
            } else if label.contains('*') {
                Label::One
            } else {
                Label::Exact(label)
            }
        })
        .collect()
}

fn label_overlap(a: Label<'_>, b: Label<'_>) -> bool {
    match (a, b) {
        (Label::Exact(x), Label::Exact(y)) => x == y,
        _ => true,
    }
}

fn sequences_overlap(a: &[Label<'_>], b: &[Label<'_>]) -> bool {
    match (a.split_first(), b.split_first()) {
        (None, None) => true,
        (None, Some(_)) | (Some(_), None) => false,
        (Some((Label::Many, rest_a)), Some((Label::Many, rest_b))) => {
            sequences_overlap(rest_a, b)
                || sequences_overlap(a, rest_b)
                || sequences_overlap(rest_a, rest_b)
        }
        (Some((Label::Many, rest_a)), Some((_, rest_b))) => {
            sequences_overlap(rest_a, rest_b) || sequences_overlap(a, rest_b)
        }
        (Some((_, rest_a)), Some((Label::Many, rest_b))) => {
            sequences_overlap(rest_a, rest_b) || sequences_overlap(rest_a, b)
        }
        (Some((first_a, rest_a)), Some((first_b, rest_b))) => {
            label_overlap(*first_a, *first_b) && sequences_overlap(rest_a, rest_b)
        }
    }
}

impl HostPattern {
    pub fn parse(text: &str) -> Result<HostPattern, PolicyError> {
        let valid = !text.is_empty()
            && text.chars().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '*' | ':' | '[' | ']')
            });
        if valid {
            Ok(HostPattern(text.to_ascii_lowercase()))
        } else {
            Err(PolicyError::InvalidHost(text.to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn overlaps(&self, other: &HostPattern) -> bool {
        sequences_overlap(&labels(&self.0), &labels(&other.0))
    }
}

impl fmt::Display for HostPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl NetworkRule {
    pub fn docker_engine(port: NonZeroU16) -> NetworkRule {
        NetworkRule {
            key: RuleKey("docker_engine".to_owned()),
            endpoints: vec![RuleEndpoint {
                host: HostPattern("host.openshell.internal".to_owned()),
                port: port.get(),
                protocol: EndpointProtocol::Tcp,
                tls: TlsMode::Skip,
            }],
            binaries: vec![SandboxPath::from_static("/usr/local/bin/docker")],
        }
    }
}
