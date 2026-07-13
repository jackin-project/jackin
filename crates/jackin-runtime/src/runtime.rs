//! jackin❯ container bootstrap runtime.
//!
//! Re-exports the public entry points consumed by `app.rs` and `console/`.
//! Each sub-module owns one slice of the container lifecycle.

pub mod apple_container;
pub mod attach;
pub mod backend;
pub mod cleanup;
pub mod discovery;
pub mod docker_profile;
pub mod drift;
pub mod exit_summary;
pub mod host_attach;
pub mod host_colors;
pub mod identity;
pub mod image;
pub mod launch;
pub mod logs;
pub mod naming;
pub mod prewarm_trigger;
pub mod progress;
pub mod prune_output;
pub mod repo_cache;
pub(crate) mod shared_runner;
pub mod snapshot;
pub mod universe;

#[cfg(test)]
pub mod test_support;

pub use self::attach::docker_unavailable_msg;
pub use self::attach::{
    AgentSession, AgentSessionInventory, ContainerState, describe_agent_session_count,
    hardline_agent, hardline_agent_with_focus, inspect_agent_sessions, inspect_hardline_instance,
    spawn_agent_session, spawn_shell_session,
};
pub use self::cleanup::{
    eject_role, exile_all, prune_all_instances, prune_cache, prune_diagnostics, prune_images,
    prune_instances, prune_jackin_home, prune_roles, purge_class_data, purge_container_state,
};
pub use self::discovery::list_role_names;
pub use self::discovery::{
    list_managed_role_names, list_running_agent_display_names, list_running_agent_names,
};
pub use self::docker_profile::{DockerSecurityProfile, ProfileSource, resolve_profile};
#[cfg(not(test))]
pub use self::image::{ImagePrewarmStatus, RoleImagePrewarmRow, prewarm_role_images};
pub use self::launch::{
    DIND_IMAGE, DindSidecarPrewarm, LoadOptions, load_role, prewarm_dind_sidecar_container,
    write_prewarmed_dind_state,
};
pub use self::naming::matching_family;
pub use self::prewarm_trigger::{
    BackgroundPrewarmTarget, background_prewarm_targets, spawn_background_image_prewarm,
    spawn_background_sidecar_prewarm,
};
pub use self::repo_cache::{RepoError, normalize_github_url};
pub use self::universe::{
    EntryClaim, StartKind, claim_entry as claim_construct_entry, force_boundary_intro_enabled,
    release_entry_if_idle,
};
pub use ::jackin_host::caffeinate::reconcile as reconcile_keep_awake;
pub use ::jackin_host::caffeinate::reconcile_when_configured as reconcile_keep_awake_when_configured;

pub use self::launch::resolve_supported_agents_for_console;

pub async fn register_agent_repo(
    paths: &jackin_core::paths::JackinPaths,
    selector: &jackin_core::selector::RoleSelector,
    git_url: &str,
    runner: &mut impl jackin_core::CommandRunner,
    debug: bool,
) -> anyhow::Result<(
    jackin_manifest::repo::CachedRepo,
    jackin_manifest::repo::ValidatedRoleRepo,
)> {
    repo_cache::register_agent_repo(paths, selector, git_url, runner, debug).await
}
