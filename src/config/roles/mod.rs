//! Auth-forward mode resolution shim — all logic now lives in
//! `jackin-config::app_config_roles`.

#[cfg(test)]
pub(crate) use crate::config::{
    AppConfig, AuthForwardMode, GithubAuthMode, RoleSource,
    build_github_env_layers, resolve_github_mode, resolve_mode, resolve_mode_with_trace,
};
#[cfg(test)]
pub(crate) use jackin_config::app_config_roles::BUILTIN_ROLES;

#[cfg(test)]
mod resolve_mode_tests;
#[cfg(test)]
mod tests;
