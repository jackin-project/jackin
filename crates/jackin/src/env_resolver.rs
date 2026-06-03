//! Resolve manifest `env` declarations (prompts, defaults, interpolation) into concrete `(name, value)` pairs.
//!
//! Moved to `crates/jackin-env/src/env_resolver.rs`.

pub use jackin_env::{
    EnvPrompter, PromptResult, ResolvedEnv, resolve_env, resolve_env_with_overrides,
};

#[cfg(test)]
mod tests;
