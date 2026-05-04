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

pub(crate) fn register_agent_repo(
    paths: &crate::paths::JackinPaths,
    selector: &crate::selector::RoleSelector,
    git_url: &str,
    runner: &mut impl crate::docker::CommandRunner,
    debug: bool,
    persist_registration: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<(crate::repo::CachedRepo, crate::repo::ValidatedRoleRepo)> {
    self::repo_cache::register_agent_repo(
        paths,
        selector,
        git_url,
        runner,
        debug,
        persist_registration,
    )
}
