mod attach;
mod caffeinate;
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
pub use self::caffeinate::reconcile as reconcile_keep_awake;
pub use self::cleanup::{eject_role, ensure_role_not_running, exile_all, purge_class_data};
pub(crate) use self::discovery::list_role_names;
pub use self::discovery::{
    list_managed_role_names, list_running_agent_display_names, list_running_agent_names,
};
pub use self::launch::{LoadOptions, load_role};
pub use self::naming::matching_family;
