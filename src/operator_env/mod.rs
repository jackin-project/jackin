//! Operator-controlled env resolution: four config layers, three value
//! syntaxes (`op://`, `$NAME` / `${NAME}`, literal), and merging onto
//! the manifest-resolved env at launch.

/// Re-exported from `jackin-env` — canonical definitions live there.
pub use jackin_env::{OpRunner, resolve_env_value};

/// Re-exported from `jackin-core` — canonical definitions live there.
pub use jackin_core::{EnvValue, FieldTarget, OpRef};

pub(crate) fn parse_host_ref(value: &str) -> Option<&str> {
    if let Some(rest) = value.strip_prefix("${")
        && let Some(name) = rest.strip_suffix('}')
        && is_valid_env_name(name)
    {
        return Some(name);
    }
    if let Some(name) = value.strip_prefix('$')
        && !name.is_empty()
        && is_valid_env_name(name)
    {
        return Some(name);
    }
    None
}

pub(crate) fn is_valid_env_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub use jackin_console::op_reference::{OpReferenceParts, parse_op_reference};

mod picker;
pub use picker::{
    OpAccount, OpCache, OpField, OpItem, OpItemCreateParams, OpStructRunner, OpVault,
    OpWriteRunner, default_op_struct_runner,
};

mod cli;
pub use cli::OpCli;

#[cfg(test)]
pub(crate) use cli::{OP_STDERR_MAX, truncate_stderr};
#[cfg(test)]
pub(crate) use picker::{RawOpField, apply_field_edit, op_section_id, resolve_edited_field_ref};

mod resolve;
pub use resolve::{
    CLAUDE_OAUTH_TOKEN_ENV, EnvLayer, lookup_operator_env_raw, merge_layers,
    print_launch_diagnostic, resolve_op_uri_to_ref, resolve_operator_env,
    resolve_operator_env_with, validate_reserved_names,
};
#[cfg(test)]
use resolve::{emit_launch_diagnostic, format_launch_diagnostic_for_test};

#[cfg(test)]
mod tests;
