//! jackin-otlp-testbed: deterministic, test-only OTLP/gRPC receiver.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`Testbed`] — scripted three-signal wire receiver and assertions.

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::net::{Shutdown, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use futures_util::StreamExt;
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
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tonic::transport::{
    Server,
    server::{Connected, TcpConnectInfo, TcpIncoming},
};
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
    /// Accept only requests carrying the exact ASCII gRPC metadata entry.
    RequireHeader {
        /// Metadata key required on every signal request.
        name: &'static str,
        /// Exact metadata value required on every signal request.
        value: &'static str,
    },
}

#[derive(Debug, Default)]
struct State {
    traces: Mutex<Vec<ExportTraceServiceRequest>>,
    logs: Mutex<Vec<ExportLogsServiceRequest>>,
    metrics: Mutex<Vec<ExportMetricsServiceRequest>>,
    behavior: Mutex<Behavior>,
    received: tokio::sync::Notify,
    progress: tokio::sync::Notify,
    acknowledged: Mutex<Acknowledged>,
    force_requested: AtomicBool,
    force_shutdown: tokio::sync::Notify,
}

#[derive(Debug, Default)]
struct Acknowledged {
    traces: usize,
    logs: usize,
    metrics: usize,
}

#[derive(Debug, Default)]
struct ConnectionRegistry {
    next_id: AtomicUsize,
    state: Mutex<ConnectionRegistryState>,
    connected: tokio::sync::Notify,
    io_polled: AtomicBool,
    io_poll_changed: tokio::sync::Notify,
    changed: tokio::sync::Notify,
}

#[derive(Debug, Default)]
struct ConnectionRegistryState {
    closing: bool,
    sockets: HashMap<usize, std::net::TcpStream>,
}

/// Tonic detaches one task per accepted connection. Keep an independently
/// closable socket handle for every such task so forced shutdown can make the
/// task finish, then wait for this wrapper's drop as the task-lifecycle proof.
#[derive(Debug)]
struct TrackedIo {
    stream: TcpStream,
    registry: Arc<ConnectionRegistry>,
    id: usize,
}

impl ConnectionRegistry {
    fn register(self: &Arc<Self>, stream: TcpStream) -> io::Result<TrackedIo> {
        let std_stream = stream.into_std()?;
        let close_stream = std_stream.try_clone()?;
        let stream = TcpStream::from_std(std_stream)?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closing {
            drop(close_stream.shutdown(Shutdown::Both));
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "OTLP testbed is shutting down",
            ));
        }
        state.sockets.insert(id, close_stream);
        drop(state);
        self.connected.notify_waiters();
        Ok(TrackedIo {
            stream,
            registry: Arc::clone(self),
            id,
        })
    }

    fn force_close(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closing = true;
        for socket in state.sockets.values() {
            drop(socket.shutdown(Shutdown::Both));
        }
    }

    fn remove(&self, id: usize) {
        let removed = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .sockets
            .remove(&id)
            .is_some();
        if removed {
            self.changed.notify_waiters();
        }
    }

    fn active_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .sockets
            .len()
    }

    fn observe_io_poll(&self) {
        self.io_polled.store(true, Ordering::Release);
        self.io_poll_changed.notify_waiters();
    }

    async fn wait_until_empty(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.active_count() == 0 {
                return;
            }
            changed.await;
        }
    }

    #[cfg(test)]
    async fn wait_for_connection(&self, timeout: std::time::Duration) -> bool {
        tokio::time::timeout(timeout, async {
            loop {
                let connected = self.connected.notified();
                tokio::pin!(connected);
                connected.as_mut().enable();
                if self.active_count() > 0 {
                    return;
                }
                connected.await;
            }
        })
        .await
        .is_ok()
    }

    #[cfg(test)]
    async fn wait_for_io_poll(&self, timeout: std::time::Duration) -> bool {
        tokio::time::timeout(timeout, async {
            loop {
                let changed = self.io_poll_changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if self.io_polled.load(Ordering::Acquire) {
                    return;
                }
                changed.await;
            }
        })
        .await
        .is_ok()
    }
}

