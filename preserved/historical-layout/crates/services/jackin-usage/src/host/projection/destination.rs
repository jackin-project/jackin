// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage destinations.

use jackin_protocol::usage_broker::UsageProjectionV1;

pub(crate) struct ProjectionMetadata<'a> {
    pub projection_id: &'a str,
    pub generated_at_epoch: i64,
    pub broker_instance_id: &'a str,
    pub broker_generation: u64,
    pub refreshing: bool,
    pub locale: &'a str,
}

/// Typed interactive destination outside the canonical JSON projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageDestination {
    /// All current canonical accounts.
    Overview,
    /// Provider destination allowed only when it owns exactly one account.
    Provider { provider_id: String },
    /// Exact account destination for a multi-account provider.
    Account {
        provider_id: String,
        canonical_account_id: String,
    },
}

/// Result of reconciling adapter selection against one immutable publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedUsageDestination {
    pub destination: UsageDestination,
    pub notice: Option<String>,
}

/// Preserve a stable destination or return honestly to Overview when removed.
#[must_use]
pub fn normalize_destination(
    projection: &UsageProjectionV1,
    requested: &UsageDestination,
) -> NormalizedUsageDestination {
    let valid = match requested {
        UsageDestination::Overview => true,
        UsageDestination::Provider { provider_id } => projection
            .providers
            .iter()
            .any(|provider| provider.provider_id == *provider_id && provider.accounts.len() == 1),
        UsageDestination::Account {
            provider_id,
            canonical_account_id,
        } => projection.providers.iter().any(|provider| {
            provider.provider_id == *provider_id
                && provider.accounts.len() > 1
                && provider
                    .accounts
                    .iter()
                    .any(|account| account.canonical_account_id == *canonical_account_id)
        }),
    };
    if valid {
        NormalizedUsageDestination {
            destination: requested.clone(),
            notice: None,
        }
    } else {
        NormalizedUsageDestination {
            destination: UsageDestination::Overview,
            notice: Some("Selected account is no longer available.".to_owned()),
        }
    }
}
