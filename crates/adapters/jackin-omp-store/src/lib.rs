// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod capture;
mod query;
mod wal;

#[cfg(all(unix, any(test, feature = "test-support")))]
pub use capture::test_hooks::{CaptureHookGuard, CapturePhase, set_capture_hook};

pub use capture::{OmpSelectedAccount, OmpSnapshot};
pub use query::{OmpAccount, OmpSelector};

pub const MAX_STANDALONE_DATABASE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OmpError {
    #[error("OMP credential source is unavailable")]
    Unavailable,
    #[error("OMP credential source exceeds a resource limit")]
    LimitExceeded,
    #[error("OMP credential operation timed out")]
    Deadline,
    #[error("OMP account selection is missing or ambiguous")]
    SelectionUnavailable,
}
