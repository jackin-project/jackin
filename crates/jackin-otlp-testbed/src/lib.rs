// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Deterministic, test-only OTLP/gRPC receiver.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use opentelemetry_proto::tonic::collector::logs::v1::{
    ExportLogsPartialSuccess, ExportLogsServiceRequest, ExportLogsServiceResponse,
    logs_service_server::{LogsService, LogsServiceServer},
};
use opentelemetry_proto::tonic::collector::metrics::v1::{
    ExportMetricsPartialSuccess, ExportMetricsServiceRequest, ExportMetricsServiceResponse,
    metrics_service_server::{MetricsService, MetricsServiceServer},
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTracePartialSuccess, ExportTraceServiceRequest, ExportTraceServiceResponse,
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
    /// Accept while reporting one rejected item through OTLP partial success.
    PartialSuccess,
    /// Hold a response to exercise exporter deadline behavior.
    Delay(std::time::Duration),
}

#[derive(Debug, Default)]
struct State {
    traces: Mutex<Vec<ExportTraceServiceRequest>>,
    logs: Mutex<Vec<ExportLogsServiceRequest>>,
    metrics: Mutex<Vec<ExportMetricsServiceRequest>>,
    behavior: Mutex<Behavior>,
    received: tokio::sync::Notify,
}

impl State {
    fn behavior(&self) -> Behavior {
        self.behavior
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    async fn apply(behavior: &Behavior) -> Result<(), Status> {
        match behavior {
            Behavior::Reject(code) => Err(Status::new(*code, "scripted OTLP testbed response")),
            Behavior::Delay(duration) => {
                tokio::time::sleep(*duration).await;
                Ok(())
            }
            Behavior::Ok | Behavior::PartialSuccess => Ok(()),
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
        self.0.received.notify_one();
        let behavior = self.0.behavior();
        State::apply(&behavior).await?;
        let partial_success =
            matches!(behavior, Behavior::PartialSuccess).then(|| ExportTracePartialSuccess {
                rejected_spans: 1,
                error_message: "scripted partial success".to_owned(),
            });
        Ok(Response::new(ExportTraceServiceResponse {
            partial_success,
        }))
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
        self.0.received.notify_one();
        let behavior = self.0.behavior();
        State::apply(&behavior).await?;
        let partial_success =
            matches!(behavior, Behavior::PartialSuccess).then(|| ExportLogsPartialSuccess {
                rejected_log_records: 1,
                error_message: "scripted partial success".to_owned(),
            });
        Ok(Response::new(ExportLogsServiceResponse { partial_success }))
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
        self.0.received.notify_one();
        let behavior = self.0.behavior();
        State::apply(&behavior).await?;
        let partial_success =
            matches!(behavior, Behavior::PartialSuccess).then(|| ExportMetricsPartialSuccess {
                rejected_data_points: 1,
                error_message: "scripted partial success".to_owned(),
            });
        Ok(Response::new(ExportMetricsServiceResponse {
            partial_success,
        }))
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
                .add_service(
                    TraceServiceServer::new(services.clone())
                        .accept_compressed(tonic::codec::CompressionEncoding::Gzip),
                )
                .add_service(
                    LogsServiceServer::new(services.clone())
                        .accept_compressed(tonic::codec::CompressionEncoding::Gzip),
                )
                .add_service(
                    MetricsServiceServer::new(services)
                        .accept_compressed(tonic::codec::CompressionEncoding::Gzip),
                )
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

    /// Decoded spans across all captured trace requests.
    #[must_use]
    pub fn spans(&self) -> Vec<opentelemetry_proto::tonic::trace::v1::Span> {
        self.traces()
            .into_iter()
            .flat_map(|request| request.resource_spans)
            .flat_map(|resource| resource.scope_spans)
            .flat_map(|scope| scope.spans)
            .collect()
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

    /// Decoded log records across all captured log requests.
    #[must_use]
    pub fn log_records(&self) -> Vec<opentelemetry_proto::tonic::logs::v1::LogRecord> {
        self.logs()
            .into_iter()
            .flat_map(|request| request.resource_logs)
            .flat_map(|resource| resource.scope_logs)
            .flat_map(|scope| scope.log_records)
            .collect()
    }

    /// Find a native OTLP event by its governed `EventName`.
    #[must_use]
    pub fn find_event(
        &self,
        name: &str,
    ) -> Option<opentelemetry_proto::tonic::logs::v1::LogRecord> {
        self.log_records()
            .into_iter()
            .find(|record| record.event_name == name)
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

    /// Decoded metric names across all captured metric requests.
    #[must_use]
    pub fn metric_names(&self) -> Vec<String> {
        self.metrics()
            .into_iter()
            .flat_map(|request| request.resource_metrics)
            .flat_map(|resource| resource.scope_metrics)
            .flat_map(|scope| scope.metrics)
            .map(|metric| metric.name)
            .collect()
    }

    /// Stop the receiver while retaining captured requests for assertions.
    pub fn stop(&mut self) {
        if let Some(shutdown) = self.shutdown.take()
            && shutdown.send(()).is_err()
        {}
    }

    /// Wait until at least one request for every signal has arrived.
    pub async fn wait_for_all_signals(&self, timeout: std::time::Duration) -> bool {
        tokio::time::timeout(timeout, async {
            loop {
                if !self.traces().is_empty()
                    && !self.logs().is_empty()
                    && !self.metrics().is_empty()
                {
                    return;
                }
                self.state.received.notified().await;
            }
        })
        .await
        .is_ok()
    }
}

impl Drop for Testbed {
    fn drop(&mut self) {
        self.stop();
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

        testbed.set_behavior(Behavior::PartialSuccess);
        let response = traces
            .export(ExportTraceServiceRequest::default())
            .await
            .expect("partial success is a successful gRPC response")
            .into_inner();
        assert_eq!(
            response
                .partial_success
                .map(|partial| partial.rejected_spans),
            Some(1)
        );
    }
}
