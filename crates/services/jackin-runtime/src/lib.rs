pub mod apple_container_client;
pub mod exec_host;
#[cfg(unix)]
pub mod host_daemon;
pub mod isolation;
mod process_telemetry;
#[cfg(all(feature = "daemon-spike", unix))]
pub mod reactive_daemon;
pub mod runtime;
pub mod spin_wait;
pub use jackin_runtime_usage_relay::usage_relay;

pub use jackin_instance as instance;

pub use runtime::{
    AgentSession, AgentSessionInventory, ContainerState, DindSidecarPrewarm, LoadOptions,
    describe_agent_session_count, eject_role, exile_all, hardline_agent, hardline_agent_with_focus,
    inspect_agent_sessions, inspect_hardline_instance, list_managed_role_names,
    list_running_agent_display_names, list_running_agent_names, load_role, matching_family,
    prewarm_dind_sidecar_container_with_paths, prune_all_instances, prune_cache, prune_images,
    prune_instances, prune_jackin_home, prune_roles, purge_class_data, purge_container_state,
    reconcile_keep_awake, resolve_supported_agents_for_console, spawn_agent_session,
    spawn_background_sidecar_prewarm, spawn_shell_session, write_prewarmed_dind_state,
};
#[cfg(not(test))]
pub use runtime::{ImagePrewarmStatus, RoleImagePrewarmRow, prewarm_role_images};

pub use runtime::drift;
pub use runtime::progress;
pub use runtime::snapshot;
