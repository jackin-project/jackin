//! Role manifest re-exports — behavior now in `jackin-manifest`.

pub use jackin_core::env_model::{JACKIN_DIND_HOSTNAME_ENV_NAME, JACKIN_ENV_NAME, JACKIN_ENV_VALUE};
pub use jackin_manifest::manifest::{
    AmpConfig, ClaudeConfig, ClaudeMarketplaceConfig, CodexConfig, EnvVarDecl, HookEntry,
    HooksConfig, IdentityConfig, KimiConfig, ManifestWarning, OpencodeConfig, RoleManifest,
    load_role_manifest,
};

pub mod migrations;
pub mod validate;
