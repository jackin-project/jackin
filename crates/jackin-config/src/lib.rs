//! jackin-config: configuration schema and workspace resolution.
//!
//! Merges the `config/` and `workspace/` modules into one crate to dissolve
//! the config↔workspace mutual cycle that prevented crate extraction. Depends
//! on `jackin-core` for the shared vocabulary types (`Agent`, `AuthForwardMode`,
//! `MountIsolation`) and provides everything above: `AppConfig`, `WorkspaceConfig`,
//! migrations, the config editor, and workspace resolution.
//!
//! **Phase 1 (current):** Self-contained auth configuration types that carry no
//! upward dependency into the binary crate. The full `AppConfig` / `WorkspaceConfig`
//! migration lands in Phase 2 after `operator_env` is extracted to `jackin-env`.

pub mod auth;

pub use auth::{
    AgentAuthConfig, AmpAuthConfig, CodexAuthConfig, GithubAuthConfig, GithubAuthMode,
    KimiAuthConfig, OpencodeAuthConfig,
};
pub use jackin_core::AuthForwardMode;