impl AsyncRead for TrackedIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.registry.observe_io_poll();
        Pin::new(&mut self.stream).poll_read(context, buffer)
    }
}

impl AsyncWrite for TrackedIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.registry.observe_io_poll();
        Pin::new(&mut self.stream).poll_write(context, buffer)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(context)
    }
}

impl Connected for TrackedIo {
    type ConnectInfo = TcpConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        self.stream.connect_info()
    }
}

impl Drop for TrackedIo {
    fn drop(&mut self) {
        self.registry.remove(self.id);
    }
}

#[derive(Clone, Copy)]
enum Signal {
    Traces,
    Logs,
    Metrics,
}

impl State {
    fn behavior(&self) -> Behavior {
        self.behavior
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn acknowledge(&self, signal: Signal) {
        let mut acknowledged = self
            .acknowledged
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match signal {
            Signal::Traces => acknowledged.traces += 1,
            Signal::Logs => acknowledged.logs += 1,
            Signal::Metrics => acknowledged.metrics += 1,
        }
        drop(acknowledged);
        self.progress.notify_waiters();
    }

    fn all_signals_acknowledged(&self) -> bool {
        let acknowledged = self
            .acknowledged
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let traces = self
            .traces
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len();
        let logs = self
            .logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len();
        let metrics = self
            .metrics
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len();
        traces > 0
            && logs > 0
            && metrics > 0
            && acknowledged.traces == traces
            && acknowledged.logs == logs
            && acknowledged.metrics == metrics
    }

    fn all_trace_requests_acknowledged(&self) -> bool {
        let acknowledged = self
            .acknowledged
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let traces = self
            .traces
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len();
        traces > 0 && acknowledged.traces == traces
    }

    fn request_forced_shutdown(&self) {
        self.force_requested.store(true, Ordering::Release);
        self.force_shutdown.notify_waiters();
    }

    async fn apply(&self, behavior: &Behavior) -> Result<(), Status> {
        if self.force_requested.load(Ordering::Acquire) {
            return Err(Status::cancelled("OTLP testbed forced shutdown"));
        }
        match behavior {
            Behavior::Reject(code) => Err(Status::new(*code, "scripted OTLP testbed response")),
            Behavior::Delay(duration) => {
                let forced = self.force_shutdown.notified();
                tokio::pin!(forced);
                forced.as_mut().enable();
                if self.force_requested.load(Ordering::Acquire) {
                    return Err(Status::cancelled("OTLP testbed forced shutdown"));
                }
                tokio::select! {
                    () = tokio::time::sleep(*duration) => Ok(()),
                    () = &mut forced => Err(Status::cancelled("OTLP testbed forced shutdown")),
                }
            }
            Behavior::Ok | Behavior::PartialSuccess | Behavior::RequireHeader { .. } => Ok(()),
        }
    }

    fn authenticate(
        behavior: &Behavior,
        metadata: &tonic::metadata::MetadataMap,
    ) -> Result<(), Status> {
        let Behavior::RequireHeader { name, value } = behavior else {
            return Ok(());
        };
        if metadata.get(*name).and_then(|actual| actual.to_str().ok()) == Some(*value) {
            Ok(())
        } else {
            Err(Status::unauthenticated(
                "required OTLP testbed metadata missing",
            ))
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
        let behavior = self.0.behavior();
        State::authenticate(&behavior, request.metadata())?;
        self.0
            .traces
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(request.into_inner());
        self.0.received.notify_waiters();
        self.0.apply(&behavior).await?;
        self.0.acknowledge(Signal::Traces);
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
        let behavior = self.0.behavior();
        State::authenticate(&behavior, request.metadata())?;
        self.0
            .logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(request.into_inner());
        self.0.received.notify_waiters();
        self.0.apply(&behavior).await?;
        self.0.acknowledge(Signal::Logs);
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
        let behavior = self.0.behavior();
        State::authenticate(&behavior, request.metadata())?;
        self.0
            .metrics
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(request.into_inner());
        self.0.received.notify_waiters();
        self.0.apply(&behavior).await?;
        self.0.acknowledge(Signal::Metrics);
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
    connections: Arc<ConnectionRegistry>,
    shutdown: Option<oneshot::Sender<()>>,
    receiver_task: Option<JoinHandle<Result<(), tonic::transport::Error>>>,
}

/// Failure while stopping the receiver after requesting graceful shutdown.
#[derive(Debug)]
pub enum ShutdownError {
    /// The gRPC server returned an error while shutting down.
    Server(tonic::transport::Error),
    /// The receiver task was cancelled or panicked before it could finish.
    Join(tokio::task::JoinError),
    /// Graceful shutdown exceeded its bounded wait and the receiver was aborted.
    Timeout,
}

impl fmt::Display for ShutdownError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Server(error) => {
                write!(formatter, "OTLP testbed server shutdown failed: {error}")
            }
            Self::Join(error) => write!(formatter, "OTLP testbed receiver task failed: {error}"),
            Self::Timeout => write!(
                formatter,
                "OTLP testbed graceful shutdown timed out; receiver task aborted"
            ),
        }
    }
}

