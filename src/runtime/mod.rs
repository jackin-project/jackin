mod attach;
mod cleanup;
mod discovery;
mod identity;
mod image;
mod launch;
mod naming;
mod repo_cache;

#[cfg(test)]
pub mod test_support;

#[cfg(test)]
pub use self::test_support::FakeRunner;

/// Environment variables owned by the jackin runtime that must not be
/// overridden by agent manifests.  These are injected as `-e` flags in
/// `launch_agent_runtime` and are silently skipped if a manifest declares them.
/// The corresponding manifest-time validation lives in
/// `manifest::RESERVED_RUNTIME_ENV_VARS`.
const RUNTIME_OWNED_ENV_VARS: &[&str] = &["DOCKER_HOST", "DOCKER_TLS_VERIFY", "DOCKER_CERT_PATH"];

pub use self::attach::{ContainerState, hardline_agent, inspect_container_state};
pub use self::cleanup::{eject_agent, exile_all, purge_class_data};
pub use self::discovery::{
    list_managed_agent_names, list_running_agent_display_names, list_running_agent_names,
};
pub use self::launch::{LoadOptions, load_agent};
pub use self::naming::matching_family;
