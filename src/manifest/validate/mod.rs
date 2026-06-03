//! Role manifest validation re-exports — behavior now in `jackin-manifest`.

pub use jackin_manifest::validate::{
    is_valid_env_var_name, validate_agent_consistency, validate_role_manifest,
};

#[cfg(test)]
mod tests;
