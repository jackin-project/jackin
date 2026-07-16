// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Deterministic, test-only OTLP/gRPC receiver.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use opentelemetry_proto::tonic::collector::logs::v1::{
    ExportLogsServiceRequest, ExportLogsServiceResponse,
    logs_service_server::{LogsService, LogsServiceServer},
};
use opentelemetry_proto::tonic::collector::metrics::v1::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
    metrics_service_server::{MetricsService, MetricsServiceServer},
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
    trace_service_server::{TraceService, TraceServiceServer},
};
use tokio::sync::oneshot;
use tonic::transport::{Server, server::TcpIncoming};
use tonic::{Request, Response, Status};

/// Scripted response applied independently to every export request.
#[derive(Clone, Debug, Default)]
pub enum Behavior {
    /// Accept and record the request.
    #[default]
    Ok,
    /// Reject with the supplied gRPC status code.
    Reject(tonic::Code),
}

#[derive(Debug, Default)]
struct State {
    traces: Mutex<Vec<ExportTraceServiceRequest>>,
    logs: Mutex<Vec<ExportLogsServiceRequest>>,
    metrics: Mutex<Vec<ExportMetricsServiceRequest>>,
    behavior: Mutex<Behavior>,
}

impl State {
    fn result<T: Default>(&self) -> Result<Response<T>, Status> {
        match *self
            .behavior
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            Behavior::Ok => Ok(Response::new(T::default())),
            Behavior::Reject(code) => Err(Status::new(code, "scripted OTLP testbed response")),
        }
    }
}

#[derive(Clone, Debug)]
struct Services(Arc<State>);

#[tonic::async_trait]
impl TraceService for Services {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        self.0
            .traces
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(request.into_inner());
        self.0.result()
    }
}

#[tonic::async_trait]
impl LogsService for Services {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        self.0
            .logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(request.into_inner());
        self.0.result()
    }
}

#[tonic::async_trait]
impl MetricsService for Services {
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> Result<Response<ExportMetricsServiceResponse>, Status> {
        self.0
            .metrics
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(request.into_inner());
        self.0.result()
    }
}

/// Running receiver and its typed captured-request accessors.
#[derive(Debug)]
pub struct Testbed {
    addr: SocketAddr,
    state: Arc<State>,
    shutdown: Option<oneshot::Sender<()>>,
}

impl Testbed {
    /// Start all three OTLP services on a random localhost port.
    pub fn start() -> std::io::Result<Self> {
        let incoming = TcpIncoming::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
        let addr = incoming.local_addr()?;
        let state = Arc::new(State::default());
        let services = Services(Arc::clone(&state));
        let (shutdown, shutdown_rx) = oneshot::channel();
        tokio::spawn(async move {
            let result = Server::builder()
                .add_service(TraceServiceServer::new(services.clone()))
                .add_service(LogsServiceServer::new(services.clone()))
                .add_service(MetricsServiceServer::new(services))
                .serve_with_incoming_shutdown(incoming, async { drop(shutdown_rx.await) })
                .await;
            assert!(result.is_ok(), "OTLP testbed server failed: {result:?}");
        });
        Ok(Self {
            addr,
            state,
            shutdown: Some(shutdown),
        })
    }

    /// Endpoint accepted by the OTLP exporter.
    #[must_use]
    pub fn endpoint(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Replace the deterministic response behavior.
    pub fn set_behavior(&self, behavior: Behavior) {
        *self
            .state
            .behavior
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = behavior;
    }

    /// Captured trace requests.
    #[must_use]
    pub fn traces(&self) -> Vec<ExportTraceServiceRequest> {
        self.state
            .traces
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Captured log requests.
    #[must_use]
    pub fn logs(&self) -> Vec<ExportLogsServiceRequest> {
        self.state
            .logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Captured metric requests.
    #[must_use]
    pub fn metrics(&self) -> Vec<ExportMetricsServiceRequest> {
        self.state
            .metrics
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Drop for Testbed {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take()
            && shutdown.send(()).is_err()
        {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn serves_all_three_otlp_services() {
        let testbed = Testbed::start().expect("start testbed");
        assert!(testbed.endpoint().starts_with("http://127.0.0.1:"));

        let mut traces = opentelemetry_proto::tonic::collector::trace::v1::
            trace_service_client::TraceServiceClient::connect(testbed.endpoint())
            .await
            .expect("connect trace client");
        traces
            .export(ExportTraceServiceRequest::default())
            .await
            .expect("export traces");
        let mut logs = opentelemetry_proto::tonic::collector::logs::v1::
            logs_service_client::LogsServiceClient::connect(testbed.endpoint())
            .await
            .expect("connect logs client");
        logs.export(ExportLogsServiceRequest::default())
            .await
            .expect("export logs");
        let mut metrics = opentelemetry_proto::tonic::collector::metrics::v1::
            metrics_service_client::MetricsServiceClient::connect(testbed.endpoint())
            .await
            .expect("connect metrics client");
        metrics
            .export(ExportMetricsServiceRequest::default())
            .await
            .expect("export metrics");
        assert_eq!(testbed.traces().len(), 1);
        assert_eq!(testbed.logs().len(), 1);
        assert_eq!(testbed.metrics().len(), 1);

        testbed.set_behavior(Behavior::Reject(tonic::Code::Unavailable));
        let error = traces
            .export(ExportTraceServiceRequest::default())
            .await
            .expect_err("scripted rejection");
        assert_eq!(error.code(), tonic::Code::Unavailable);
    }
}
