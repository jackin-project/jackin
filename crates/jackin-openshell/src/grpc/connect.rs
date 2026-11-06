// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use openshell_sdk::raw::proto::{HealthRequest, ServiceStatus};
use openshell_sdk::{ClientConfig, ClientIdentity, OpenShellClient, SdkError};
use tokio::net::TcpStream;
use tonic::codegen::http::Uri;

use super::{GrpcGateway, GrpcGatewayConfig, Inner};
use crate::error::{GatewayError, GatewayOp};
use crate::gateway::GatewayHealth;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const HTTPS_PORT: u16 = 443;

fn unreachable(endpoint: &str, message: impl ToString) -> GatewayError {
    GatewayError::Unreachable {
        endpoint: endpoint.to_owned(),
        message: message.to_string(),
    }
}

async fn probe(endpoint: &str) -> Result<(), GatewayError> {
    let uri: Uri = endpoint
        .parse()
        .map_err(|error| unreachable(endpoint, format!("invalid endpoint: {error}")))?;
    let host = uri
        .host()
        .ok_or_else(|| unreachable(endpoint, "the endpoint has no host"))?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_owned();
    let port = uri.port_u16().unwrap_or(HTTPS_PORT);
    match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((host.as_str(), port))).await {
        Err(_) => Err(unreachable(endpoint, "the connection timed out")),
        Ok(Err(error)) => Err(unreachable(endpoint, error)),
        Ok(Ok(_)) => Ok(()),
    }
}

fn classify(endpoint: &str, error: SdkError) -> GatewayError {
    match error {
        SdkError::Tls { message } => GatewayError::Tls { message },
        SdkError::Connect { .. } => GatewayError::Tls {
            message: format!(
                "the TLS handshake with {endpoint} failed; check that its certificate is signed by the registered ca.crt"
            ),
        },
        SdkError::Io { source } => unreachable(endpoint, source),
        other => GatewayError::Protocol {
            message: other.to_string(),
        },
    }
}

impl GrpcGateway {
    pub async fn connect(config: GrpcGatewayConfig) -> Result<GrpcGateway, GatewayError> {
        let cancel = config.cancel.clone();
        tokio::select! {
            biased;
            () = cancel.cancelled() => Err(GatewayError::Cancelled {
                op: GatewayOp::Health,
                group: None,
            }),
            connected = Self::establish(config) => connected,
        }
    }

    async fn establish(config: GrpcGatewayConfig) -> Result<GrpcGateway, GatewayError> {
        let gateway = &config.gateway;
        let endpoint = gateway.endpoint().to_owned();
        probe(&endpoint).await?;
        let mut client_config = ClientConfig::new(endpoint.clone());
        client_config.ca_cert = Some(gateway.ca().to_vec());
        client_config.client_identity = Some(ClientIdentity::new(
            gateway.certificate().to_vec(),
            gateway.key().to_vec(),
        ));
        let client = OpenShellClient::connect(client_config)
            .await
            .map_err(|error| classify(&endpoint, error))?;
        Ok(GrpcGateway {
            inner: Arc::new(Inner {
                name: gateway.name().clone(),
                endpoint,
                workspace: config.workspace,
                client,
                runner: config.runner,
                transfer_timeout: config.transfer_timeout,
                call_timeout: config.call_timeout,
            }),
        })
    }
}

impl Inner {
    pub(crate) async fn health(&self) -> Result<GatewayHealth, GatewayError> {
        let response = self
            .unary(
                GatewayOp::Health,
                None,
                self.grpc().health(HealthRequest {}),
            )
            .await?;
        Ok(GatewayHealth {
            version: response.version,
            serving: response.status == ServiceStatus::Healthy as i32,
        })
    }
}
