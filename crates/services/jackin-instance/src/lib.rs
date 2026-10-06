//! jackin-instance: instance naming, manifests, and lifecycle records.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`InstanceManifest`] — on-disk instance record.

mod auth;
pub use auth::{AuthMountLease, validate_sync_source_dir};
mod error;
pub use error::{InstanceError, SyncSourceValidationError};
pub mod manifest;
pub mod naming;
mod process_telemetry;
pub use manifest::{
    AdmittedInstance, AppleContainerResources, BackendResources, DockerIdentity, DockerResources,
    InstanceIndex, InstanceIndexEntry, InstanceManifest, InstanceQuery, InstanceStatus,
    NewInstanceManifest, RegistrationState, SessionRecord, SessionStatus,
};
pub use naming::{class_family_matches, container_name_with_id, new_container_name, runtime_slug};
mod account_sources;
mod agent_auth;
mod agent_slots;
mod ignore;
mod outcomes;
mod prepare;
mod provision;
mod role_state;
mod single_file_slots;
mod slots;

pub use agent_auth::{
    AgentRuntimeState, InstanceAuthBinding, ProvisionedAuth, ProvisionedInstanceAuth,
};
pub use outcomes::{
    AuthProvisionOutcome, GithubProvisionKind, GithubProvisionOutcome, GithubTokenSource,
    HostMissingReason,
};
pub use role_state::{GithubAuthContext, PrepareResolvers, RoleState};
pub use slots::slot_home_rel;

pub(crate) use account_sources::capture_selected_account_sources;
#[cfg(test)]
pub(crate) use account_sources::validate_selected_account_sources;
pub(crate) use agent_auth::{AgentAuthProvision, DEFAULT_ACCOUNT_ID, emit_agent_auth_provision};
pub(crate) use ignore::{
    agent_ignore_can_skip_state_prepare, github_ignore_can_skip_state_prepare,
    skipped_ignore_instance_auth,
};
#[cfg(test)]
pub(crate) use slots::slot_home_and_target;
pub(crate) use slots::{
    SlotLayout, agent_slot_dirs, slot_layout, slot_store_rel, slot_suffixes, xdg_cache_rel,
    xdg_root_agent,
};

#[cfg(test)]
mod tests;
