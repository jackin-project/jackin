// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[cfg(any(test, feature = "test-support"))]
use std::cell::RefCell;
#[cfg(any(test, feature = "test-support"))]
use std::time::Instant;

/// A typed point reached during the secure source-pair capture.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapturePhase {
    /// The nofollow stat succeeded and the database leaf is about to be opened.
    BeforeDatabaseOpen {
        /// Zero-based capture attempt.
        attempt: usize,
        /// Zero-based complete-pair read within the attempt.
        pass: usize,
        /// The operation deadline created by the public capture entry point.
        deadline: Instant,
    },
    /// The database was read completely and the WAL read has not started.
    AfterDatabaseRead {
        /// Zero-based capture attempt.
        attempt: usize,
        /// Zero-based complete-pair read within the attempt.
        pass: usize,
        /// The operation deadline created by the public capture entry point.
        deadline: Instant,
    },
    /// One complete pair read returned successfully, including stable absence.
    AfterPairRead {
        /// Zero-based capture attempt.
        attempt: usize,
        /// Zero-based complete-pair read within the attempt.
        pass: usize,
        /// The operation deadline created by the public capture entry point.
        deadline: Instant,
    },
}

#[cfg(any(test, feature = "test-support"))]
type CaptureHook = Box<dyn FnMut(CapturePhase)>;

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static CAPTURE_HOOK: RefCell<Option<CaptureHook>> = const { RefCell::new(None) };
}

/// Scoped owner of an installed capture hook.
#[cfg(any(test, feature = "test-support"))]
#[must_use = "keep the guard alive while the capture hook is needed"]
pub struct CaptureHookGuard {
    previous: Option<CaptureHook>,
}

#[cfg(any(test, feature = "test-support"))]
impl std::fmt::Debug for CaptureHookGuard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CaptureHookGuard")
            .finish_non_exhaustive()
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for CaptureHookGuard {
    fn drop(&mut self) {
        CAPTURE_HOOK.with(|slot| {
            *slot.borrow_mut() = self.previous.take();
        });
    }
}

/// Install a thread-local capture callback and restore its prior value on drop.
#[cfg(any(test, feature = "test-support"))]
pub fn set_capture_hook(hook: impl FnMut(CapturePhase) + 'static) -> CaptureHookGuard {
    let previous = CAPTURE_HOOK.with(|slot| slot.replace(Some(Box::new(hook))));
    CaptureHookGuard { previous }
}

#[cfg(any(test, feature = "test-support"))]
pub(super) fn notify(phase: CapturePhase) {
    CAPTURE_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().as_mut() {
            hook(phase);
        }
    });
}
