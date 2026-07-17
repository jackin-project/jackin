// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Physical OTLP channel ownership and eventually delivered connection facts.

use super::super::config::TlsConfig;
use jackin_telemetry::schema::enums::{ConnectionPeerType, ErrorType, OutcomeValue};
use rustls::pki_types::{CertificateDer, ServerName};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::io::{BufReader, Cursor};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tonic::codegen::http::Uri;
use tonic::transport::{Channel, Endpoint};
use tower::Service;

const MAX_PENDING_ATTEMPTS: usize = 256;

#[cfg(test)]
pub(super) fn uses_tls(endpoint: &str) -> bool {
    endpoint.starts_with("https://")
}

#[cfg(test)]
pub(super) fn validate_transport(endpoint: &str, tls: &TlsConfig) -> anyhow::Result<()> {
    PhysicalConnector::new(&ChannelKey::new(endpoint, Duration::from_secs(1), tls)).map(drop)
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) struct ChannelKey {
    endpoint: String,
    timeout: Duration,
    certificate: Option<String>,
    client_key: Option<String>,
    client_certificate: Option<String>,
}

impl ChannelKey {
    pub(super) fn new(endpoint: &str, timeout: Duration, tls: &TlsConfig) -> Self {
        Self {
            endpoint: endpoint.to_owned(),
            timeout,
            certificate: tls.certificate.clone(),
            client_key: tls.client_key.clone(),
            client_certificate: tls.client_certificate.clone(),
        }
    }
}

#[derive(Clone)]
pub(super) struct PhysicalChannels {
    channels: HashMap<ChannelKey, Channel>,
}

impl PhysicalChannels {
    pub(super) fn build(keys: impl IntoIterator<Item = ChannelKey>) -> anyhow::Result<Self> {
        let mut channels = HashMap::new();
        for key in keys {
            if channels.contains_key(&key) {
                continue;
            }
            let connector = PhysicalConnector::new(&key)?;
            let endpoint = Endpoint::from_shared(key.endpoint.clone())?
                .timeout(key.timeout)
                .connect_timeout(key.timeout);
            channels.insert(key, endpoint.connect_with_connector_lazy(connector));
        }
        Ok(Self { channels })
    }

    pub(super) fn get(&self, key: &ChannelKey) -> anyhow::Result<Channel> {
        self.channels
            .get(key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("OTLP physical channel was not constructed"))
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.channels.len()
    }
}

#[derive(Clone)]
struct PhysicalConnector {
    tls: Option<Arc<rustls::ClientConfig>>,
    timeout: Duration,
    observations: Arc<Mutex<VecDeque<AttemptObservation>>>,
}

#[derive(Clone, Copy)]
struct AttemptObservation {
    elapsed: Duration,
    outcome: OutcomeValue,
    error: Option<ErrorType>,
}

impl PhysicalConnector {
    fn new(key: &ChannelKey) -> anyhow::Result<Self> {
        let use_tls = key.endpoint.starts_with("https://")
            || key.certificate.is_some()
            || key.client_key.is_some()
            || key.client_certificate.is_some();
        let tls = use_tls.then(|| tls_config(key)).transpose()?.map(Arc::new);
        Ok(Self {
            tls,
            timeout: key.timeout,
            observations: Arc::new(Mutex::new(VecDeque::new())),
        })
    }

    fn record(&self, observation: AttemptObservation) {
        let recovered = observation.error.is_none();
        let drained = {
            let mut pending = self
                .observations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if pending.len() == MAX_PENDING_ATTEMPTS {
                pending.pop_front();
            }
            pending.push_back(observation);
            recovered.then(|| pending.drain(..).collect::<Vec<_>>())
        };
        if let Some(observations) = drained {
            for observation in observations {
                publish_observation(observation);
            }
        }
    }

    #[cfg(test)]
    fn pending_len(&self) -> usize {
        self.observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

pub(super) fn validate_tls_assets(tls: &TlsConfig, signal: &'static str) -> anyhow::Result<()> {
    if let Some(path) = &tls.certificate {
        std::fs::read(path)
            .map(drop)
            .map_err(|_| anyhow::anyhow!("OTLP {signal} CA certificate is unavailable"))?;
    }
    if let Some(path) = &tls.client_certificate {
        std::fs::read(path)
            .map(drop)
            .map_err(|_| anyhow::anyhow!("OTLP {signal} client certificate is unavailable"))?;
    }
    if let Some(path) = &tls.client_key {
        std::fs::read(path)
            .map(drop)
            .map_err(|_| anyhow::anyhow!("OTLP {signal} client key is unavailable"))?;
    }
    Ok(())
}

impl Service<Uri> for PhysicalConnector {
    type Response = hyper_util::rt::TokioIo<PhysicalIo>;
    type Error = std::io::Error;
    type Future = Pin<
        Box<
            dyn Future<Output = Result<hyper_util::rt::TokioIo<PhysicalIo>, std::io::Error>> + Send,
        >,
    >;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let connector = self.clone();
        Box::pin(async move {
            let started = Instant::now();
            let result = tokio::time::timeout(
                connector.timeout,
                connect_physical(uri, connector.tls.clone()),
            )
            .await
            .unwrap_or_else(|_| {
                Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "physical OTLP connection timed out",
                ))
            });
            connector.record(match &result {
                Ok(_) => AttemptObservation {
                    elapsed: started.elapsed(),
                    outcome: OutcomeValue::Success,
                    error: None,
                },
                Err(error) => AttemptObservation {
                    elapsed: started.elapsed(),
                    outcome: if error.kind() == std::io::ErrorKind::TimedOut {
                        OutcomeValue::Timeout
                    } else {
                        OutcomeValue::Error
                    },
                    error: Some(match error.kind() {
                        std::io::ErrorKind::TimedOut => ErrorType::Timeout,
                        std::io::ErrorKind::ConnectionRefused => ErrorType::ConnectionRefused,
                        _ => ErrorType::RpcError,
                    }),
                },
            });
            result.map(hyper_util::rt::TokioIo::new)
        })
    }
}

