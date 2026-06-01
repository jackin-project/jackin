//! Non-TUI Claude token setup services.

pub fn mint_token_value(
    paths: &crate::paths::JackinPaths,
    config: &crate::config::AppConfig,
    scope: &crate::workspace::token_setup::TokenSetupScope,
    args: &crate::workspace::token_setup::TokenSetupArgs,
) -> anyhow::Result<crate::operator_env::EnvValue> {
    crate::workspace::token_setup::mint_token_value(paths, config, scope, args)
}
