//! jackin-usage-broker-wire: broker socket/lease wire primitives.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`secure_run_directory`] — harden the broker run directory.
//!
//! Socket/lease constants, descriptor-bound lease ownership, run-directory
//! hardening, bounded probe budgets, and wire error constructors. The
//! socket client, dispatch, and serve loop stay with the T4 broker.

mod consts;
mod errors;
mod lease;
mod paths;
mod probe;

pub use consts::{
    BROKER_ACTIVATE_LOCK, BROKER_ACTIVATION_ATTEMPTS, BROKER_CONNECTION_QUEUE,
    BROKER_CONNECTION_WORKERS, BROKER_DIR, BROKER_IDLE_EXIT, BROKER_LEADER, BROKER_LEASE_DURATION,
    BROKER_LEASE_RENEWAL, BROKER_RUN_DIR, BROKER_SOCKET, BROKER_SOCKET_ALIAS_DIR_PREFIX,
    CONNECT_RETRY, CONNECT_RETRY_STEP, PUBLISH_TICK, UNIX_SOCKET_PATH_LIMIT,
};
pub use errors::{
    catalog_discovery_mismatch, credential_scope_mismatch, protocol_error, unavailable,
};
pub use lease::{BrokerLease, BrokerLeaseOwner, ServePolicy};
pub use paths::{
    private_child_directory, secure_run_directory, validate_owned_base_directory,
    validate_owned_directory, validate_owned_mode,
};
pub use probe::{ProbeBudgetExpired, probe_timeout_outcome, run_probe_with_budget};
