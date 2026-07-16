//! jackin-otlp-testbed: deterministic, test-only OTLP/gRPC receiver.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`Testbed`] — scripted three-signal wire receiver and assertions.

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

    /// Report forbidden backend/product namespaces anywhere in decoded OTLP.
    #[must_use]
    pub fn legacy_namespace_violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        for request in self.traces() {
            for resource in &request.resource_spans {
                scan_resource(resource.resource.as_ref(), &mut violations);
            }
            for span in request
                .resource_spans
                .iter()
                .flat_map(|resource| &resource.scope_spans)
                .flat_map(|scope| &scope.spans)
            {
                scan_name(&span.name, &mut violations);
                scan_attributes(&span.attributes, &mut violations);
                for event in &span.events {
                    scan_name(&event.name, &mut violations);
                    scan_attributes(&event.attributes, &mut violations);
                }
                for link in &span.links {
                    scan_attributes(&link.attributes, &mut violations);
                }
            }
        }
        for request in self.logs() {
            for resource in &request.resource_logs {
                scan_resource(resource.resource.as_ref(), &mut violations);
            }
            for record in request
                .resource_logs
                .iter()
                .flat_map(|resource| &resource.scope_logs)
                .flat_map(|scope| &scope.log_records)
            {
                scan_name(&record.event_name, &mut violations);
                scan_attributes(&record.attributes, &mut violations);
            }
        }
        for request in self.metrics() {
            for resource in &request.resource_metrics {
                scan_resource(resource.resource.as_ref(), &mut violations);
            }
            for metric in request
                .resource_metrics
                .iter()
                .flat_map(|resource| &resource.scope_metrics)
                .flat_map(|scope| &scope.metrics)
            {
                scan_name(&metric.name, &mut violations);
                scan_metric_points(metric.data.as_ref(), &mut violations);
            }
        }
        violations
    }

    /// Report captured string fields containing any prohibited fixture value.
    #[must_use]
    pub fn prohibited_value_violations(&self, prohibited: &[&str]) -> Vec<String> {
        let mut violations = Vec::new();
        for request in self.traces() {
            for resource in &request.resource_spans {
                scan_values(
                    resource
                        .resource
                        .as_ref()
                        .map(|value| value.attributes.as_slice()),
                    prohibited,
                    &mut violations,
                );
                for span in resource.scope_spans.iter().flat_map(|scope| &scope.spans) {
                    scan_span_values(span, prohibited, &mut violations);
                }
            }
        }
        for request in self.logs() {
            for resource in &request.resource_logs {
                scan_values(
                    resource
                        .resource
                        .as_ref()
                        .map(|value| value.attributes.as_slice()),
                    prohibited,
                    &mut violations,
                );
                for record in resource
                    .scope_logs
                    .iter()
                    .flat_map(|scope| &scope.log_records)
                {
                    scan_text(&record.event_name, prohibited, &mut violations);
                    scan_values(Some(&record.attributes), prohibited, &mut violations);
                    scan_any_value(record.body.as_ref(), prohibited, &mut violations);
                }
            }
        }
        for request in self.metrics() {
            for resource in &request.resource_metrics {
                scan_values(
                    resource
                        .resource
                        .as_ref()
                        .map(|value| value.attributes.as_slice()),
                    prohibited,
                    &mut violations,
                );
            }
        }
        violations
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

fn scan_span_values(
    span: &opentelemetry_proto::tonic::trace::v1::Span,
    prohibited: &[&str],
    violations: &mut Vec<String>,
) {
    scan_text(&span.name, prohibited, violations);
    scan_values(Some(&span.attributes), prohibited, violations);
    for event in &span.events {
        scan_text(&event.name, prohibited, violations);
        scan_values(Some(&event.attributes), prohibited, violations);
    }
    if let Some(status) = &span.status {
        scan_text(&status.message, prohibited, violations);
    }
}

fn scan_resource(
    resource: Option<&opentelemetry_proto::tonic::resource::v1::Resource>,
    violations: &mut Vec<String>,
) {
    if let Some(resource) = resource {
        scan_attributes(&resource.attributes, violations);
    }
}

fn scan_attributes(
    attributes: &[opentelemetry_proto::tonic::common::v1::KeyValue],
    violations: &mut Vec<String>,
) {
    for attribute in attributes {
        scan_name(&attribute.key, violations);
    }
}

fn scan_values(
    attributes: Option<&[opentelemetry_proto::tonic::common::v1::KeyValue]>,
    prohibited: &[&str],
    violations: &mut Vec<String>,
) {
    if let Some(attributes) = attributes {
        for attribute in attributes {
            scan_text(&attribute.key, prohibited, violations);
            scan_any_value(attribute.value.as_ref(), prohibited, violations);
        }
    }
}

fn scan_any_value(
    value: Option<&opentelemetry_proto::tonic::common::v1::AnyValue>,
    prohibited: &[&str],
    violations: &mut Vec<String>,
) {
    use opentelemetry_proto::tonic::common::v1::any_value::Value;
    match value.and_then(|value| value.value.as_ref()) {
        Some(Value::StringValue(value)) => scan_text(value, prohibited, violations),
        Some(Value::ArrayValue(value)) => {
            for value in &value.values {
                scan_any_value(Some(value), prohibited, violations);
            }
        }
        Some(Value::KvlistValue(value)) => scan_values(Some(&value.values), prohibited, violations),
        _ => {}
    }
}

fn scan_text(text: &str, prohibited: &[&str], violations: &mut Vec<String>) {
    for value in prohibited {
        if !value.is_empty() && text.contains(value) {
            violations.push((*value).to_owned());
        }
    }
}

fn scan_name(name: &str, violations: &mut Vec<String>) {
    let legacy_namespace = |prefix: &str| {
        name.strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('.'))
    };
    if legacy_namespace("jackin") || legacy_namespace("parallax") {
        violations.push(name.to_owned());
    }
}

fn scan_metric_points(
    data: Option<&opentelemetry_proto::tonic::metrics::v1::metric::Data>,
    violations: &mut Vec<String>,
) {
    use opentelemetry_proto::tonic::metrics::v1::metric::Data;
    match data {
        Some(Data::Gauge(value)) => {
            for point in &value.data_points {
                scan_attributes(&point.attributes, violations);
            }
        }
        Some(Data::Sum(value)) => {
            for point in &value.data_points {
                scan_attributes(&point.attributes, violations);
            }
        }
        Some(Data::Histogram(value)) => {
            for point in &value.data_points {
                scan_attributes(&point.attributes, violations);
            }
        }
        Some(Data::ExponentialHistogram(value)) => {
            for point in &value.data_points {
                scan_attributes(&point.attributes, violations);
            }
        }
        Some(Data::Summary(value)) => {
            for point in &value.data_points {
                scan_attributes(&point.attributes, violations);
            }
        }
        None => {}
    }
}

impl Drop for Testbed {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests;
