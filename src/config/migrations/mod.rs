//! Config/workspace migration registry — re-exported from `jackin-config`.

// These two are re-exported publicly from `crate::config`.
pub use jackin_config::{migrate_config_file_if_needed, migrate_workspace_file_if_needed};

pub(crate) use jackin_config::{
    MigrationStep, SchemaVersion,
    doc_version, migrate_file_if_needed,
    migrate_workspace_op_account_to_refs, noop_migration,
    parse_version,
};
pub(crate) use jackin_config::versions::{LEGACY_VERSION, current_config_version};

#[cfg(test)]
pub(crate) use jackin_config::{
    Channel, Migration, apply_migrations, parse_registry_version, set_doc_version,
};
#[cfg(test)]
pub(crate) use jackin_config::migrations::{
    CONFIG_MIGRATIONS, KubernetesVersion, WORKSPACE_MIGRATIONS, assert_registry_chain,
};
#[cfg(test)]
pub(crate) use jackin_config::versions::{CURRENT_CONFIG_VERSION, CURRENT_WORKSPACE_VERSION};
#[cfg(test)]
pub(crate) use toml_edit::DocumentMut;
#[cfg(test)]
pub(crate) use std::num::NonZeroU32;

#[cfg(test)]
mod tests;