impl std::error::Error for ShutdownError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Server(error) => Some(error),
            Self::Join(error) => Some(error),
            Self::Timeout => None,
        }
    }
}

const RECEIVER_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

impl Testbed {
    /// Start all three OTLP services on a random localhost port.
    pub fn start() -> io::Result<Self> {
        let incoming = TcpIncoming::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
        let addr = incoming.local_addr()?;
        let state = Arc::new(State::default());
        let connections = Arc::new(ConnectionRegistry::default());
        let tracked_incoming = incoming.map({
            let connections = Arc::clone(&connections);
            move |result| result.and_then(|stream| connections.register(stream))
        });
        let services = Services(Arc::clone(&state));
        let (shutdown, shutdown_rx) = oneshot::channel();
        let receiver_task =
            jackin_telemetry::spawn::spawn_stream("otlp-testbed.receiver", async move {
                Server::builder()
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
                    .serve_with_incoming_shutdown(tracked_incoming, async {
                        drop(shutdown_rx.await);
                    })
                    .await
            });
        Ok(Self {
            addr,
            state,
            connections,
            shutdown: Some(shutdown),
            receiver_task: Some(receiver_task),
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

    /// Decoded attribute keys across every metric datapoint and exemplar.
    #[must_use]
    pub fn metric_dimension_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        for metric in self
            .metrics()
            .into_iter()
            .flat_map(|request| request.resource_metrics)
            .flat_map(|resource| resource.scope_metrics)
            .flat_map(|scope| scope.metrics)
        {
            visit_metric_points(metric.data.as_ref(), |attributes, exemplars| {
                keys.extend(attributes.iter().map(|attribute| attribute.key.clone()));
                keys.extend(exemplars.iter().flat_map(|exemplar| {
                    exemplar
                        .filtered_attributes
                        .iter()
                        .map(|attribute| attribute.key.clone())
                }));
            });
        }
        keys
    }

    /// Report forbidden backend/product namespaces anywhere in decoded OTLP.
    #[must_use]
    pub fn legacy_namespace_violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        for request in self.traces() {
            for resource in &request.resource_spans {
                scan_resource(resource.resource.as_ref(), &mut violations);
                scan_name(&resource.schema_url, &mut violations);
                for scope in &resource.scope_spans {
                    scan_scope(scope.scope.as_ref(), &scope.schema_url, &mut violations);
                }
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
                scan_name(&resource.schema_url, &mut violations);
                for scope in &resource.scope_logs {
                    scan_scope(scope.scope.as_ref(), &scope.schema_url, &mut violations);
                }
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
                scan_name(&resource.schema_url, &mut violations);
                for scope in &resource.scope_metrics {
                    scan_scope(scope.scope.as_ref(), &scope.schema_url, &mut violations);
                }
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
                scan_text(&resource.schema_url, prohibited, &mut violations);
                scan_values(
                    resource
                        .resource
                        .as_ref()
                        .map(|value| value.attributes.as_slice()),
                    prohibited,
                    &mut violations,
                );
                for scope in &resource.scope_spans {
                    scan_scope_values(
                        scope.scope.as_ref(),
                        &scope.schema_url,
                        prohibited,
                        &mut violations,
                    );
                }
                for span in resource.scope_spans.iter().flat_map(|scope| &scope.spans) {
                    scan_span_values(span, prohibited, &mut violations);
                }
            }
        }
        for request in self.logs() {
            for resource in &request.resource_logs {
                scan_text(&resource.schema_url, prohibited, &mut violations);
                scan_values(
                    resource
                        .resource
                        .as_ref()
                        .map(|value| value.attributes.as_slice()),
                    prohibited,
                    &mut violations,
                );
                for scope in &resource.scope_logs {
                    scan_scope_values(
                        scope.scope.as_ref(),
                        &scope.schema_url,
                        prohibited,
                        &mut violations,
                    );
                }
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
                scan_text(&resource.schema_url, prohibited, &mut violations);
                scan_values(
                    resource
                        .resource
                        .as_ref()
                        .map(|value| value.attributes.as_slice()),
                    prohibited,
                    &mut violations,
                );
                for scope in &resource.scope_metrics {
                    scan_scope_values(
                        scope.scope.as_ref(),
                        &scope.schema_url,
                        prohibited,
                        &mut violations,
                    );
                }
                for metric in resource
                    .scope_metrics
                    .iter()
                    .flat_map(|scope| &scope.metrics)
                {
                    scan_text(&metric.name, prohibited, &mut violations);
                    scan_text(&metric.description, prohibited, &mut violations);
                    scan_text(&metric.unit, prohibited, &mut violations);
                    scan_metric_point_values(metric.data.as_ref(), prohibited, &mut violations);
                }
            }
        }
        violations
    }

    /// Gracefully stop the receiver and join its task.
    pub async fn shutdown(&mut self) -> Result<(), ShutdownError> {
        if let Some(shutdown) = self.shutdown.take()
            && shutdown.send(()).is_err()
        {}
        let result = if let Some(receiver_task) = self.receiver_task.as_mut() {
            tokio::time::timeout(RECEIVER_SHUTDOWN_TIMEOUT, receiver_task).await
        } else {
            return Ok(());
        };

        if result.is_err() {
            self.state.request_forced_shutdown();
            self.connections.force_close();
            self.abort_receiver_task().await;
            if tokio::time::timeout(
                RECEIVER_SHUTDOWN_TIMEOUT,
                self.connections.wait_until_empty(),
            )
            .await
            .is_err()
            {
                return Err(ShutdownError::Timeout);
            }
            return Err(ShutdownError::Timeout);
        }
        self.receiver_task.take();
        match result {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(error))) => Err(ShutdownError::Server(error)),
            Ok(Err(error)) => Err(ShutdownError::Join(error)),
            Err(_) => unreachable!("timed-out receiver task handled above"),
        }
    }

    #[cfg(test)]
    async fn wait_for_trace_request(&self, timeout: std::time::Duration) -> bool {
        tokio::time::timeout(timeout, async {
            loop {
                let received = self.state.received.notified();
                tokio::pin!(received);
                received.as_mut().enable();
                if !self.traces().is_empty() {
                    return;
                }
                received.await;
            }
        })
        .await
        .is_ok()
    }

    /// Wait until at least one request for every signal has been acknowledged.
    pub async fn wait_for_all_signals(&self, timeout: std::time::Duration) -> bool {
        tokio::time::timeout(timeout, async {
            loop {
                let progress = self.state.progress.notified();
                tokio::pin!(progress);
                progress.as_mut().enable();
                if self.state.all_signals_acknowledged() {
                    return;
                }
                progress.await;
            }
        })
        .await
        .is_ok()
    }

    /// Wait until at least `count` spans with the exact wire name have arrived.
    ///
    /// Signal-level readiness is insufficient when one flush produces multiple
    /// trace export requests: an earlier child-span batch can arrive before the
    /// later root-span batch. This also waits for every captured request's ACK.
    pub async fn wait_for_span_count(
        &self,
        name: &str,
        count: usize,
        timeout: std::time::Duration,
    ) -> bool {
        if count == 0 {
            return true;
        }
        tokio::time::timeout(timeout, async {
            loop {
                let progress = self.state.progress.notified();
                tokio::pin!(progress);
                progress.as_mut().enable();
                if self.state.all_trace_requests_acknowledged()
                    && self.spans().iter().filter(|span| span.name == name).count() >= count
                {
                    return;
                }
                progress.await;
            }
        })
        .await
        .is_ok()
    }

    async fn abort_receiver_task(&mut self) {
        if let Some(receiver_task) = self.receiver_task.take() {
            receiver_task.abort();
            drop(receiver_task.await);
        }
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
    for link in &span.links {
        scan_values(Some(&link.attributes), prohibited, violations);
    }
    if let Some(status) = &span.status {
        scan_text(&status.message, prohibited, violations);
    }
}

