//! jackin-usage-provider-antigravity: `Antigravity` (`agy`) usage snapshot collection.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`antigravity_snapshot`] — `Antigravity` usage snapshot.
//!
//! Official read-only commands (changelog-verified, no model turn):
//! `agy -p /usage --output-format json` and `agy -p /credits --output-format json`.
//! Version-gated at >= 1.1.11: older binaries interpret an unknown slash
//! command as a prompt, so the gate is a correctness boundary, not a nicety.
//! At >= 1.2.2 the official commands are preferred over the local
//! `LanguageServerService` loopback API (tokenless break per `CodexBar`); this
//! module implements the official commands only.
//!
//! Response families (see `ref-contracts-C.md` §1):
//!
//! * Quota summary (`groups[].buckets[]`, bare, `{response: {groups}}`
//!   loopback-wrapped, or `{command: {data: {groups}}}` as the live CLI
//!   emits; `pools[]`/`buckets[]` aliases): exact `bucketId` match
//!   `gemini-5h` / `gemini-weekly` / `3p-5h` / `3p-weekly`. A parsed summary
//!   (even empty) wins over legacy shapes.
//! * Legacy per-model quota (`models{}` map with `quotaInfo`): collapses to
//!   the worst (lowest) remaining fraction per family and is 5h-only; weekly
//!   reads "No data". Internal (`isInternal`) and empty-label models are
//!   dropped, as are availability-only rows: model availability fractions are
//!   not quota, so an all-available response never becomes fabricated 100%.
//!
//! The command response may omit identity: the account label is then empty and
//!! the credential origin says so, so the broker binds the observation to the
//! runtime that produced it instead of attaching it to an arbitrary account.

mod buckets;
mod cli;
mod credits;
mod parse;
mod snapshot;
mod types;

pub use buckets::{
    antigravity_buckets, antigravity_identity_from_value, antigravity_plan_from_value,
};
pub use cli::{
    ANTIGRAVITY_KEYCHAIN_SERVICE, ANTIGRAVITY_MIN_JSON_VERSION, agy_version_supports_json,
    antigravity_cli_version, fetch_antigravity_cli_credits, fetch_antigravity_cli_usage,
    parse_agy_version,
};
pub use credits::{
    AntigravityCredits, antigravity_credits_bucket, parse_antigravity_credits_output,
};
pub use parse::{antigravity_remaining_from_entry, parse_antigravity_usage_output};
pub use snapshot::antigravity_snapshot;
#[cfg(test)]
pub(crate) use snapshot::{
    antigravity_snapshot_status, antigravity_status_view, antigravity_version_error_status,
};
pub use types::{AntigravityFamily, AntigravityPool, AntigravityUsage, AntigravityWindow};

#[cfg(test)]
mod tests;
