//! Config persist shim — persistence logic now lives in `jackin-config`.

#[cfg(test)]
pub(crate) use crate::config::AppConfig;
#[cfg(test)]
pub(crate) use jackin_config::app_config_persist::{
    config_needs_split_migration, load_workspace_files,
};
#[cfg(test)]
pub(crate) use jackin_config::atomic_write;

#[cfg(test)]
mod tests;
