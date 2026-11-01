// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccountLifecycle {
    Current,
    Historical,
    ProviderPresenceOnly,
}

impl AccountLifecycle {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Historical => "historical",
            Self::ProviderPresenceOnly => "provider_presence_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccountProvenance {
    ConfiguredSource,
    LiveHost,
    DurableHistory,
}

impl AccountProvenance {
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::ConfiguredSource => "Configured source",
            Self::LiveHost => "Live host",
            Self::DurableHistory => "History",
        }
    }
}
