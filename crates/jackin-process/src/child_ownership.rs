// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Coordinate child owners with a process-wide orphan reaper.
//!
//! Hold [`coordinate`] across spawn and registration, and across reaper
//! inspection and waitpid. A registration reserves the child for its owner
//! until that owner has waited or killed it and handed off reaping.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static CHILDREN: OnceLock<Mutex<ChildRegistry>> = OnceLock::new();
static REAPER_WAKEUP: OnceLock<fn()> = OnceLock::new();

/// Install the PID 1 wakeup used after an owned head leaves the zombie queue.
/// A release wake prevents coalesced SIGCHLD from stranding orphans behind it.
pub fn install_reaper_wakeup(wakeup: fn()) {
    let _already_installed = REAPER_WAKEUP.set(wakeup);
}

/// Child PIDs whose exit status still belongs to a specific owner.
#[derive(Debug, Default)]
pub struct ChildRegistry {
    children: HashMap<u32, usize>,
}

impl ChildRegistry {
    /// Reserve a child while still holding the spawn coordination lock.
    /// Counted reservations also cover rapid PID reuse between a native wait
    /// and the previous owner's registration destructor.
    #[must_use]
    pub fn register(&mut self, pid: u32) -> ChildRegistration {
        *self.children.entry(pid).or_default() += 1;
        ChildRegistration { pid }
    }

    /// Whether the orphan reaper must leave this child waitable.
    #[must_use]
    pub fn contains(&self, pid: u32) -> bool {
        self.children.contains_key(&pid)
    }

    /// Whether any child currently has an explicit owner.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }
}

/// Reserve a child's status until its owner completes its disposition.
///
/// Drop only after waiting, or after sending the final termination signal and
/// giving reaping to Tokio/PID 1. No later signal may use the released PID.
#[derive(Debug)]
pub struct ChildRegistration {
    pid: u32,
}

impl Drop for ChildRegistration {
    fn drop(&mut self) {
        coordinate(|registry| {
            if let Some(owners) = registry.children.get_mut(&self.pid) {
                *owners -= 1;
                if *owners == 0 {
                    registry.children.remove(&self.pid);
                }
            }
        });
        if let Some(wakeup) = REAPER_WAKEUP.get() {
            wakeup();
        }
    }
}

/// Serialize spawning and owner registration with orphan inspection/reaping.
///
/// The closure must not drop an existing registration: its destructor takes
/// this same lock. Poison recovery preserves existing reservations rather than
/// exposing owned children to the orphan reaper.
pub fn coordinate<R>(operation: impl FnOnce(&mut ChildRegistry) -> R) -> R {
    let mut registry = CHILDREN
        .get_or_init(|| Mutex::new(ChildRegistry::default()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    operation(&mut registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_reservation_cannot_release_newer_owner_of_reused_pid() {
        let (older, newer) =
            coordinate(|registry| (registry.register(u32::MAX), registry.register(u32::MAX)));
        drop(older);
        assert!(coordinate(|registry| registry.contains(u32::MAX)));
        drop(newer);
        assert!(!coordinate(|registry| registry.contains(u32::MAX)));
    }
}
