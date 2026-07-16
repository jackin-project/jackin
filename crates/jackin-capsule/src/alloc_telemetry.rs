// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Opt-in heap allocation telemetry for live capsule performance smokes.
//!
//! Normal capsule builds compile these helpers to no-ops. A capsule built with
//! `--features dhat-heap` and launched with `JACKIN_DHAT_ALLOC_LOG=1` starts a
//! DHAT heap profile guard in testing mode without writing profile artifacts
//! on process exit. Allocation assertions live in the dedicated integration
//! test so production rendering does not sample or narrate heap state.

#[cfg(feature = "dhat-heap")]
pub(crate) type LiveProfiler = dhat::Profiler;

#[cfg(not(feature = "dhat-heap"))]
pub(crate) type LiveProfiler = ();

pub(crate) fn init_from_env() -> Option<LiveProfiler> {
    if !env_truthy("JACKIN_DHAT_ALLOC_LOG") {
        return None;
    }

    init_enabled_profiler()
}

#[cfg(feature = "dhat-heap")]
fn init_enabled_profiler() -> Option<LiveProfiler> {
    Some(dhat::Profiler::builder().testing().build())
}

#[cfg(not(feature = "dhat-heap"))]
fn init_enabled_profiler() -> Option<LiveProfiler> {
    None
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}
