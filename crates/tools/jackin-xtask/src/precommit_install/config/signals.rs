// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, Result, ensure};

const INSTALL_SIGNALS: [i32; 3] = [
    signal_hook::consts::signal::SIGHUP,
    signal_hook::consts::signal::SIGINT,
    signal_hook::consts::signal::SIGTERM,
];

pub(super) struct InstallSignals {
    hup: Arc<AtomicBool>,
    int: Arc<AtomicBool>,
    term: Arc<AtomicBool>,
    terminal: Arc<AtomicBool>,
}

pub(super) fn with_install_signals(
    operation: impl FnOnce(&InstallSignals) -> Result<()>,
) -> Result<()> {
    let signals = InstallSignals::register()?;
    let result = operation(&signals);
    signals.terminal.store(true, Ordering::SeqCst);
    result?;
    signals.check_committed()
}

impl InstallSignals {
    fn register() -> Result<Self> {
        let signals = Self {
            hup: Arc::new(AtomicBool::new(false)),
            int: Arc::new(AtomicBool::new(false)),
            term: Arc::new(AtomicBool::new(false)),
            terminal: Arc::new(AtomicBool::new(true)),
        };
        let mut fallbacks = Vec::with_capacity(INSTALL_SIGNALS.len());
        for signal in INSTALL_SIGNALS {
            fallbacks.push(
                signal_hook::flag::register_conditional_default(
                    signal,
                    Arc::clone(&signals.terminal),
                )
                .context("registering temporary Git config signal default")?,
            );
        }
        for (signal, flag) in [
            (INSTALL_SIGNALS[0], Arc::clone(&signals.hup)),
            (INSTALL_SIGNALS[1], Arc::clone(&signals.int)),
            (INSTALL_SIGNALS[2], Arc::clone(&signals.term)),
        ] {
            signal_hook::flag::register(signal, flag)
                .context("registering Git config installation signal flags")?;
        }
        for signal in INSTALL_SIGNALS {
            signal_hook::flag::register_conditional_default(signal, Arc::clone(&signals.terminal))
                .context("registering final Git config signal default")?;
        }
        for fallback in fallbacks {
            ensure!(
                signal_hook::low_level::unregister(fallback),
                "temporary Git config signal default was not registered"
            );
        }
        Ok(signals)
    }

    pub(super) fn enter_critical_phase(&self) -> Result<()> {
        self.check()?;
        self.terminal.store(false, Ordering::SeqCst);
        Ok(())
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

    fn check_committed(&self) -> Result<()> {
        if self.term.load(Ordering::SeqCst) {
            anyhow::bail!("Git worktree config committed, but installation received SIGTERM");
        }
        if self.int.load(Ordering::SeqCst) {
            anyhow::bail!("Git worktree config committed, but installation received SIGINT");
        }
        if self.hup.load(Ordering::SeqCst) {
            anyhow::bail!("Git worktree config committed, but installation received SIGHUP");
        }
        Ok(())
    }
}
