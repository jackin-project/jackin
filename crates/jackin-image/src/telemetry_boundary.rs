// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Privacy-safe telemetry helpers for image cache and download boundaries.

use std::future::Future;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DownloadRoute {
    AgentMetadata,
    AgentArtifact,
    CapsuleArtifact,
    CapsuleManifest,
    CapsuleManifestBundle,
}

impl DownloadRoute {
    const fn template(self) -> &'static str {
        match self {
            Self::AgentMetadata => "/agent-binaries/{version}/metadata",
            Self::AgentArtifact => "/agent-binaries/{version}/{artifact}",
            Self::CapsuleArtifact => "/releases/download/{version}/{artifact}",
            Self::CapsuleManifest => "/releases/download/{version}/capsule-manifest.json",
            Self::CapsuleManifestBundle => {
                "/releases/download/{version}/capsule-manifest.json.bundle"
            }
        }
    }
}

fn known_server(url: &str) -> Option<&'static str> {
    [
        ("https://api.github.com/", "api.github.com"),
        ("https://github.com/", "github.com"),
        ("https://downloads.claude.ai/", "downloads.claude.ai"),
        ("https://static.ampcode.com/", "static.ampcode.com"),
        ("https://code.kimi.com/", "code.kimi.com"),
        ("https://x.ai/", "x.ai"),
        ("https://storage.googleapis.com/", "storage.googleapis.com"),
    ]
    .into_iter()
    .find_map(|(prefix, server)| url.starts_with(prefix).then_some(server))
}

pub(crate) async fn download_request<T>(
    route: DownloadRoute,
    url: &str,
    future: impl Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<T> {
    let mut attrs = vec![
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::HTTP_REQUEST_METHOD,
            value: jackin_telemetry::Value::Str("GET"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::URL_TEMPLATE,
            value: jackin_telemetry::Value::Str(route.template()),
        },
    ];
    if let Some(server) = known_server(url) {
        attrs.push(jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::SERVER_ADDRESS,
            value: jackin_telemetry::Value::Str(server),
        });
    }
    let operation =
        jackin_telemetry::operation_or_disabled(&jackin_telemetry::operation::HTTP_CLIENT, &attrs);
    let result = future.await;
    operation.complete(
        if result.is_ok() {
            jackin_telemetry::schema::enums::OutcomeValue::Success
        } else {
            jackin_telemetry::schema::enums::OutcomeValue::Failure
        },
        result
            .as_ref()
            .err()
            .map(|_| jackin_telemetry::schema::enums::ErrorType::HttpError),
    );
    result
}

pub(crate) fn cache_decision(
    name: jackin_telemetry::schema::enums::CacheName,
    result: jackin_telemetry::schema::enums::CacheResult,
) {
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::CACHE_NAME,
            value: jackin_telemetry::Value::Str(name.as_str()),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::CACHE_RESULT,
            value: jackin_telemetry::Value::Str(result.as_str()),
        },
    ];
    let _result = jackin_telemetry::emit_event(
        &jackin_telemetry::event::CACHE_DECISION,
        jackin_telemetry::FieldSet::new(&attrs, None),
    );
}
