// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{ProfileAsset, ProfileId, SandboxPath};
use noyalib::{Mapping, Value};
use openshell_sdk::raw::proto::{NetworkTlsMode, SandboxPolicy};

use super::{
    ClosureViolation, DOCKER_ENGINE_RULE, EndpointProtocol, HostPattern, NetworkRule,
    PolicyDocument, ProfileEndpoint, ProviderProfile, RuleEndpoint, RuleKey, TlsMode,
};
use crate::error::PolicyError;

const NETWORK_POLICIES: &str = "network_policies";
const FILESYSTEM_POLICY: &str = "filesystem_policy";
const READ_ONLY: &str = "read_only";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Source {
    Live,
    Authored,
}

pub(super) fn is_reserved(key: &str) -> bool {
    key == DOCKER_ENGINE_RULE || openshell_policy::is_provider_rule_name(key)
}

pub(super) fn provider_rule_name(provider: &str) -> String {
    openshell_policy::provider_rule_name(provider)
}

fn syntax(message: impl ToString) -> PolicyError {
    PolicyError::Syntax {
        message: message.to_string(),
    }
}

fn port_problem(rule: &str, value: &Value) -> Option<PolicyError> {
    let port = value.as_i64().or_else(|| {
        value
            .as_u64()
            .map(|port| i64::try_from(port).unwrap_or(i64::MAX))
    })?;
    if (1..=i64::from(u16::MAX)).contains(&port) {
        None
    } else {
        Some(PolicyError::InvalidPort {
            rule: rule.to_owned(),
            port,
        })
    }
}

fn check_endpoint(rule: &str, endpoint: &Value) -> Result<(), PolicyError> {
    let host = endpoint
        .get("host")
        .and_then(Value::as_str)
        .unwrap_or_default();
    HostPattern::parse(host)?;
    if let Some(error) = endpoint
        .get("port")
        .and_then(|port| port_problem(rule, port))
    {
        return Err(error);
    }
    if let Some(ports) = endpoint.get("ports").and_then(Value::as_sequence) {
        for port in ports {
            if let Some(error) = port_problem(rule, port) {
                return Err(error);
            }
        }
    }
    Ok(())
}

fn check_shape(root: &Value, source: Source) -> Result<(), PolicyError> {
    if !root.is_mapping() {
        return Err(syntax("the policy document is not a mapping"));
    }
    let Some(rules) = root.get(NETWORK_POLICIES) else {
        return Ok(());
    };
    if rules.is_null() {
        return Ok(());
    }
    let Some(rules) = rules.as_mapping() else {
        return Err(PolicyError::NotAMapping {
            key: NETWORK_POLICIES.to_owned(),
        });
    };
    for (key, rule) in rules {
        if source == Source::Authored {
            if is_reserved(key) {
                return Err(PolicyError::ReservedRuleName(key.to_owned()));
            }
            if let Some(endpoints) = rule.get("endpoints").and_then(Value::as_sequence) {
                for endpoint in endpoints {
                    check_endpoint(key, endpoint)?;
                }
            }
        }
    }
    Ok(())
}

fn normalized(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|piece| !piece.is_empty() && *piece != ".")
        .collect()
}

fn check_read_write(paths: &[String]) -> Result<(), PolicyError> {
    for path in paths.iter().filter(|path| path.starts_with('/')) {
        match normalized(path).first().copied() {
            None => return Err(PolicyError::ReadWriteRoot { path: path.clone() }),
            Some("proc" | "sys") => {
                return Err(PolicyError::ReadWriteAlias { path: path.clone() });
            }
            Some(_) => {}
        }
    }
    Ok(())
}

fn check_sandbox(proto: &SandboxPolicy) -> Result<(), PolicyError> {
    let found = proto
        .landlock
        .as_ref()
        .map(|landlock| landlock.compatibility.as_str());
    if found != Some("hard_requirement") {
        return Err(PolicyError::LandlockMode {
            found: found.map(str::to_owned),
        });
    }
    let Some(filesystem) = proto
        .filesystem
        .as_ref()
        .filter(|filesystem| !filesystem.read_only.is_empty() || !filesystem.read_write.is_empty())
    else {
        return Err(PolicyError::NoFilesystemPaths);
    };
    check_read_write(&filesystem.read_write)?;
    openshell_policy::validate_sandbox_policy(proto)
        .map_err(|violations| PolicyError::Gateway { violations })
}

