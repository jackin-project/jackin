// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProviderError {
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
    #[error("{0}")]
    Decode(&'static str),
    #[error("{0}")]
    Unsupported(&'static str),
}
