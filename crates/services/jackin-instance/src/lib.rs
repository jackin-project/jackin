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

#[cfg(test)]
mod tests;
