//! Config persist shim — persistence logic now lives in `jackin-config`.

#[cfg(test)]
pub(crate) use crate::config::AppConfig;
#[cfg(test)]
pub(crate) use jackin_config::{atomic_write, migrate_config_file_if_needed, migrate_workspace_file_if_needed};
#[cfg(test)]
pub(crate) use jackin_config::app_config_persist::{config_needs_split_migration, load_workspace_files};

#[cfg(test)]
mod tests;
