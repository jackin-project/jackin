// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Provider HTTP requests, errors, and retry parsing.

use std::time::UNIX_EPOCH;

use super::{now_epoch, provider_http_client};

pub(crate) fn provider_request<T, E>(
    provider: jackin_telemetry::schema::enums::ProviderName,
    method: &'static str,
    template: &'static str,
    request: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::GEN_AI_PROVIDER_NAME,
            value: jackin_telemetry::Value::Str(provider.as_str()),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::HTTP_REQUEST_METHOD,
            value: jackin_telemetry::Value::Str(method),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::URL_TEMPLATE,
            value: jackin_telemetry::Value::Str(template),
        },
    ];
    let operation =
        jackin_telemetry::operation_or_disabled(&jackin_telemetry::operation::HTTP_CLIENT, &attrs);
    let result = request();
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

/// Failure classes preserved by the shared bearer-auth JSON fetcher.
///
/// Provider snapshots may map an HTTP auth status to `NeedsLogin`, but a
/// transport or decode failure must remain an ordinary provider error even if
/// its rendered message happens to contain the same digits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProviderHttpError {
    Transport(String),
    HttpStatus {
        status: u16,
        message: String,
        retry_after_seconds: Option<u64>,
        response_received_at_epoch: Option<i64>,
    },
    Decode(String),
}

impl std::fmt::Display for ProviderHttpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(message) | Self::HttpStatus { message, .. } | Self::Decode(message) => {
                formatter.write_str(message)
            }
        }
    }
}

pub(crate) fn retry_after_header_seconds(
    headers: &reqwest::header::HeaderMap,
    response_received_at_epoch: i64,
) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| retry_after_header_value(value, response_received_at_epoch))
}

pub(crate) fn retry_after_header_value(
    value: &str,
    response_received_at_epoch: i64,
) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(seconds);
    }
    let response_epoch = httpdate::parse_http_date(value)
        .ok()?
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        });
    Some(
        u64::try_from(
            response_epoch
                .saturating_sub(response_received_at_epoch)
                .max(0),
        )
        .unwrap_or_default(),
    )
}

/// Shared GET → bearer-auth → JSON skeleton for provider quota endpoints. The
/// caller supplies the human label (used verbatim in every error string so the
/// per-provider wording is unchanged), the URL, the bearer token, and any extra
/// request headers beyond the always-sent `Accept: application/json`. Per-
/// provider response validation stays at the call site.
pub(crate) fn get_json_bearer<T: serde::de::DeserializeOwned>(
    provider: jackin_telemetry::schema::enums::ProviderName,
    template: &'static str,
    label: &str,
    url: &str,
    token: &str,
    extra_headers: &[(reqwest::header::HeaderName, &str)],
) -> Result<T, ProviderHttpError> {
    provider_request(provider, "GET", template, || {
        let client = provider_http_client().map_err(ProviderHttpError::Transport)?;
        let mut request = client
            .get(url)
            .bearer_auth(token)
            .header(reqwest::header::ACCEPT, "application/json");
        for (name, value) in extra_headers {
            request = request.header(name.clone(), *value);
        }
        let response = request.send().map_err(|err| {
            ProviderHttpError::Transport(format!("{label} request failed: {err}"))
        })?;
        let response_received_at_epoch = now_epoch();
        let status = response.status();
        let retry_after_seconds =
            retry_after_header_seconds(response.headers(), response_received_at_epoch);
        if !status.is_success() {
            return Err(ProviderHttpError::HttpStatus {
                status: status.as_u16(),
                message: format!("{label} HTTP {status}"),
                retry_after_seconds,
                response_received_at_epoch: Some(response_received_at_epoch),
            });
        }
        response
            .json::<T>()
            .map_err(|err| ProviderHttpError::Decode(format!("{label} decode failed: {err}")))
    })
}

pub(crate) fn epoch_seconds_from_maybe_ms(value: i64) -> i64 {
    if value > 1_000_000_000_000 {
        value / 1000
    } else {
        value
    }
}

pub(crate) fn normalize_url_or_host(value: &str, suffix: &str) -> String {
    let mut cleaned = value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_owned();
    if !cleaned.starts_with("http://") && !cleaned.starts_with("https://") {
        cleaned = format!("https://{cleaned}");
    }
    if suffix.is_empty() {
        return cleaned;
    }
    let trimmed = cleaned.trim_end_matches('/');
    if trimmed.ends_with(suffix) {
        trimmed.to_owned()
    } else {
        format!("{trimmed}/{suffix}")
    }
}
