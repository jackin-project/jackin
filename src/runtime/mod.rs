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

pub use self::attach::{ContainerState, hardline_agent, inspect_container_state};
pub use self::cleanup::{eject_agent, ensure_agent_not_running, exile_all, purge_class_data};
pub use self::discovery::{
    list_managed_agent_names, list_running_agent_display_names, list_running_agent_names,
};
pub use self::launch::{LoadOptions, load_agent};
pub use self::naming::matching_family;
