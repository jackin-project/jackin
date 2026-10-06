// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Provider connection layer and HTTP client.

use std::future::Future;

use super::PROVIDER_HTTP_TIMEOUT;

#[derive(Clone)]
pub(crate) struct ProviderConnectionLayer {
    dispatcher: tracing::Dispatch,
}

impl ProviderConnectionLayer {
    pub(crate) fn capture() -> Self {
        Self {
            dispatcher: tracing::dispatcher::get_default(Clone::clone),
        }
    }
}

impl<S> tower::Layer<S> for ProviderConnectionLayer {
    type Service = ProviderConnectionService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ProviderConnectionService {
            inner,
            dispatcher: self.dispatcher.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ProviderConnectionService<S> {
    inner: S,
    dispatcher: tracing::Dispatch,
}

impl<S, Request> tower::Service<Request> for ProviderConnectionService<S>
where
    S: tower::Service<Request> + Send,
    S::Future: Send + 'static,
    S::Response: 'static,
    S::Error: 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>,
    >;

    fn poll_ready(
        &mut self,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let operation = tracing::dispatcher::with_default(&self.dispatcher, || {
            jackin_telemetry::operation_or_disabled(
                &jackin_telemetry::operation::CONNECTION_ATTEMPT,
                &[jackin_telemetry::Attr {
                    key: jackin_telemetry::schema::attrs::CONNECTION_PEER_TYPE,
                    value: jackin_telemetry::Value::Str(
                        jackin_telemetry::schema::enums::ConnectionPeerType::Provider.as_str(),
                    ),
                }],
            )
        });
        let future = self.inner.call(request);
        Box::pin(async move {
            let result = future.await;
            operation.complete(
                if result.is_ok() {
                    jackin_telemetry::schema::enums::OutcomeValue::Success
                } else {
                    jackin_telemetry::schema::enums::OutcomeValue::Error
                },
                result
                    .as_ref()
                    .err()
                    .map(|_| jackin_telemetry::schema::enums::ErrorType::IoError),
            );
            result
        })
    }
}

pub(crate) fn parse_chatgpt_base_url(contents: &str) -> Option<String> {
    for raw_line in contents.lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "chatgpt_base_url" {
            continue;
        }
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim()
            .to_owned();
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

pub(crate) fn provider_http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(PROVIDER_HTTP_TIMEOUT)
        .connect_timeout(PROVIDER_HTTP_TIMEOUT)
        .connector_layer(ProviderConnectionLayer::capture())
        .build()
        .map_err(|err| format!("provider HTTP client unavailable: {err}"))
}
