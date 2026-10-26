// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, Result};

pub(super) struct InstallSignals {
    hup: Arc<AtomicBool>,
    int: Arc<AtomicBool>,
    term: Arc<AtomicBool>,
    registrations: Vec<signal_hook::SigId>,
}

impl InstallSignals {
    pub(super) fn register() -> Result<Self> {
        let mut signals = Self {
            hup: Arc::new(AtomicBool::new(false)),
            int: Arc::new(AtomicBool::new(false)),
            term: Arc::new(AtomicBool::new(false)),
            registrations: Vec::with_capacity(3),
        };
        for (signal, flag) in [
            (
                signal_hook::consts::signal::SIGHUP,
                Arc::clone(&signals.hup),
            ),
            (
                signal_hook::consts::signal::SIGINT,
                Arc::clone(&signals.int),
            ),
            (
                signal_hook::consts::signal::SIGTERM,
                Arc::clone(&signals.term),
            ),
        ] {
            match signal_hook::flag::register(signal, flag) {
                Ok(registration) => signals.registrations.push(registration),
                Err(error) => {
                    return Err(error).context("registering Git config installation signal flags");
                }
            }
        }
        Ok(signals)
    }

    pub(super) fn check(&self) -> Result<()> {
        if self.term.load(Ordering::SeqCst) {
            anyhow::bail!("Git config installation interrupted by SIGTERM");
        }
        if self.int.load(Ordering::SeqCst) {
            anyhow::bail!("Git config installation interrupted by SIGINT");
        }
        if self.hup.load(Ordering::SeqCst) {
            anyhow::bail!("Git config installation interrupted by SIGHUP");
        }
        Ok(())
    }
}

impl Drop for InstallSignals {
    fn drop(&mut self) {
        for registration in self.registrations.drain(..) {
            signal_hook::low_level::unregister(registration);
        }
    }
}
