// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProviderError {
    #[error("the hub refused its management key")]
    HubRefused,
    #[error("the login was rejected")]
    Unauthorized,
    #[error("the login is not allowed to read this")]
    Forbidden,
    #[error("the provider rate-limited the request")]
    RateLimited { retry_after_seconds: Option<u64> },
    #[error("{0}")]
    Network(&'static str),
    #[error("the provider answered HTTP {0}")]
    Status(u16),
    #[error("HTTP {status} {}", plain(.reason))]
    Denied { status: u16, reason: String },
    #[error("{0}")]
    Decode(&'static str),
    #[error("{0}")]
    Unsupported(&'static str),
}

impl ProviderError {
    pub const OUT_OF_RANGE: ProviderError =
        ProviderError::Decode("a number in the response is out of range");
}

fn plain(value: impl std::fmt::Display) -> String {
    let text = value.to_string();
    let mut out = String::with_capacity(text.len());
    let mut run = 0;
    for (index, c) in text.char_indices() {
        if matches!(c, '"' | '\'' | '\\') {
            out.extend(text[run..index].escape_debug());
            out.push(c);
            run = index + c.len_utf8();
        }
    }
    out.extend(text[run..].escape_debug());
    out
}
