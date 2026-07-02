//! jackin-env: operator-env resolution and 1Password CLI integration.
//!
//! **Phase 3 (current):** Full `operator_env` stack extracted here.
//!
//! **Dependency tier:** `jackin-core` → `jackin-config` → `jackin-env`
//!
//! **Architecture Invariant:** L1 application crate. Allowed dependencies:
//! `jackin-core`, `jackin-config`, `jackin-protocol`, `jackin-diagnostics`.
//! Operator-env types (`PromptResult`, `OpCache`) live here in the
//! domain/infra layer so presentation crates (`jackin-launch-tui`,
//! `jackin-console`) reach them through `jackin-env` rather than reaching
//! into each other.

pub mod env_layer;
pub mod env_resolver;
pub mod host_claude;
pub mod op_cli;
pub mod op_runner;
pub mod op_struct;
mod output;
pub mod parse_helpers;
pub mod picker;
pub mod resolve;
pub mod token_setup;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use env_layer::{EnvLayer, merge_layers};
pub use env_resolver::{
    EnvPrompter, PromptResult, ResolvedEnv, resolve_env, resolve_env_with_overrides,
};
pub use host_claude::{
    ClaudeProbe, TOKEN_PREFIX, capture_setup_token, capture_setup_token_with_binary,
    probe_claude_cli, probe_with_binary,
};
pub use op_cli::OpCli;
pub use op_runner::{OpRunner, resolve_env_value};
pub use op_struct::{OpItemCreateParams, OpStructRunner, OpWriteRunner};
pub use parse_helpers::{is_valid_env_name, parse_host_ref};
pub use picker::{OpAccount, OpCache, OpField, OpItem, OpVault, default_op_struct_runner};
pub use resolve::{
    CLAUDE_OAUTH_TOKEN_ENV, collect_on_demand_bindings, has_operator_env,
    has_operator_env_matching, lookup_operator_env_raw, print_launch_diagnostic,
    resolve_op_uri_to_ref, resolve_operator_env, resolve_operator_env_matching,
    resolve_operator_env_with, resolve_operator_env_with_matching, validate_reserved_names,
};
pub use token_setup::{
    DEFAULT_FIELD_LABEL, DEFAULT_ITEM_CATEGORY, DEFAULT_ITEM_TEMPLATE, DoctorReport,
    EditExistingTarget, JACKIN_TAG, RevokeReport, TokenSetupArgs, TokenSetupReport,
    TokenSetupScope, WORKSPACE_TAG_PREFIX, clear_expiry_stamp, days_until_expiry,
    expiry_cache_path, expiry_days_for_launch, mint_token_value, prior_token_slot, run_doctor,
    run_doctor_with_runner, run_revoke, run_revoke_with_runner, run_setup, run_setup_with_runner,
    tags_indicate_jackin_owned, vault_for_rotate, write_expiry_stamp,
};