fn scan_scope(
    scope: Option<&opentelemetry_proto::tonic::common::v1::InstrumentationScope>,
    schema_url: &str,
    violations: &mut Vec<String>,
) {
    scan_name(schema_url, violations);
    if let Some(scope) = scope {
        scan_name(&scope.name, violations);
        scan_name(&scope.version, violations);
        scan_attributes(&scope.attributes, violations);
    }
}

fn scan_scope_values(
    scope: Option<&opentelemetry_proto::tonic::common::v1::InstrumentationScope>,
    schema_url: &str,
    prohibited: &[&str],
    violations: &mut Vec<String>,
) {
    scan_text(schema_url, prohibited, violations);
    if let Some(scope) = scope {
        scan_text(&scope.name, prohibited, violations);
        scan_text(&scope.version, prohibited, violations);
        scan_values(Some(&scope.attributes), prohibited, violations);
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
    visit_metric_points(data, |attributes, exemplars| {
        scan_attributes(attributes, violations);
        for exemplar in exemplars {
            scan_attributes(&exemplar.filtered_attributes, violations);
        }
    });
}

fn scan_metric_point_values(
    data: Option<&opentelemetry_proto::tonic::metrics::v1::metric::Data>,
    prohibited: &[&str],
    violations: &mut Vec<String>,
) {
    visit_metric_points(data, |attributes, exemplars| {
        scan_values(Some(attributes), prohibited, violations);
        for exemplar in exemplars {
            scan_values(Some(&exemplar.filtered_attributes), prohibited, violations);
        }
    });
}

fn visit_metric_points(
    data: Option<&opentelemetry_proto::tonic::metrics::v1::metric::Data>,
    mut visit: impl FnMut(
        &[opentelemetry_proto::tonic::common::v1::KeyValue],
        &[opentelemetry_proto::tonic::metrics::v1::Exemplar],
    ),
) {
    use opentelemetry_proto::tonic::metrics::v1::metric::Data;

    match data {
        Some(Data::Gauge(value)) => {
            for point in &value.data_points {
                visit(&point.attributes, &point.exemplars);
            }
        }
        Some(Data::Sum(value)) => {
            for point in &value.data_points {
                visit(&point.attributes, &point.exemplars);
            }
        }
        Some(Data::Histogram(value)) => {
            for point in &value.data_points {
                visit(&point.attributes, &point.exemplars);
            }
        }
        Some(Data::ExponentialHistogram(value)) => {
            for point in &value.data_points {
                visit(&point.attributes, &point.exemplars);
            }
        }
        Some(Data::Summary(value)) => {
            for point in &value.data_points {
                visit(&point.attributes, &[]);
            }
        }
        None => {}
    }
}

impl Drop for Testbed {
    fn drop(&mut self) {
        self.state.request_forced_shutdown();
        self.connections.force_close();
        if let Some(shutdown) = self.shutdown.take()
            && shutdown.send(()).is_err()
        {}
        if let Some(receiver_task) = self.receiver_task.take() {
            receiver_task.abort();
        }
    }
}

#[cfg(test)]
mod tests;
