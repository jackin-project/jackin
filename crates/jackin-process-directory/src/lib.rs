// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Audited child descriptor setup: pinned working directories and foreground
//! process groups. All allocation and descriptor duplication happen before fork.

#[cfg(unix)]
mod unix;

#[cfg(unix)]
mod foreground;

#[cfg(unix)]
pub use unix::current_dir;

#[cfg(unix)]
pub use foreground::{
    ForegroundGuard, ForegroundRestoreError, NativeSpawnGuard, foreground_cleanup_error,
    native_spawn_guard,
};

#[cfg(all(test, unix))]
mod tests;
