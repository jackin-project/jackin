// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! jackin-runtime: container bootstrap pipeline.
//!
//! Holds the concrete `DockerApi` / `CommandRunner` implementations,
//! image build, `DinD` sidecar management, mount materialization, and
//! instance lifecycle.
//!
//! **Dependency tier:** `jackin-core` → `jackin-config` → `jackin-env` → `jackin-runtime`
//!
//! **Architecture Invariant:** L1 application / orchestration crate.
//! Allowed dependencies: `jackin-core`, `jackin-config`, `jackin-env`,
//! `jackin-manifest`, `jackin-docker`, `jackin-image`,
//! `jackin-diagnostics`, `jackin-launch-tui`, `jackin-host`,
//! `jackin-protocol`, `jackin-isolation`, `jackin-instance`.
//! (R1: `jackin-tui` production edge removed via pure-item relocation to
//! core + `LaunchOutputSink` port; only dev-dep remains for tests.)

pub mod apple_container_client;
pub mod exec_host;
pub mod isolation;
pub mod runtime;
pub mod spin_wait;

// Re-export jackin_instance as `instance` so existing call sites
// (crate::instance::X) continue to compile unchanged.
pub use jackin_instance as instance;

// Re-export the key public items to match what the binary's src/runtime/mod.rs exposes.
pub use runtime::{
    AgentSession, AgentSessionInventory, ContainerState, DindSidecarPrewarm, LoadOptions,
    describe_agent_session_count, eject_role, exile_all, hardline_agent, hardline_agent_with_focus,
    inspect_agent_sessions, inspect_hardline_instance, list_managed_role_names,
    list_running_agent_display_names, list_running_agent_names, load_role, matching_family,
    prewarm_dind_sidecar_container, prune_all_instances, prune_cache, prune_diagnostics,
    prune_images, prune_instances, prune_jackin_home, prune_roles, purge_class_data,
    purge_container_state, reconcile_keep_awake, resolve_supported_agents_for_console,
    spawn_agent_session, spawn_shell_session, write_prewarmed_dind_state,
};
#[cfg(not(test))]
pub use runtime::{ImagePrewarmStatus, RoleImagePrewarmRow, prewarm_role_images};

pub use runtime::drift;
pub use runtime::logs;
pub use runtime::progress;
pub use runtime::snapshot;
