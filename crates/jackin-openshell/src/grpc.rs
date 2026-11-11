// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::net::Ipv6Addr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    EnvName, GatewayName, HostPath, OpenShellWorkspace, ProcessRunner, ProfileId, ProjectRoot,
    ProviderName, SandboxName, SandboxPath, Sha256,
};
use openshell_sdk::OpenShellClient;
use openshell_sdk::raw::AuthedGrpcClient;
use openshell_sdk::raw::proto::{WorkspaceSelector, workspace_selector};
use tokio_util::sync::CancellationToken;

use crate::discovery::ResolvedGateway;
use crate::error::{GatewayError, GatewayOp};
use crate::gateway::{
    Gateway, GatewayHealth, PowerAction, ProfileSync, ProgramOutcome, ProgramRun, ProviderSpec,
    ProviderState, SandboxCreate, SandboxDetail, SandboxFile, SandboxQuery, SandboxView,
};
use crate::policy::ProviderProfile;
use crate::terminal::{TerminalRequest, TerminalSession};

mod connect;
mod main_pty;
mod program;
mod provider;
mod sandbox;
mod shell;
mod status;

const REDACTED_ENDPOINT: &str = "<redacted>";

pub(crate) fn endpoint_authority(endpoint: &str) -> String {
    parse_authority(endpoint).unwrap_or_else(|_| REDACTED_ENDPOINT.to_owned())
}

fn parse_authority(endpoint: &str) -> Result<String, &'static str> {
    let Some((scheme, rest)) = endpoint.split_once("://") else {
        return Err("the scheme is missing");
    };
    if !valid_scheme(scheme) {
        return Err("the scheme is invalid");
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() {
        return Err("the authority is missing");
    }
    if authority.contains('@') {
        return Err("the authority carries credentials");
    }
    let host = valid_host(authority)?;
    Ok(format!("{scheme}://{host}"))
}

fn valid_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn valid_host(host: &str) -> Result<String, &'static str> {
    if let Some(rest) = host.strip_prefix('[') {
        let Some((address, tail)) = rest.split_once(']') else {
            return Err("the host is invalid");
        };
        if !valid_ipv6(address) {
            return Err("the host is invalid");
        }
        if tail.is_empty() {
            return Ok(format!("[{address}]"));
        }
        let Some(port) = tail.strip_prefix(':') else {
            return Err("the port is invalid");
        };
        valid_port(port)?;
        return Ok(format!("[{address}]:{port}"));
    }
    if let Some((name, port)) = host.rsplit_once(':') {
        valid_port(port)?;
        if !valid_name(name) {
            return Err("the host is invalid");
        }
        return Ok(format!("{name}:{port}"));
    }
    if !valid_name(host) {
        return Err("the host is invalid");
    }
    Ok(host.to_owned())
}

fn valid_ipv6(address: &str) -> bool {
    address.parse::<Ipv6Addr>().is_ok()
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~'))
}

fn valid_port(port: &str) -> Result<(), &'static str> {
    if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("the port is invalid");
    }
    port.parse::<u16>().map_err(|_| "the port is invalid")?;
    Ok(())
}

#[derive(Clone)]
pub struct GrpcGatewayConfig {
    pub gateway: ResolvedGateway,
    pub workspace: OpenShellWorkspace,
    pub runner: Arc<dyn ProcessRunner>,
    pub transfer_timeout: Duration,
    pub call_timeout: Duration,
    pub cancel: CancellationToken,
}

#[derive(Clone)]
pub struct GrpcGateway {
    inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub(crate) name: GatewayName,
    pub(crate) endpoint: String,
    pub(crate) workspace: OpenShellWorkspace,
    pub(crate) client: OpenShellClient,
    pub(crate) runner: Arc<dyn ProcessRunner>,
    pub(crate) transfer_timeout: Duration,
    pub(crate) call_timeout: Duration,
}

impl fmt::Debug for GrpcGatewayConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GrpcGatewayConfig")
            .field("name", &self.gateway.name())
            .field("endpoint", &endpoint_authority(self.gateway.endpoint()))
            .field("workspace", &self.workspace)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for GrpcGateway {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GrpcGateway")
            .field("name", &self.inner.name)
            .field("endpoint", &endpoint_authority(&self.inner.endpoint))
            .field("workspace", &self.inner.workspace)
            .finish_non_exhaustive()
    }
}

