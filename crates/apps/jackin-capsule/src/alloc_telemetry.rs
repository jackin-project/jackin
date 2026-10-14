// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
