//! jackin-usage-coordinator: per-account single-flight refresh generations.
//!
//! **Architecture Invariant:** T2.
//! Entry point: [`UsageCoordinator`] — single-flight refresh generations.
//!
//! Per-account single-flight refresh generations owned by the host broker.
//! Probe execution stays behind [`UsageProviderExecutor`], implemented by
//! the T4 host broker; file-backed generation and projection state lives
//! behind [`AccountStateStore`].

pub mod policy;
mod state;

mod cadence;
mod catalog;
mod config;
mod construct;
mod entries;
mod errors;
mod finish;
mod jobs;
mod join;
mod outcome;
mod reconcile;
mod refresh;
mod upkeep;
mod worker;

#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(test)]
use std::sync::mpsc;
#[cfg(test)]
use std::sync::{Arc, Condvar, Mutex};
#[cfg(test)]
use std::time::{Duration, Instant};

#[cfg(test)]
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationErrorKind, UsageGenerationView,
    UsageRefreshPhase,
};
#[cfg(test)]
use policy::UsageActivity;

pub use state::{
    AccountStateEnvelope, AccountStateStore, FileAccountStateStore, FileProjectionStateStore,
    ProjectionAlias, ProjectionStateEnvelope, StateStoreError,
};

pub(crate) use catalog::{
    catalog_entries_from_map, catalog_purge_set, first_catalog_rollback_error,
    preserve_catalog_error, reconcile_executor_catalog, restore_catalog_preimages,
    validate_catalog_entries,
};
pub use config::{UsageCapabilitySet, UsageCoordinatorConfig};
pub use entries::CatalogTransaction;
pub(crate) use entries::{AccountEntry, CatalogAccountPreimage, CoordinatorState, Shared};
pub(crate) use errors::{
    catalog_revoked_error, coordination_error, state_error, unavailable_error,
};
pub(crate) use finish::{finish_failure, finish_success, mark_updating};
pub use jobs::UsageCoordinator;
pub(crate) use jobs::{ProbeJob, WorkerMessage};
pub(crate) use outcome::TERMINAL_HISTORY_LIMIT;
pub use outcome::{ProviderProbeOutcome, UsageProviderExecutor};
pub(crate) use upkeep::{
    cadence_deadline, data_bearing, generation_view, record_blocked_terminal, reset_entry,
    revoke_entry,
};
pub(crate) use worker::coordinator_worker;

#[cfg(test)]
mod tests;
