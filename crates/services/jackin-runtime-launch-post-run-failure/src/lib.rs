//! jackin-runtime-launch-post-run-failure: post-run failure emit helper.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`post_run_failure::emit_post_run_failure`] —
//! firewall telemetry for failed post-run steps.
//!
//! Split out of `jackin-runtime` (S7 split 117): the post-run
//! failure emit in the launch runtime post-run path.
//! The hub keeps a private import; the step
//! stays exercised through the same launch path.

pub mod post_run_failure;
