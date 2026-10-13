// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Account lifecycle and provenance.

/// Account lifecycle is independent from snapshot freshness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccountLifecycle {
    /// Affirmed by current broker discovery or a current host projection.
    Current,
    /// Available only as durable/stale history.
    Historical,
    /// Credential presence without authenticated account identity.
    ProviderPresenceOnly,
}

impl AccountLifecycle {
    /// Stable DTO label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Historical => "historical",
            Self::ProviderPresenceOnly => "provider_presence_only",
        }
    }
}

/// Non-secret places that contributed evidence for one canonical account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccountProvenance {
    /// Current config-derived credential source.
    ConfiguredSource,
    /// Active host credential/login.
    LiveHost,
    /// Durable last-good snapshot.
    DurableHistory,
}

impl AccountProvenance {
    /// Rust-owned user-facing provenance copy.
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::ConfiguredSource => "Configured source",
            Self::LiveHost => "Live host",
            Self::DurableHistory => "History",
        }
    }
}
