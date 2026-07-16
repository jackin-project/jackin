// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Opt-in heap allocation telemetry for live capsule performance smokes.
//!
//! Normal capsule builds compile these helpers to no-ops. A capsule built with
//! `--features dhat-heap` and launched with `JACKIN_DHAT_ALLOC_LOG=1` starts a
//! DHAT heap profile guard in testing mode, allowing selected hot paths to log
//! per-frame allocation deltas to stderr without writing profile
//! artifacts on process exit.

#[cfg(feature = "dhat-heap")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "dhat-heap")]
static ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HeapSnapshot {
    total_blocks: u64,
    total_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HeapDelta {
    pub(crate) blocks: u64,
    pub(crate) bytes: u64,
}

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
    let profiler = dhat::Profiler::builder().testing().build();
    ENABLED.store(true, Ordering::Relaxed);
    jackin_diagnostics::telemetry_info!(
        "capsule",
        "dhat allocation telemetry enabled: direct-grid-patch frames log encoder and frame allocation deltas"
    );
    Some(profiler)
}

#[cfg(not(feature = "dhat-heap"))]
fn init_enabled_profiler() -> Option<LiveProfiler> {
    jackin_diagnostics::telemetry_info!(
        "capsule",
        "JACKIN_DHAT_ALLOC_LOG ignored: jackin-capsule was not built with --features dhat-heap"
    );
    None
}

pub(crate) fn snapshot() -> Option<HeapSnapshot> {
    snapshot_enabled()
}

#[cfg(feature = "dhat-heap")]
fn snapshot_enabled() -> Option<HeapSnapshot> {
    if !ENABLED.load(Ordering::Relaxed) {
        return None;
    }
    let stats = dhat::HeapStats::get();
    Some(HeapSnapshot {
        total_blocks: stats.total_blocks,
        total_bytes: stats.total_bytes,
    })
}

#[cfg(not(feature = "dhat-heap"))]
fn snapshot_enabled() -> Option<HeapSnapshot> {
    None
}

pub(crate) fn delta_since(before: Option<HeapSnapshot>) -> Option<HeapDelta> {
    let before = before?;
    let after = snapshot()?;
    Some(HeapDelta {
        blocks: after.total_blocks.saturating_sub(before.total_blocks),
        bytes: after.total_bytes.saturating_sub(before.total_bytes),
    })
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

#[cfg(test)]
mod tests;
