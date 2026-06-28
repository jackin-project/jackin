//! Result of a one-shot user prompt (e.g. trust confirm, vault selection).
//!
//! Moved here from `jackin-launch` as part of Workstream 1
//! (architecture/boundaries) in `codebase-health-enforcement`. The
//! `launch_cockpit` TUI used to live in `jackin-launch` and owned this
//! type; lower-layer crates (notably `jackin-env::env_resolver`) had to
//! depend upward on `jackin-launch` purely to use it, inverting the
//! layering. Now both `jackin-launch` and `jackin-env` depend on
//! `jackin-core` for the type, and the dependency edge flips to point
//! inward.

/// Outcome of prompting the operator for a single value.
///
/// `Value` carries the committed input; `Skipped` means the prompt was
/// dismissed (Esc / Cancel) and the caller falls back to a default or
/// aborts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptResult {
    /// The operator confirmed and supplied a value.
    Value(String),
    /// The prompt was cancelled or skipped; no value captured.
    Skipped,
}
