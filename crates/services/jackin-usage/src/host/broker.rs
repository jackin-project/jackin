// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-only usage broker lifecycle and bounded Unix-socket transport.

mod runtime;

#[cfg(test)]
pub(crate) use jackin_usage_broker::short_socket_alias;
pub use jackin_usage_broker::{
    ForwardedUsageSources, UsageBrokerClient, UsageBrokerConfig, UsageBrokerHandle,
    ensure_usage_broker, ensure_usage_broker_process, ensure_usage_broker_with_executor,
    forwarded_usage_capabilities, run_usage_broker_service, run_usage_broker_service_with_executor,
    usage_capability_for_selected_account, usage_capability_for_selected_account_with_sources,
};
#[cfg(all(test, not(target_os = "macos")))]
pub(crate) use jackin_usage_broker_wire::UNIX_SOCKET_PATH_LIMIT;
#[cfg(all(test, target_os = "macos"))]
pub(crate) use jackin_usage_broker_wire::UNIX_SOCKET_PATH_LIMIT;
