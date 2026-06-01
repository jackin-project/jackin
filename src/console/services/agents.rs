//! Non-TUI agent resolution services.

pub async fn resolve_supported_for_console(
    paths: &crate::paths::JackinPaths,
    config: &crate::config::AppConfig,
    role: &crate::selector::RoleSelector,
    runner: &mut impl crate::docker::CommandRunner,
) -> anyhow::Result<Vec<crate::agent::Agent>> {
    crate::runtime::resolve_supported_agents_for_console(paths, config, role, runner).await
}
