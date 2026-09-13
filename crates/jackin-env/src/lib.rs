//! jackin-env: environment resolution, secrets probes, and auth wiring.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`resolve_operator_env`] — layered operator env resolution.

#![deny(missing_docs)]

mod accounts;
mod env_layer;
mod env_resolver;
mod op_cli;
mod op_runner;
mod op_struct;
mod parse_helpers;
mod picker;
mod process_telemetry;
mod resolve;

pub use accounts::{is_account_env, resolve_account_env_with};
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
    CLAUDE_OAUTH_TOKEN_ENV, OperatorEnvError, OperatorEnvKeyResolution, OperatorEnvKeyStatus,
    collect_on_demand_bindings, has_operator_env, has_operator_env_matching,
    lookup_operator_env_declaration, lookup_operator_env_raw, print_launch_diagnostic,
    resolve_account_declaration, resolve_account_declaration_with, resolve_op_uri_to_ref,
    resolve_operator_env, resolve_operator_env_matching, resolve_operator_env_per_key_matching,
    resolve_operator_env_per_key_with_matching, resolve_operator_env_with,
    resolve_operator_env_with_matching, validate_reserved_names,
};
