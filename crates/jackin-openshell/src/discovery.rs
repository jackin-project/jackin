// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::path::Path;
use std::time::Duration;

use jackin_core::{
    CredentialFileProblem, FileSystem, GatewayName, HostPath, ProcessOutput, ProcessRunner,
    ProcessSpec, ProcessStatus, ProcessStdin, Program, SecretBytes, TerminalText,
    read_credential_file,
};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::cli::inherited_env_prefixes;
use crate::error::DiscoveryError;

const HTTPS: &str = "https://";
const SCHEME_MAX: usize = 16;
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
const TAIL_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredGateway {
    pub name: GatewayName,
    pub endpoint: String,
    pub auth: GatewayAuth,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GatewayAuth {
    Mtls,
    Other(String),
}

pub struct ResolvedGateway {
    name: GatewayName,
    endpoint: String,
    ca: Vec<u8>,
    certificate: Vec<u8>,
    key: SecretBytes,
}

impl fmt::Debug for ResolvedGateway {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedGateway")
            .field("name", &self.name)
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl Clone for ResolvedGateway {
    fn clone(&self) -> ResolvedGateway {
        ResolvedGateway {
            name: self.name.clone(),
            endpoint: self.endpoint.clone(),
            ca: self.ca.clone(),
            certificate: self.certificate.clone(),
            key: SecretBytes::new(self.key.expose().to_vec()),
        }
    }
}

impl ResolvedGateway {
    pub fn name(&self) -> &GatewayName {
        &self.name
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub(crate) fn ca(&self) -> &[u8] {
        &self.ca
    }

    pub(crate) fn certificate(&self) -> &[u8] {
        &self.certificate
    }

    pub(crate) fn key(&self) -> &[u8] {
        self.key.expose()
    }
}

pub(crate) fn tail(bytes: &[u8]) -> TerminalText {
    let start = bytes.len().saturating_sub(TAIL_BYTES);
    TerminalText::sanitize(&bytes[start..])
}

pub(crate) fn failure_tail(output: &ProcessOutput) -> TerminalText {
    if output.stderr.is_empty() {
        tail(&output.stdout)
    } else {
        tail(&output.stderr)
    }
}

fn list_spec() -> ProcessSpec {
    ProcessSpec {
        program: Program::Openshell,
        args: ["gateway", "list", "-o", "json"]
            .map(str::to_owned)
            .to_vec(),
        cwd: None,
        env: Vec::new(),
        env_remove_prefixes: inherited_env_prefixes(),
        stdin: ProcessStdin::Null,
        timeout: LIST_TIMEOUT,
    }
}

fn unparsable(message: impl ToString) -> DiscoveryError {
    DiscoveryError::ListUnparsable {
        message: message.to_string(),
    }
}

fn parse_entry(entry: &Value) -> Result<Option<RegisteredGateway>, DiscoveryError> {
    let field = |key: &str| {
        entry
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| unparsable(format!("a gateway entry has no string field {key}")))
    };
    let Ok(name) = GatewayName::parse(field("name")?) else {
        return Ok(None);
    };
    let auth = match field("auth")? {
        "mtls" => GatewayAuth::Mtls,
        other => GatewayAuth::Other(other.to_owned()),
    };
    Ok(Some(RegisteredGateway {
        name,
        endpoint: field("endpoint")?.to_owned(),
        auth,
    }))
}

pub async fn registered_gateways(
    runner: &dyn ProcessRunner,
    cancel: &CancellationToken,
) -> Result<Vec<RegisteredGateway>, DiscoveryError> {
    let output = runner.run(&list_spec(), None, cancel).await?;
    if output.status != ProcessStatus::Exited(0) {
        return Err(DiscoveryError::ListFailed {
            status: output.status,
            tail: failure_tail(&output),
        });
    }
    let parsed: Value = serde_json::from_slice(&output.stdout).map_err(unparsable)?;
    let Some(entries) = parsed.as_array() else {
        return Err(unparsable("the output is not a list"));
    };
    let mut gateways = Vec::new();
    for entry in entries {
        gateways.extend(parse_entry(entry)?);
    }
    Ok(gateways)
}

async fn read_material(
    fs: &dyn FileSystem,
    home: &HostPath,
    name: &GatewayName,
    file: &str,
) -> Result<Vec<u8>, DiscoveryError> {
    let relative = Path::new("gateways")
        .join(name.as_str())
        .join("mtls")
        .join(file);
    let path = HostPath::resolve(&relative, home).map_err(unparsable)?;
    match read_credential_file(fs, &path).await? {
        Ok(bytes) => Ok(bytes),
        Err(CredentialFileProblem::Missing) => Err(DiscoveryError::MissingCertificate { path }),
        Err(CredentialFileProblem::NotARegularFile) => {
            Err(DiscoveryError::CertificateNotRegular { path })
        }
        Err(CredentialFileProblem::TooLarge) => Err(DiscoveryError::CertificateTooLarge { path }),
    }
}

fn scheme_of(endpoint: &str) -> Option<String> {
    let (scheme, _) = endpoint.split_once("://")?;
    let plain = !scheme.is_empty()
        && scheme.len() <= SCHEME_MAX
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    plain.then(|| scheme.to_owned())
}

fn require_secure(entry: &RegisteredGateway) -> Result<(), DiscoveryError> {
    if !entry.endpoint.starts_with(HTTPS) {
        return Err(DiscoveryError::InsecureEndpoint {
            name: entry.name.clone(),
            scheme: scheme_of(&entry.endpoint),
        });
    }
    let rest = &entry.endpoint[HTTPS.len()..];
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.contains('@') {
        return Err(DiscoveryError::EndpointCredentials {
            name: entry.name.clone(),
        });
    }
    if rest.contains(['?', '#']) {
        return Err(DiscoveryError::EndpointQuery {
            name: entry.name.clone(),
        });
    }
    match &entry.auth {
        GatewayAuth::Mtls => Ok(()),
        GatewayAuth::Other(auth) => Err(DiscoveryError::UnsupportedAuth {
            name: entry.name.clone(),
            auth: auth.clone(),
        }),
    }
}

pub async fn resolve_gateway(
    runner: &dyn ProcessRunner,
    fs: &dyn FileSystem,
    openshell_config_home: &HostPath,
    name: &GatewayName,
    cancel: &CancellationToken,
) -> Result<ResolvedGateway, DiscoveryError> {
    let registered = registered_gateways(runner, cancel).await?;
    let entry = registered
        .into_iter()
        .find(|entry| &entry.name == name)
        .ok_or_else(|| DiscoveryError::UnknownGateway(name.clone()))?;
    require_secure(&entry)?;
    let home = openshell_config_home;
    let ca = read_material(fs, home, name, "ca.crt").await?;
    let certificate = read_material(fs, home, name, "tls.crt").await?;
    let key = read_material(fs, home, name, "tls.key").await?;
    Ok(ResolvedGateway {
        name: entry.name,
        endpoint: entry.endpoint,
        ca,
        certificate,
        key: SecretBytes::new(key),
    })
}
