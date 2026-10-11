//! jackin-usage-broker: host usage broker lifecycle and transport.
//!
//! **Architecture Invariant:** T5.
//! Entry points: host process activation and broker-owned service lifecycle.
//!
//! Host-only usage broker lifecycle and bounded Unix-socket transport.

pub(crate) use jackin_usage_broker_publish as publish;
pub(crate) use jackin_usage_broker_wire as probe;
mod view;
mod waits;

mod authorize;
mod capabilities;
mod catalog;
pub(crate) mod catalog_diagnostics;
mod client;
mod config;
mod dispatch;
mod ensure;
mod errors;
mod executor;
mod forwarding;
mod handle;
mod leader;
mod monitor;
mod projection;
mod rediscover;
mod serve;
mod service;
mod source_identity;
mod startup;

pub(crate) use capabilities::{
    forwarded_usage_capabilities, usage_capability_for_selected_account_with_sources,
};
pub use client::UsageBrokerClient;
pub(crate) use client::{validate_broker_data_ancestors, validate_broker_data_tree};
pub use config::UsageBrokerConfig;
pub use config::short_socket_alias;
pub use ensure::{ensure_usage_broker_process, ensure_usage_monitor_process};
pub use handle::ForwardedUsageSources;
#[cfg(not(target_os = "macos"))]
pub(crate) use jackin_usage_broker_wire::UNIX_SOCKET_PATH_LIMIT;
#[cfg(target_os = "macos")]
pub(crate) use jackin_usage_broker_wire::UNIX_SOCKET_PATH_LIMIT;
pub(crate) use jackin_usage_discovery::capability_for_binding;
pub(crate) use jackin_usage_discovery::usage_catalog_entries;
pub use service::{
    UsageBrokerForegroundReady, ensure_usage_broker_with_executor,
    run_usage_broker_foreground_bootstrap, run_usage_broker_service,
    run_usage_broker_service_with_executor, run_usage_monitor_service,
};

pub(crate) use authorize::{
    ForwardingRequirement, authorize_credential_binding_group, credential_scope_has_matching_proof,
};
pub(crate) use catalog::BrokerCatalogRefresh;

pub(crate) use dispatch::{dispatch, read_frame};
pub(crate) use errors::publication_identity_metadata;
pub(crate) use executor::DiscoveryProviderExecutor;
#[cfg(test)]
pub(crate) use executor::probe_with_scope;
pub(crate) use forwarding::{
    forwarding_requirement, refresh_authority_equivalent, unscoped_refresh_binding,
};
pub(crate) use jackin_usage_broker_wire::{
    BROKER_ACTIVATION_ATTEMPTS, BROKER_CONNECTION_QUEUE, BROKER_CONNECTION_WORKERS, BROKER_DIR,
    BROKER_IDLE_EXIT, BROKER_LEADER, BROKER_LEASE_DURATION, BROKER_LEASE_RENEWAL, BROKER_RUN_DIR,
    BROKER_SOCKET, BROKER_SOCKET_ALIAS_DIR_PREFIX, CONNECT_RETRY, CONNECT_RETRY_STEP, PUBLISH_TICK,
};
pub(crate) use jackin_usage_broker_wire::{BrokerLease, BrokerLeaseOwner, ServePolicy};
pub(crate) use jackin_usage_broker_wire::{
    catalog_discovery_mismatch, credential_scope_mismatch, protocol_error, unavailable,
};
pub(crate) use jackin_usage_broker_wire::{secure_run_directory, validate_owned_mode};
pub(crate) use leader::{
    BrokerSocketIdentity, claim_leader, cleanup_owned_files, connect_probe, renew_lease,
    wait_for_leader,
};
pub(crate) use monitor::MonitorStore;
pub use monitor::parse_statusline;
pub(crate) use projection::{LoadedProjection, load_projection};
#[cfg(test)]
pub(crate) use rediscover::provider_probe_outcome;
#[cfg(test)]
pub(crate) use rediscover::provider_probe_outcome_with_metadata;
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
