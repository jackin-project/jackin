// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, TryLockError};

static CHILDREN: OnceLock<Mutex<ChildRegistry>> = OnceLock::new();
static REAPER_WAKEUP: OnceLock<fn()> = OnceLock::new();
static REGISTRY_POISONED: AtomicBool = AtomicBool::new(false);
static REAPER_WAKE_PENDING: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "test-support")]
static REGISTRY_CONTENTION_EVENTS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static COORDINATE_DEPTH: Cell<usize> = const { Cell::new(0) };
    static LOCAL_WAKE_PENDING: Cell<bool> = const { Cell::new(false) };
}

#[derive(Debug, Default)]
pub struct ChildRegistry {
    counts: HashMap<u32, Arc<AtomicUsize>>,
}

impl ChildRegistry {
    pub(crate) fn reserve_registrations(
        &mut self,
        count: usize,
    ) -> Result<(), std::collections::TryReserveError> {
        self.counts.try_reserve(count)
    }

    pub fn prepare_registration(
        &mut self,
    ) -> Result<ChildRegistrationSlot, std::collections::TryReserveError> {
        self.counts.try_reserve(1)?;
        Ok(ChildRegistrationSlot {
            count: Arc::new(AtomicUsize::new(0)),
        })
    }

    #[must_use]
    pub fn register_prepared(
        &mut self,
        pid: u32,
        slot: ChildRegistrationSlot,
    ) -> ChildRegistration {
        let count = match self.counts.entry(pid) {
            std::collections::hash_map::Entry::Occupied(entry) => Arc::clone(entry.get()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                Arc::clone(entry.insert(slot.count))
            }
        };
        count.fetch_add(1, Ordering::AcqRel);
        ChildRegistration { pid, count }
    }

    pub fn register(
        &mut self,
        pid: u32,
    ) -> Result<ChildRegistration, std::collections::TryReserveError> {
        let slot = self.prepare_registration()?;
        Ok(self.register_prepared(pid, slot))
    }

    #[must_use]
    pub fn contains(&self, pid: u32) -> bool {
        self.counts
            .get(&pid)
            .is_some_and(|count| count.load(Ordering::Acquire) > 0)
    }

    fn remove_released(&mut self) {
        self.counts
            .retain(|_, count| count.load(Ordering::Acquire) != 0);
    }
}

#[derive(Debug)]
pub struct ChildRegistrationSlot {
    count: Arc<AtomicUsize>,
}

#[derive(Debug)]
pub struct ChildRegistration {
    pid: u32,
    count: Arc<AtomicUsize>,
}

impl ChildRegistration {
    #[must_use]
    pub const fn pid(&self) -> u32 {
        self.pid
    }

    pub fn release_to_reaper(self) {
        drop(self);
    }
}

impl Drop for ChildRegistration {
    fn drop(&mut self) {
        let previous = self.count.fetch_sub(1, Ordering::AcqRel);
        if previous == 1 && REAPER_WAKEUP.get().is_some() {
            let inside_coordinate = COORDINATE_DEPTH.with(|depth| depth.get() > 0);
            if inside_coordinate {
                LOCAL_WAKE_PENDING.with(|pending| pending.set(true));
            } else {
                REAPER_WAKE_PENDING.store(true, Ordering::Release);
                flush_after_registry_release();
            }
        }
    }
}

fn flush_after_registry_release() {
    let Some(mutex) = CHILDREN.get() else {
        flush_reaper_wakeup();
        return;
    };
    let mut registry = match mutex.try_lock() {
        Ok(registry) => registry,
        Err(TryLockError::Poisoned(poisoned)) => {
            REGISTRY_POISONED.store(true, Ordering::Release);
            poisoned.into_inner()
        }
        Err(TryLockError::WouldBlock) => return,
    };
    registry.remove_released();
    drop(registry);
    flush_reaper_wakeup();
}

struct CoordinateDepthGuard;

impl CoordinateDepthGuard {
    fn enter() -> Self {
        COORDINATE_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
        Self
    }
}

impl Drop for CoordinateDepthGuard {
    fn drop(&mut self) {
        let outermost = COORDINATE_DEPTH.with(|depth| {
            let remaining = depth.get().saturating_sub(1);
            depth.set(remaining);
            remaining == 0
        });
        if outermost {
            flush_reaper_wakeup();
        }
    }
}

fn flush_reaper_wakeup() {
    let local_pending = LOCAL_WAKE_PENDING.with(|pending| pending.replace(false));
    let shared_pending = REAPER_WAKE_PENDING.swap(false, Ordering::AcqRel);
    if (local_pending || shared_pending)
        && let Some(wakeup) = REAPER_WAKEUP.get()
    {
        wakeup();
    }
}

pub fn coordinate_children<R>(operation: impl FnOnce(&mut ChildRegistry) -> R) -> R {
    let _coordinate_depth = CoordinateDepthGuard::enter();
    let mutex = CHILDREN.get_or_init(|| Mutex::new(ChildRegistry::default()));
    #[cfg(feature = "test-support")]
    let mut registry = match mutex.try_lock() {
        Ok(registry) => registry,
        Err(TryLockError::Poisoned(poisoned)) => {
            REGISTRY_POISONED.store(true, Ordering::Release);
            poisoned.into_inner()
        }
        Err(TryLockError::WouldBlock) => {
            REGISTRY_CONTENTION_EVENTS.fetch_add(1, Ordering::AcqRel);
            match mutex.lock() {
                Ok(registry) => registry,
                Err(poisoned) => {
                    REGISTRY_POISONED.store(true, Ordering::Release);
                    poisoned.into_inner()
                }
            }
        }
    };
    #[cfg(not(feature = "test-support"))]
    let mut registry = match mutex.lock() {
        Ok(registry) => registry,
        Err(poisoned) => {
            REGISTRY_POISONED.store(true, Ordering::Release);
            poisoned.into_inner()
        }
    };
    registry.remove_released();
    operation(&mut registry)
}

#[doc(hidden)]
#[cfg(feature = "test-support")]
#[must_use]
pub fn registry_contention_count() -> usize {
    REGISTRY_CONTENTION_EVENTS.load(Ordering::Acquire)
}

pub fn install_reaper_wakeup(wakeup: fn()) -> Result<(), fn()> {
    REAPER_WAKEUP.set(wakeup)
}

#[must_use]
pub fn has_reaper_wakeup() -> bool {
    REAPER_WAKEUP.get().is_some()
}

#[must_use]
pub fn take_registry_poisoned() -> bool {
    REGISTRY_POISONED.swap(false, Ordering::AcqRel)
}

#[must_use]
pub(crate) const fn supports_selective_reaping() -> bool {
    cfg!(any(
        target_os = "macos",
        all(target_os = "linux", not(target_env = "uclibc"))
    ))
}

#[cfg(test)]
mod tests;
