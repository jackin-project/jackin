//! Auth-forward mode resolution shim — all logic now lives in
//! `jackin-config::app_config_roles`.

#[cfg(test)]
pub(crate) use crate::config::{AppConfig, AuthForwardMode, build_github_env_layers, resolve_mode};

#[cfg(test)]
mod resolve_mode_tests;
#[cfg(test)]
mod tests;
