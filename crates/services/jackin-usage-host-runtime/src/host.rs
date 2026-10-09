// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Credential-free host projection presentation and broker client APIs.

mod projection;

/// Subdirectory for host-owned projection preferences.
pub(crate) const HOST_USAGE_STATE_REL: &str = "usage-menu-bar";

pub use jackin_usage_broker::{
    ForwardedUsageSources, UsageBrokerClient, UsageBrokerConfig, ensure_usage_broker_process,
    ensure_usage_broker_with_executor, ensure_usage_monitor_process, parse_statusline,
    run_usage_broker_service, run_usage_broker_service_with_executor, run_usage_monitor_service,
};
pub use jackin_usage_discovery::UsageDiscoveryScope;
pub use jackin_usage_host_presentation::HostSurfaceId;
pub use projection::{
    HostUsageProjectionAccountPresentation, HostUsageProjectionConfig,
    HostUsageProjectionProviderPresentation, HostUsageProjectionRuntime,
    HostUsageProjectionSelectedAccount,
};

#[cfg(test)]
mod tests;
