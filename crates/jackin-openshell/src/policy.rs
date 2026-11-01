// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ProfileId, SandboxPath};

#[derive(Clone, Debug)]
pub struct PolicyDocument {}

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureViolation {
    pub rule: RuleKey,
    pub host: HostPattern,
    pub provider_host: HostPattern,
}
