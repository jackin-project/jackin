//! jackin-runtime: container bootstrap pipeline.
//!
//! Holds the concrete `DockerApi` / `CommandRunner` implementations,
//! image build, DinD sidecar management, mount materialization, and
//! instance lifecycle.
//!
//! **Dependency tier:** `jackin-core` → `jackin-config` → `jackin-env` → `jackin-runtime`

pub mod instance;
pub mod isolation;
pub mod runtime;
pub mod spin_wait;

// Re-export the key public items to match what the binary's src/runtime/mod.rs exposes.
pub use runtime::{
    AgentSession, AgentSessionInventory, ContainerState,
    LoadOptions, load_role,
    describe_agent_session_count, hardline_agent, hardline_agent_with_focus,
    inspect_agent_sessions, inspect_hardline_instance, spawn_agent_session, spawn_shell_session,
    reconcile_keep_awake,
    eject_role, exile_all, prune_all_instances, prune_cache, prune_diagnostics, prune_images,
    prune_instances, prune_jackin_home, prune_roles, purge_class_data, purge_container_state,
    list_managed_role_names, list_running_agent_display_names, list_running_agent_names,
    matching_family,
    resolve_supported_agents_for_console,
};

pub use runtime::drift;
pub use runtime::logs;
pub use runtime::progress;
pub use runtime::snapshot;
