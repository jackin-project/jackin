//! jackin-env: environment resolution, secrets probes, and auth wiring.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`resolve`] — environment resolution entry.

mod env_layer;
mod env_resolver;
mod host_claude;
mod op_cli;
mod op_runner;
mod op_struct;
mod output;
mod parse_helpers;
mod picker;
mod resolve;
mod token_setup;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use env_resolver::{
    EnvPrompter, PromptResult, ResolveEnvError, ResolvedEnv, resolve_env,
    resolve_env_with_overrides,
};
pub use op_cli::OpCli;
pub use op_runner::{OpRunner, resolve_env_value};
pub use op_struct::{OpItemCreateParams, OpStructRunner, OpWriteRunner};
pub use parse_helpers::parse_host_ref;
pub use picker::{OpAccount, OpCache, OpField, OpItem, OpVault, default_op_struct_runner};
pub use resolve::{
    CLAUDE_OAUTH_TOKEN_ENV, OperatorEnvError, collect_on_demand_bindings, has_operator_env,
    has_operator_env_matching, lookup_operator_env_raw, print_launch_diagnostic,
    resolve_op_uri_to_ref, resolve_operator_env, resolve_operator_env_matching,
    resolve_operator_env_with, resolve_operator_env_with_matching, validate_reserved_names,
};
pub use token_setup::{
    DEFAULT_FIELD_LABEL, DEFAULT_ITEM_TEMPLATE, DoctorReport, EditExistingTarget, JACKIN_TAG,
    RevokeReport, TokenSetupArgs, TokenSetupReport, TokenSetupScope, expiry_days_for_launch,
    mint_token_value, prior_token_slot, run_doctor, run_revoke, run_setup,
    tags_indicate_jackin_owned, vault_for_rotate,
};
