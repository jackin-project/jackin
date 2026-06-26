//! Schema version constants and serde default helpers.
//!
//! These are the canonical version strings for the config and workspace schema.
//! Migration logic (the TOML transformation chains) lives in `migrations.rs`;
//! these constants are the shared reference point.

pub const CURRENT_CONFIG_VERSION: &str = "v1alpha8";
pub const CURRENT_WORKSPACE_VERSION: &str = "v1alpha8";
pub const LEGACY_VERSION: &str = "legacy";

/// Serde default for `AppConfig::version`.
pub fn current_config_version() -> String {
    CURRENT_CONFIG_VERSION.to_owned()
}

/// Serde default for `WorkspaceConfig::version`.
pub fn current_workspace_version() -> String {
    CURRENT_WORKSPACE_VERSION.to_owned()
}
