//! Operator-controlled env resolution: four config layers, three value
//! syntaxes (`op://`, `$NAME` / `${NAME}`, literal), and merging onto
//! the manifest-resolved env at launch.

pub trait OpRunner {
    fn read(&self, reference: &str) -> anyhow::Result<String>;

    /// Read pinned to a specific 1Password account. The production
    /// `OpCli` rebinds itself to `account` before invoking `op` so a
    /// ref whose vault lives in a non-default account resolves. Default
    /// ignores `account` and delegates to `read`, keeping mock runners
    /// trivial.
    fn read_with_account(&self, reference: &str, _account: Option<&str>) -> anyhow::Result<String> {
        self.read(reference)
    }

    /// Probed once per launch so a missing `op` surfaces as a single
    /// install-link error rather than one-per-key noise. Default no-op
    /// keeps mock runners trivial.
    fn probe(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Resolve a single [`EnvValue`] to its final string, dispatching on the
/// enum variant rather than lexical string prefix.
///
/// - `EnvValue::Plain` passes through `$VAR` / `${VAR}` expansion via
///   the host environment; bare `op://...` strings stored as `Plain` are
///   **not** resolved and flow to the container literally.
/// - `EnvValue::OpRef` shells out to `op read <op>` using the canonical
///   UUID URI; failures are wrapped with the human-readable `path` for
///   actionable error messages.
///
/// `layer_label` / `var_name` are used only in error messages.
///
/// Only structural `EnvValue::OpRef` triggers `op read`. Bare
/// `op://...` strings stored as `EnvValue::Plain` flow to the
/// container literally.
pub fn resolve_env_value<R, H>(
    layer_label: &str,
    var_name: &str,
    value: &EnvValue,
    op_runner: &R,
    host_env: H,
) -> anyhow::Result<String>
where
    R: OpRunner + ?Sized,
    H: FnMut(&str) -> Result<String, std::env::VarError>,
{
    match value {
        EnvValue::Plain(s) => dispatch_plain(layer_label, var_name, s, host_env),
        EnvValue::OpRef(r) => op_runner
            .read_with_account(&r.op, r.account.as_deref())
            .map_err(|e| {
                anyhow::anyhow!(
                    "{layer_label} env var {var_name:?}: 1Password reference {:?} failed: {e}",
                    r.path
                )
            }),
    }
}

/// Resolve a plain string value: `$NAME` / `${NAME}` → host env lookup,
/// otherwise verbatim. `op://...` strings are intentionally NOT resolved
/// here — that branch lives exclusively in [`resolve_env_value`] for
/// `EnvValue::OpRef`.
fn dispatch_plain<H>(
    layer_label: &str,
    var_name: &str,
    value: &str,
    mut host_env: H,
) -> anyhow::Result<String>
where
    H: FnMut(&str) -> Result<String, std::env::VarError>,
{
    if let Some(host_name) = parse_host_ref(value) {
        return host_env(host_name).map_err(|_| {
            anyhow::anyhow!(
                "{layer_label} env var {var_name:?}: host env var {host_name:?} is not set"
            )
        });
    }
    Ok(value.to_string())
}

/// Parse `$NAME` or `${NAME}` and return the name. Rejects bare `$`,
/// unmatched braces, and non-identifier characters.
fn parse_host_ref(value: &str) -> Option<&str> {
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

/// Re-exported from `jackin-core` — canonical definitions live there.
pub use jackin_core::{EnvValue, FieldTarget, OpRef};

pub use jackin_console::op_reference::{OpReferenceParts, parse_op_reference};

fn is_valid_env_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

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
