//! Schema version constants and serde default helpers.
//!
//! These are the canonical version strings for the config and workspace schema.
//! Migration logic (the TOML transformation chains) lives in the binary crate's
//! `config/migrations/` module; these constants are the shared reference point.

pub const CURRENT_CONFIG_VERSION: &str = "v1alpha5";
pub const CURRENT_WORKSPACE_VERSION: &str = "v1alpha5";

/// Serde default for `AppConfig::version`.
pub fn current_config_version() -> String {
    CURRENT_CONFIG_VERSION.to_string()
}

/// Serde default for `WorkspaceConfig::version`.
pub fn current_workspace_version() -> String {
    CURRENT_WORKSPACE_VERSION.to_string()
}
