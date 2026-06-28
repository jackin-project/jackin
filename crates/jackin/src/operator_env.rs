//! Operator-controlled env resolution: four config layers, three value
//! syntaxes (`op://`, `$NAME` / `${NAME}`, literal), and merging onto
//! the manifest-resolved env at launch.

/// Re-exported from `jackin-env` — canonical definitions live there.
pub use jackin_env::{OpRunner, resolve_env_value};

/// Re-exported from `jackin-core` — canonical definitions live there.
pub use jackin_core::{EnvValue, FieldTarget, OpRef};

pub use jackin_env::{is_valid_env_name, parse_host_ref};

pub use jackin_core::op_reference::{OpReferenceParts, parse_op_reference};

pub use jackin_env::{
    OpAccount, OpCache, OpField, OpItem, OpItemCreateParams, OpStructRunner, OpVault,
    OpWriteRunner, default_op_struct_runner,
};

pub use jackin_env::OpCli;

pub use jackin_env::{
    CLAUDE_OAUTH_TOKEN_ENV, EnvLayer, lookup_operator_env_raw, merge_layers,
    print_launch_diagnostic, resolve_op_uri_to_ref, resolve_operator_env,
    resolve_operator_env_with, validate_reserved_names,
};

/// Re-export of `jackin_env::test_support` (the shared `FakeOpWriter`
/// fake used by rotate-cleanup tests). Available under `test-support`
/// or when compiling tests (dev-dependencies enable the feature).
#[cfg(any(test, feature = "test-support"))]
pub use jackin_env::test_support;
