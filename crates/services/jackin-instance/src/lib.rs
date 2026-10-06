//! jackin-instance: instance naming, manifests, and lifecycle records.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`InstanceManifest`] — on-disk instance record.
//!
//! Auth provisioning lives in the sibling crates
//! `jackin-instance-roles` (orchestration), `jackin-instance-agents`
//! (per-agent provisioners), and `jackin-instance-credentials`
//! (substrate); this crate re-exports their API for compatibility.

pub mod manifest;
pub mod naming;

pub use jackin_instance_agents::slot_home_rel;
pub use jackin_instance_agents::{
    AgentRuntimeState, InstanceAuthBinding, ProvisionedAuth, ProvisionedInstanceAuth,
    validate_sync_source_dir,
};
pub use jackin_instance_credentials::{
    AuthMountLease, AuthProvisionOutcome, GithubAuthContext, GithubProvisionKind,
    GithubProvisionOutcome, GithubTokenSource, HostMissingReason, InstanceError,
    SyncSourceValidationError,
};
pub use jackin_instance_roles::{PrepareResolvers, RoleState};
pub use manifest::{
    AdmittedInstance, AppleContainerResources, BackendResources, DockerIdentity, DockerResources,
    InstanceIndex, InstanceIndexEntry, InstanceManifest, InstanceQuery, InstanceStatus,
    NewInstanceManifest, RegistrationState, SessionRecord, SessionStatus,
};
pub use naming::{class_family_matches, container_name_with_id, new_container_name, runtime_slug};