async fn connect_physical(
    uri: Uri,
    tls: Option<Arc<rustls::ClientConfig>>,
) -> std::io::Result<PhysicalIo> {
    let host = uri
        .host()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing host"))?;
    let port = uri
        .port_u16()
        .unwrap_or(if tls.is_some() { 443 } else { 80 });
    let stream = tokio::net::TcpStream::connect((host, port)).await?;
    stream.set_nodelay(true)?;
    let Some(config) = tls else {
        return Ok(PhysicalIo::Plain(stream));
    };
    let server_name = ServerName::try_from(host.to_owned())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid TLS name"))?;
    tokio_rustls::TlsConnector::from(config)
        .connect(server_name, stream)
        .await
        .map(|stream| PhysicalIo::Tls(Box::new(stream)))
        .map_err(std::io::Error::other)
}

fn tls_config(key: &ChannelKey) -> anyhow::Result<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    let native = rustls_native_certs::load_native_certs();
    for certificate in native.certs {
        let _ignored = roots.add(certificate);
    }
    if let Some(path) = &key.certificate {
        let pem = std::fs::read(path)
            .map_err(|_| anyhow::anyhow!("OTLP CA certificate is unavailable"))?;
        for certificate in rustls_pemfile::certs(&mut BufReader::new(Cursor::new(pem))) {
            roots
                .add(certificate.map_err(|_| anyhow::anyhow!("OTLP CA certificate is invalid"))?)
                .map_err(|_| anyhow::anyhow!("OTLP CA certificate is invalid"))?;
        }
    }
    let builder = rustls::ClientConfig::builder().with_root_certificates(roots);
    let mut config = match (&key.client_certificate, &key.client_key) {
        (Some(certificate), Some(key_path)) => {
            let certificate = std::fs::read(certificate)
                .map_err(|_| anyhow::anyhow!("OTLP client certificate is unavailable"))?;
            let certificates = rustls_pemfile::certs(&mut BufReader::new(Cursor::new(certificate)))
                .collect::<Result<Vec<CertificateDer<'static>>, _>>()
                .map_err(|_| anyhow::anyhow!("OTLP client certificate is invalid"))?;
            let key_bytes = std::fs::read(key_path)
                .map_err(|_| anyhow::anyhow!("OTLP client key is unavailable"))?;
            let key = rustls_pemfile::private_key(&mut BufReader::new(Cursor::new(key_bytes)))
                .map_err(|_| anyhow::anyhow!("OTLP client key is invalid"))?
                .ok_or_else(|| anyhow::anyhow!("OTLP client key is invalid"))?;
            builder
                .with_client_auth_cert(certificates, key)
                .map_err(|_| anyhow::anyhow!("OTLP client identity is invalid"))?
        }
        _ => builder.with_no_client_auth(),
    };
    config.alpn_protocols = vec![b"h2".to_vec()];
    Ok(config)
}

fn publish_observation(observation: AttemptObservation) {
    let _elapsed = observation.elapsed;
    let attrs = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::CONNECTION_PEER_TYPE,
        value: jackin_telemetry::Value::Str(ConnectionPeerType::Parallax.as_str()),
    }];
    let operation = jackin_telemetry::autonomous_root_operation(
        &jackin_telemetry::operation::CONNECTION_ATTEMPT,
        &attrs,
    );
    if let Ok(operation) = operation {
        operation.complete(observation.outcome, observation.error);
    }
}

#[derive(Debug)]
pub(super) enum PhysicalIo {
    Plain(tokio::net::TcpStream),
    Tls(Box<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>),
}

impl AsyncRead for PhysicalIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
            Self::Tls(stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for PhysicalIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        match &mut *self {
            Self::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            Self::Tls(stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Plain(stream) => Pin::new(stream).poll_flush(cx),
            Self::Tls(stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            Self::Tls(stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

#[cfg(test)]
mod tests;
