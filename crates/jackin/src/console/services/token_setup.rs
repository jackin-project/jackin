//! Non-TUI Claude token setup services.

pub(crate) fn mint_token_value(
    paths: &crate::paths::JackinPaths,
    config: &jackin_config::AppConfig,
    scope: &jackin_env::TokenSetupScope,
    args: &jackin_env::TokenSetupArgs,
) -> anyhow::Result<jackin_core::EnvValue> {
    jackin_env::mint_token_value(paths, config, scope, args)
}
