// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-only usage broker lifecycle and bounded Unix-socket transport.

mod probe;
pub(super) mod publish;
mod view;
mod waits;

mod authorize;
mod capabilities;
mod client;
mod config;
mod consts;
mod dispatch;
mod ensure;
mod errors;
mod executor;
mod forwarding;
mod handle;
mod leader;
mod lease;
mod paths;
mod projection;
mod rediscover;
mod runtime;
mod serve;
mod service;
mod startup;

pub(crate) use capabilities::usage_catalog_entries;
pub use capabilities::{
    forwarded_usage_capabilities, usage_broker_capabilities, usage_capability_for_selected_account,
    usage_capability_for_selected_account_with_sources,
};
pub use client::UsageBrokerClient;
pub use config::UsageBrokerConfig;
#[cfg(test)]
pub(crate) use config::short_socket_alias;
#[cfg(not(target_os = "macos"))]
pub(crate) use consts::UNIX_SOCKET_PATH_LIMIT;
#[cfg(target_os = "macos")]
pub(crate) use consts::UNIX_SOCKET_PATH_LIMIT;
pub use ensure::{ensure_usage_broker, ensure_usage_broker_process};
pub use handle::{ForwardedUsageSources, UsageBrokerHandle};
pub(crate) use leader::capability_for_binding;
pub use service::{
    ensure_usage_broker_with_executor, run_usage_broker_service,
    run_usage_broker_service_with_executor,
};

pub(crate) use authorize::{
    ForwardingRequirement, ScopedCapability, authorize_credential_binding_group,
    credential_scope_has_matching_proof,
};

pub(crate) use consts::{
    BROKER_ACTIVATE_LOCK, BROKER_ACTIVATION_ATTEMPTS, BROKER_CONNECTION_QUEUE,
    BROKER_CONNECTION_WORKERS, BROKER_DIR, BROKER_IDLE_EXIT, BROKER_LEADER, BROKER_LEASE_DURATION,
    BROKER_LEASE_RENEWAL, BROKER_RUN_DIR, BROKER_SOCKET, BROKER_SOCKET_ALIAS_DIR_PREFIX,
    CONNECT_RETRY, CONNECT_RETRY_STEP, PUBLISH_TICK,
};
pub(crate) use dispatch::{dispatch, read_frame};
#[cfg(test)]
pub(crate) use ensure::ensure_usage_broker_with_hooks;
pub(crate) use errors::{
    catalog_discovery_mismatch, credential_scope_mismatch, protocol_error,
    publication_identity_metadata, unavailable,
};
pub(crate) use executor::DiscoveryProviderExecutor;
#[cfg(test)]
pub(crate) use executor::probe_with_scope;
pub(crate) use forwarding::{
    forwarding_requirement, refresh_authority_equivalent, unscoped_refresh_binding,
};
pub(crate) use leader::{
    claim_leader, cleanup_owned_files, connect_probe, renew_lease, wait_for_leader,
};
pub(crate) use lease::{BrokerLease, BrokerLeaseOwner, ServePolicy};
pub(crate) use paths::{secure_run_directory, validate_owned_mode};
pub(crate) use projection::{LoadedProjection, load_projection};
#[cfg(test)]
pub(crate) use rediscover::provider_probe_outcome;
#[cfg(test)]
pub(crate) use rediscover::provider_probe_outcome_with_rate_limit;
pub(crate) use rediscover::{
    catalog_entry_map, ensure_catalog_matches, grouped_bindings, rediscover_all_bindings,
    rediscover_bindings, rediscover_discovery, refresh_binding_outcome,
};
#[cfg(test)]
pub(crate) use serve::write_with_deadline;
pub(crate) use serve::{ServeConfig, serve, write_response};

pub(crate) use startup::BrokerStartupCleanup;

#[cfg(test)]
mod tests;