pub(super) fn build(text: &str, source: Source) -> Result<PolicyDocument, PolicyError> {
    let value: Value = noyalib::from_str(text).map_err(syntax)?;
    check_shape(&value, source)?;
    let proto = openshell_policy::parse_sandbox_policy(text).map_err(syntax)?;
    if source == Source::Authored {
        check_sandbox(&proto)?;
    }
    Ok(PolicyDocument { value, proto })
}

pub(super) fn from_proto(proto: &SandboxPolicy) -> Result<PolicyDocument, PolicyError> {
    let text = openshell_policy::serialize_sandbox_policy(proto).map_err(syntax)?;
    build(&text, Source::Live)
}

pub(super) fn render(value: &Value) -> Result<String, PolicyError> {
    noyalib::to_string(value).map_err(syntax)
}

pub(super) struct Scanned {
    pub(super) rule: NetworkRule,
    pub(super) unreadable: Vec<ClosureViolation>,
}

pub(super) fn scan(proto: &SandboxPolicy) -> Vec<Scanned> {
    let sorted: BTreeMap<_, _> = proto.network_policies.iter().collect();
    sorted
        .into_iter()
        .map(|(key, rule)| {
            let key = RuleKey::new(key.clone());
            let mut endpoints = Vec::new();
            let mut unreadable = Vec::new();
            for endpoint in &rule.endpoints {
                let Ok(host) = HostPattern::parse(&endpoint.host) else {
                    unreadable.push(ClosureViolation::UnparsableHost {
                        rule: key.clone(),
                        host: endpoint.host.clone(),
                    });
                    continue;
                };
                let ports = if endpoint.ports.is_empty() {
                    vec![endpoint.port]
                } else {
                    endpoint.ports.clone()
                };
                let protocol = match endpoint.protocol.as_str() {
                    "" | "tcp" => EndpointProtocol::Tcp,
                    _ => EndpointProtocol::Rest,
                };
                let tls = if endpoint.tls == NetworkTlsMode::Skip as i32 {
                    TlsMode::Skip
                } else {
                    TlsMode::Default
                };
                for port in ports {
                    match u16::try_from(port) {
                        Ok(valid) if valid != 0 => endpoints.push(RuleEndpoint {
                            host: host.clone(),
                            port: valid,
                            protocol,
                            tls,
                        }),
                        _ => unreadable.push(ClosureViolation::InvalidPort {
                            rule: key.clone(),
                            port,
                        }),
                    }
                }
            }
            Scanned {
                rule: NetworkRule {
                    key,
                    endpoints,
                    binaries: rule
                        .binaries
                        .iter()
                        .filter_map(|binary| SandboxPath::parse(&binary.path).ok())
                        .collect(),
                },
                unreadable,
            }
        })
        .collect()
}

fn text(value: &str) -> Value {
    Value::String(value.to_owned())
}

fn rule_node(rule: &NetworkRule) -> Value {
    let mut node = Mapping::new();
    node.insert("name", text(rule.key.as_str()));
    let endpoints = rule
        .endpoints
        .iter()
        .map(|endpoint| {
            let mut entry = Mapping::new();
            entry.insert("host", text(endpoint.host.as_str()));
            entry.insert("port", Value::from(endpoint.port));
            entry.insert(
                "protocol",
                text(match endpoint.protocol {
                    EndpointProtocol::Tcp => "tcp",
                    EndpointProtocol::Rest => "rest",
                }),
            );
            if endpoint.tls == TlsMode::Skip {
                entry.insert("tls", text("skip"));
            }
            Value::Mapping(entry)
        })
        .collect();
    node.insert("endpoints", Value::Sequence(endpoints));
    let binaries = rule
        .binaries
        .iter()
        .map(|binary| {
            let mut entry = Mapping::new();
            entry.insert("path", text(binary.as_str()));
            Value::Mapping(entry)
        })
        .collect();
    node.insert("binaries", Value::Sequence(binaries));
    Value::Mapping(node)
}

