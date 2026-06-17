//! Non-TUI Claude token setup services.

pub(crate) fn mint_token_value(
    paths: &crate::paths::JackinPaths,
    config: &jackin_config::AppConfig,
    scope: &crate::workspace::token_setup::TokenSetupScope,
    args: &crate::workspace::token_setup::TokenSetupArgs,
) -> anyhow::Result<jackin_core::EnvValue> {
    crate::workspace::token_setup::mint_token_value(paths, config, scope, args)
}
