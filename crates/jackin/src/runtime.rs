//! Runtime shim — all code lives in `jackin-runtime`.
//!
//! Re-exports the public API from `jackin-runtime::runtime` so that existing
//! `crate::runtime::*` call sites in the binary continue to compile unchanged.

pub mod attach {
    pub use jackin_runtime::runtime::attach::*;
}
pub mod drift {
    pub use jackin_runtime::runtime::drift::*;
}
pub mod docker_profile {
    pub use jackin_runtime::runtime::docker_profile::*;
}
pub mod logs {
    pub use jackin_runtime::runtime::logs::*;
}
pub mod naming {
    pub use jackin_runtime::runtime::naming::*;
}
pub mod progress {
    pub use jackin_runtime::runtime::progress::*;
}
pub mod snapshot {
    pub use jackin_runtime::runtime::snapshot::*;
}

pub use jackin_runtime::runtime::DockerSecurityProfile;
pub(crate) use jackin_runtime::runtime::RepoError;
pub(crate) use jackin_runtime::runtime::docker_unavailable_msg;
pub use jackin_runtime::runtime::matching_family;
pub use jackin_runtime::runtime::prewarm_dind_sidecar_container;
pub use jackin_runtime::runtime::reconcile_keep_awake;
pub use jackin_runtime::runtime::write_prewarmed_dind_state;
pub use jackin_runtime::runtime::{
    AgentSession, AgentSessionInventory, ContainerState, describe_agent_session_count,
    hardline_agent, hardline_agent_with_focus, inspect_agent_sessions, inspect_hardline_instance,
    spawn_agent_session, spawn_shell_session,
};
pub use jackin_runtime::runtime::{
    DIND_IMAGE, DindSidecarPrewarm, ImagePrewarmStatus, RoleImagePrewarmRow, prewarm_role_images,
};
pub(crate) use jackin_runtime::runtime::{
    EntryClaim, StartKind, claim_construct_entry, force_boundary_intro_enabled,
    release_entry_if_idle,
};
pub use jackin_runtime::runtime::{LoadOptions, load_role};
pub use jackin_runtime::runtime::{background_prewarm_targets, spawn_background_image_prewarm};
pub use jackin_runtime::runtime::{
    eject_role, exile_all, prune_all_instances, prune_cache, prune_diagnostics, prune_images,
    prune_instances, prune_jackin_home, prune_roles, purge_class_data, purge_container_state,
};
pub use jackin_runtime::runtime::{
    list_managed_role_names, list_running_agent_display_names, list_running_agent_names,
};

pub use jackin_runtime::runtime::resolve_supported_agents_for_console;

pub(crate) async fn register_agent_repo(
    paths: &crate::paths::JackinPaths,
    selector: &crate::selector::RoleSelector,
    git_url: &str,
    runner: &mut impl crate::docker::CommandRunner,
    debug: bool,
) -> anyhow::Result<(crate::repo::CachedRepo, crate::repo::ValidatedRoleRepo)> {
    jackin_runtime::runtime::register_agent_repo(paths, selector, git_url, runner, debug).await
}

#[cfg(test)]
pub use jackin_runtime::runtime::test_support::FakeRunner;

#[cfg(test)]
pub mod test_support {
    pub use jackin_runtime::runtime::test_support::*;
}