pub(super) fn check_added(rule: &NetworkRule) -> Result<(), PolicyError> {
    if is_reserved(rule.key.as_str()) && rule.key.as_str() != DOCKER_ENGINE_RULE {
        return Err(PolicyError::ReservedRuleName(rule.key.as_str().to_owned()));
    }
    if rule.endpoints.iter().any(|endpoint| endpoint.port == 0) {
        return Err(PolicyError::InvalidPort {
            rule: rule.key.as_str().to_owned(),
            port: 0,
        });
    }
    Ok(())
}

pub(super) fn with_rule(value: &Value, rule: &NetworkRule) -> Result<PolicyDocument, PolicyError> {
    let mut root = value.clone();
    let Some(map) = root.as_mapping_mut() else {
        return Err(syntax("the policy document is not a mapping"));
    };
    let section = map
        .entry(NETWORK_POLICIES)
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    if section.is_null() {
        *section = Value::Mapping(Mapping::new());
    }
    let Some(rules) = section.as_mapping_mut() else {
        return Err(PolicyError::NotAMapping {
            key: NETWORK_POLICIES.to_owned(),
        });
    };
    if rules.contains_key(rule.key.as_str()) {
        return Err(PolicyError::DuplicateRule(rule.key.clone()));
    }
    rules.insert(rule.key.as_str(), rule_node(rule));
    build(&render(&root)?, Source::Live)
}

pub(super) fn with_read_only(value: &Value, path: &str) -> Result<PolicyDocument, PolicyError> {
    let mut root = value.clone();
    let Some(map) = root.as_mapping_mut() else {
        return Err(syntax("the policy document is not a mapping"));
    };
    let section = map
        .entry(FILESYSTEM_POLICY)
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    if section.is_null() {
        *section = Value::Mapping(Mapping::new());
    }
    let Some(fields) = section.as_mapping_mut() else {
        return Err(PolicyError::NotAMapping {
            key: FILESYSTEM_POLICY.to_owned(),
        });
    };
    let mut paths = match fields.get(READ_ONLY) {
        None => Vec::new(),
        Some(value) if value.is_null() => Vec::new(),
        Some(Value::Sequence(items)) => items.clone(),
        Some(_) => {
            return Err(PolicyError::NotAMapping {
                key: format!("{FILESYSTEM_POLICY}.{READ_ONLY}"),
            });
        }
    };
    let wanted = normalized(path);
    if !paths
        .iter()
        .filter_map(Value::as_str)
        .any(|found| normalized(found) == wanted)
    {
        paths.push(text(path));
    }
    fields.insert(READ_ONLY, Value::Sequence(paths));
    build(&render(&root)?, Source::Live)
}

fn profile_error(id: &str, message: impl ToString) -> PolicyError {
    PolicyError::Profile {
        id: id.to_owned(),
        message: message.to_string(),
    }
}

pub(super) fn parse_profile(asset: &'static ProfileAsset) -> Result<ProviderProfile, PolicyError> {
    let id = ProfileId::parse(asset.id).map_err(|error| profile_error(asset.id, error))?;
    let parsed = openshell_providers::parse_profile_yaml(asset.yaml)
        .map_err(|error| profile_error(asset.id, error))?;
    if parsed.id != asset.id {
        return Err(profile_error(
            asset.id,
            format!("the asset declares the id {}", parsed.id),
        ));
    }
    let mut endpoints = Vec::new();
    for endpoint in &parsed.endpoints {
        let host =
            HostPattern::parse(&endpoint.host).map_err(|error| profile_error(asset.id, error))?;
        let ports = if endpoint.ports.is_empty() {
            vec![endpoint.port]
        } else {
            endpoint.ports.clone()
        };
        for port in ports {
            let port = u16::try_from(port)
                .map_err(|_| profile_error(asset.id, format!("port {port} is out of range")))?;
            endpoints.push(ProfileEndpoint {
                host: host.clone(),
                port,
            });
        }
    }
    let binaries = parsed
        .binaries
        .iter()
        .map(|binary| {
            SandboxPath::parse(&binary.path).map_err(|error| profile_error(asset.id, error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProviderProfile {
        id,
        yaml: asset.yaml,
        endpoints,
        binaries,
    })
}

pub(super) fn profile_to_proto(
    profile: &ProviderProfile,
) -> Result<openshell_sdk::raw::proto::ProviderProfile, PolicyError> {
    openshell_providers::parse_profile_yaml(profile.yaml)
        .map(|parsed| parsed.to_proto())
        .map_err(|error| profile_error(profile.id.as_str(), error))
}
