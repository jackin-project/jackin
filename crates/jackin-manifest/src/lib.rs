//! Role manifest loading, validation, and migration.

pub mod manifest;
pub mod migrations;
pub mod repo;
pub mod repo_contract;
pub mod validate;

pub use manifest::{
    AmpConfig, ClaudeConfig, ClaudeMarketplaceConfig, CodexConfig, EnvVarDecl, HookEntry,
    HooksConfig, IdentityConfig, KimiConfig, ManifestWarning, OpencodeConfig, RoleManifest,
    load_role_manifest,
};
pub use migrations::{
    CURRENT_MANIFEST_VERSION, current_manifest_version, migrate_manifest_file,
    validate_manifest_version,
};
pub use repo::{CachedRepo, RoleRepoValidationError, ValidatedRoleRepo, validate_role_repo};
pub use repo_contract::{
    BASE_DOCKERFILE_FROM, CONSTRUCT_IMAGE, CONSTRUCT_PINNED_TAG, CONSTRUCT_REGISTRY_IMAGE,
    CONSTRUCT_STABLE_TAG, DOCKERFILE_NAME, MANIFEST_FILENAME, ValidatedDockerfile, construct_image,
    validate_agent_dockerfile,
};
pub use validate::{is_valid_env_var_name, validate_agent_consistency, validate_role_manifest};