impl Inner {
    pub(crate) fn grpc(&self) -> AuthedGrpcClient {
        self.client.raw_grpc()
    }

    pub(crate) fn scope(&self) -> WorkspaceSelector {
        workspace_selector(self.workspace.as_str())
    }

    pub(crate) async fn unary<T>(
        &self,
        op: GatewayOp,
        missing: Option<crate::error::Missing>,
        call: impl Future<Output = Result<tonic::Response<T>, tonic::Status>>,
    ) -> Result<T, GatewayError> {
        match tokio::time::timeout(self.call_timeout, call).await {
            Err(_) => Err(GatewayError::Timeout {
                op,
                after: self.call_timeout,
                group: None,
            }),
            Ok(Ok(response)) => Ok(response.into_inner()),
            Ok(Err(status)) => Err(self.map_status(op, missing, &status)),
        }
    }
}

#[async_trait]
impl Gateway for GrpcGateway {
    fn name(&self) -> &GatewayName {
        &self.inner.name
    }

    async fn health(&self) -> Result<GatewayHealth, GatewayError> {
        self.inner.health().await
    }

    async fn ensure_profile(&self, profile: &ProviderProfile) -> Result<ProfileSync, GatewayError> {
        self.inner.ensure_profile(profile).await
    }

    async fn delete_profile(&self, id: &ProfileId) -> Result<(), GatewayError> {
        self.inner.delete_profile(id).await
    }

    async fn provider_state(&self, name: &ProviderName) -> Result<ProviderState, GatewayError> {
        self.inner.provider_state(name).await
    }

    async fn put_provider(&self, spec: &ProviderSpec) -> Result<(), GatewayError> {
        self.inner.put_provider(spec).await
    }

    async fn delete_provider(&self, name: &ProviderName) -> Result<(), GatewayError> {
        self.inner.delete_provider(name).await
    }

    async fn create_sandbox(&self, spec: &SandboxCreate) -> Result<SandboxView, GatewayError> {
        self.inner.create_sandbox(spec).await
    }

    async fn find_sandboxes(&self, query: &SandboxQuery) -> Result<Vec<SandboxView>, GatewayError> {
        self.inner.find_sandboxes(query).await
    }

    async fn sandbox_detail(&self, name: &SandboxName) -> Result<SandboxDetail, GatewayError> {
        self.inner.sandbox_detail(name).await
    }

    async fn sandbox_environment(
        &self,
        name: &SandboxName,
    ) -> Result<BTreeMap<EnvName, String>, GatewayError> {
        self.inner.sandbox_environment(name).await
    }

    async fn set_power(
        &self,
        name: &SandboxName,
        action: PowerAction,
    ) -> Result<SandboxView, GatewayError> {
        self.inner.set_power(name, action).await
    }

    async fn delete_sandbox(&self, name: &SandboxName) -> Result<(), GatewayError> {
        self.inner.delete_sandbox(name).await
    }

    async fn run_program(
        &self,
        run: &ProgramRun,
        cancel: &CancellationToken,
    ) -> Result<ProgramOutcome, GatewayError> {
        self.inner.run_program(run, cancel).await
    }

    async fn read_files(
        &self,
        sandbox: &SandboxName,
        paths: &[SandboxPath],
    ) -> Result<Vec<Option<Vec<u8>>>, GatewayError> {
        self.inner.read_files(sandbox, paths).await
    }

    async fn write_file(
        &self,
        sandbox: &SandboxName,
        file: &SandboxFile,
    ) -> Result<(), GatewayError> {
        self.inner.write_file(sandbox, file).await
    }

    async fn upload_tree(
        &self,
        sandbox: &SandboxName,
        source: &ProjectRoot,
        dest: &SandboxPath,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        crate::transfer::upload_tree(&self.inner, sandbox, source, dest, cancel).await
    }

    async fn upload_file(
        &self,
        sandbox: &SandboxName,
        source: &HostPath,
        dest: &SandboxPath,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        crate::transfer::upload_file(&self.inner, sandbox, source, dest, cancel).await
    }

    async fn download_file(
        &self,
        sandbox: &SandboxName,
        remote: &SandboxPath,
        local: &HostPath,
        cancel: &CancellationToken,
    ) -> Result<Sha256, GatewayError> {
        crate::transfer::download_file(&self.inner, sandbox, remote, local, cancel).await
    }

