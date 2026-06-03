//! Config editor shim — `ConfigEditor` and `EnvScope` now live in `jackin-config`.

pub use jackin_config::{ConfigEditor, EnvScope};

#[cfg(test)]
pub(crate) use crate::config::AppConfig;
#[cfg(test)]
pub(crate) use jackin_core::JackinPaths;

#[cfg(test)]
mod tests;
