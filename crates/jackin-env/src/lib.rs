//! jackin-env: operator-env resolution and 1Password CLI integration.
//!
//! **Phase 2 (current):** `OpRunner` trait and `resolve_env_value` moved here.
//! The full operator_env stack (`OpCli`, `OpStructRunner`, `EnvLayer`, etc.)
//! migrates in Phase 3 after `AppConfig` is in `jackin-config`.
//!
//! **Dependency tier:** `jackin-core` → `jackin-config` → `jackin-env`

pub mod env_layer;
pub mod op_runner;

pub use env_layer::{EnvLayer, merge_layers};
pub use op_runner::{OpRunner, resolve_env_value};