    async fn open_terminal(
        &self,
        request: &TerminalRequest,
    ) -> Result<TerminalSession, GatewayError> {
        self.inner.open_terminal(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::{REDACTED_ENDPOINT, endpoint_authority, parse_authority};

    #[test]
    fn the_authority_keeps_the_scheme_the_host_and_the_explicit_port() {
        assert_eq!(
            endpoint_authority("https://127.0.0.1:8443/path-secret"),
            "https://127.0.0.1:8443"
        );
        assert_eq!(
            endpoint_authority("https://example.com/path-secret"),
            "https://example.com"
        );
        assert_eq!(
            endpoint_authority("https://[::1]:8443/path-secret"),
            "https://[::1]:8443"
        );
        assert_eq!(
            endpoint_authority("https://[::1]/path-secret"),
            "https://[::1]"
        );
    }

    #[test]
    fn the_authority_drops_the_path_the_query_and_the_fragment() {
        assert_eq!(
            endpoint_authority("https://example.com/path-secret?token=query-secret#fragment"),
            "https://example.com"
        );
    }

    #[test]
    fn a_relative_or_a_malformed_endpoint_never_keeps_a_secret() {
        for endpoint in [
            "path-secret",
            "relative/path-secret?redirect=https://alice:password@example.com/path#fragment",
            "https://",
            "https://gw example.com:8443/path-secret",
            "https://example.com:port/path-secret",
            "https://[::1/path-secret",
            "https://alice:password@example.com/path-secret",
        ] {
            let authority = endpoint_authority(endpoint);
            assert_eq!(authority, REDACTED_ENDPOINT, "{endpoint}");
            for secret in [
                "path-secret",
                "alice",
                "password",
                "redirect",
                "gw example",
                "query-secret",
            ] {
                assert!(!authority.contains(secret), "{endpoint}: {authority}");
            }
        }
    }

    #[test]
    fn a_bracketed_host_must_be_a_valid_ipv6_address() {
        for endpoint in [
            "https://[deadbeef]:8443/path-secret",
            "https://[:::]:8443/path-secret",
            "https://[127.0.0.1]:8443/path-secret",
        ] {
            let authority = endpoint_authority(endpoint);
            assert_eq!(authority, REDACTED_ENDPOINT, "{endpoint}");
        }
        assert_eq!(
            parse_authority("https://[deadbeef]:8443/path-secret"),
            Err("the host is invalid")
        );
        assert_eq!(
            parse_authority("https://[:::]:8443/path-secret"),
            Err("the host is invalid")
        );
        assert_eq!(
            parse_authority("https://[127.0.0.1]:8443/path-secret"),
            Err("the host is invalid")
        );
        assert_eq!(
            parse_authority("https://[::ffff:127.0.0.1]:8443/path-secret"),
            Ok("https://[::ffff:127.0.0.1]:8443".to_owned())
        );
    }

    #[test]
    fn a_percent_sign_is_never_valid_in_a_transport_host() {
        assert_eq!(
            parse_authority("https://secret%ZZ/path-secret"),
            Err("the host is invalid")
        );
        assert_eq!(
            endpoint_authority("https://secret%ZZ/path-secret"),
            REDACTED_ENDPOINT
        );
    }

    #[test]
    fn every_rejection_names_its_reason() {
        assert_eq!(
            parse_authority("path-secret"),
            Err("the scheme is missing")
        );
        assert_eq!(
            parse_authority("relative/path-secret?redirect=https://alice:password@example.com/path"),
            Err("the scheme is invalid")
        );
        assert_eq!(parse_authority("https://"), Err("the authority is missing"));
        assert_eq!(
            parse_authority("https://alice:password@example.com/path-secret"),
            Err("the authority carries credentials")
        );
        assert_eq!(
            parse_authority("https://gw example.com:8443/path-secret"),
            Err("the host is invalid")
        );
        assert_eq!(
            parse_authority("https://example.com:port/path-secret"),
            Err("the port is invalid")
        );
        assert_eq!(
            parse_authority("https://[::1/path-secret"),
            Err("the host is invalid")
        );
        assert_eq!(
            parse_authority("https://example.com:65536/path-secret"),
            Err("the port is invalid")
        );
    }
}
