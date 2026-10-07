//! jackin-runtime-launch-progress-helpers: launch progress step helpers.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`progress_helpers::StepCounter`] — step counter.
//!
//! Launch progress, prompt, and summary helpers: the step counter with
//! stage telemetry, the env prompter bridge, and target/mount label
//! rendering. Split out of `jackin-runtime` (S7 split 72); the old
//! `jackin_runtime::runtime::launch::progress_helpers::*` paths keep
//! working through a module re-export in `launch.rs`.

pub mod progress_helpers;
