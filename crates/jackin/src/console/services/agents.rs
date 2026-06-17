//! Non-TUI agent resolution services.

pub(crate) async fn resolve_supported_for_console(
    paths: &crate::paths::JackinPaths,
    config: &crate::config::AppConfig,
    role: &crate::selector::RoleSelector,
    runner: &mut impl crate::docker::CommandRunner,
) -> anyhow::Result<Vec<jackin_core::Agent>> {
    crate::runtime::resolve_supported_agents_for_console(paths, config, role, runner).await
}

pub(crate) async fn load_inline_picker_choices(
    paths: &crate::paths::JackinPaths,
    config: &crate::config::AppConfig,
    role: &crate::selector::RoleSelector,
    runner: &mut impl crate::docker::CommandRunner,
) -> anyhow::Result<Option<Vec<jackin_core::Agent>>> {
    let agents = resolve_supported_for_console(paths, config, role, runner).await?;
    if agents.len() < 2 {
        return Ok(None);
    }
    Ok(Some(agents))
}
