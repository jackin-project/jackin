// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "test-support")]
use std::cell::RefCell;
#[cfg(feature = "test-support")]
use std::time::Instant;

#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapturePhase {
    BeforeDatabaseOpen {
        attempt: usize,

        pass: usize,

        deadline: Instant,
    },

    AfterDatabaseRead {
        attempt: usize,

        pass: usize,

        deadline: Instant,
    },

    AfterPairRead {
        attempt: usize,

        pass: usize,

        deadline: Instant,
    },
}

#[cfg(feature = "test-support")]
type CaptureHook = Box<dyn FnMut(CapturePhase)>;

#[cfg(feature = "test-support")]
thread_local! {
    static CAPTURE_HOOK: RefCell<Option<CaptureHook>> = const { RefCell::new(None) };
}

#[cfg(feature = "test-support")]
#[must_use = "keep the guard alive while the capture hook is needed"]
pub struct CaptureHookGuard {
    previous: Option<CaptureHook>,
}

#[cfg(feature = "test-support")]
impl std::fmt::Debug for CaptureHookGuard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CaptureHookGuard")
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "test-support")]
impl Drop for CaptureHookGuard {
    fn drop(&mut self) {
        CAPTURE_HOOK.with(|slot| {
            *slot.borrow_mut() = self.previous.take();
        });
    }
}

#[cfg(feature = "test-support")]
pub fn set_capture_hook(hook: impl FnMut(CapturePhase) + 'static) -> CaptureHookGuard {
    let previous = CAPTURE_HOOK.with(|slot| slot.replace(Some(Box::new(hook))));
    CaptureHookGuard { previous }
}

#[cfg(feature = "test-support")]
pub(super) fn notify(phase: CapturePhase) {
    CAPTURE_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().as_mut() {
            hook(phase);
        }
    });
}
