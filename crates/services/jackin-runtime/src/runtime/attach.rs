// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Session attach/reconnect/hardline for running containers.
//!
//! Drives capsule client connections and session inventory queries against a
//! live container's daemon socket. Not responsible for container start-up,
//! image build, or identity resolution — those live in sibling modules.
//!
//! Key invariant: callers treat `AgentSessionInventory::Unavailable` as a
//! transient state during the setup-once window; they must not surface it as
//! a terminal error.

pub use jackin_docker::docker_client::ContainerState;

mod admission;
// Moved to jackin_runtime_attach_capsule_ready::capsule_ready
// (S7 split 74); the module re-export keeps every
// `attach::capsule_ready::*` path stable.
pub(crate) use jackin_runtime_attach_capsule_ready::capsule_ready;
mod exec_args;
mod finalize;
mod hardline;
mod hardline_start;
mod inspect;
mod reconnect;
mod reconnect_lease;
// Moved to jackin_runtime_attach_sessions::sessions (S7
// split 73); the module re-export keeps every
// `attach::sessions::*` path stable.
pub(crate) use jackin_runtime_attach_sessions::sessions;
mod spawn;
mod transport;

pub use hardline::{hardline_agent, hardline_agent_with_focus};
pub use inspect::{describe_agent_session_count, inspect_hardline_instance};
pub use sessions::{
    AgentSession, AgentSessionInventory, docker_unavailable_msg, inspect_agent_sessions,
};
pub use spawn::{spawn_agent_session, spawn_shell_session};
pub use transport::{
    ATTACH_PROXY_SUBCOMMAND, HostAttachTransportPlan, JACKIN_CAPSULE_PATH, attach_proxy_exec_args,
    select_host_attach_transport,
};

pub(crate) use admission::{
    ReconnectAdmissionFailure, mark_reconnect_admission_failure, require_current_account_admission,
    require_current_instance_admission, validate_current_account_admission,
    validate_recorded_role_handle,
};
pub(crate) use capsule_ready::{
    capsule_socket_negotiates, wait_for_capsule_daemon_with_handle, wait_for_dind,
};
pub(crate) use exec_args::{
    git_policy_env_pairs, host_alt_screen_exec_flag, insert_run_as_user, set_role_terminal_title,
};
pub(crate) use finalize::{
    finalize_reconnected_foreground_session, finalize_reconnected_foreground_session_with_handle,
};
pub(crate) use hardline::{
    hardline_docker_agent_with_focus, hardline_docker_agent_with_focus_with_lease,
};
pub(crate) use hardline_start::{
    require_container_reachable, require_container_running, start_or_hardline_agent,
    start_or_hardline_agent_with_container_handle,
};
pub(crate) use inspect::missing_restore_message;
pub(crate) use reconnect::{
    reconnect_or_create_session_with_container_handle_with_lease,
    reconnect_or_create_session_with_focus, start_or_reconnect_capsule_client,
};
pub(crate) use reconnect_lease::start_or_reconnect_capsule_client_with_handle_with_lease;
pub(crate) use sessions::inspect_unavailable_message;

#[cfg(test)]
mod tests;
