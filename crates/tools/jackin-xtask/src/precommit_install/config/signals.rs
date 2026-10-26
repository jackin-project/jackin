// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{error::Error, fmt};

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
    let committed_result = signals.check_committed();
    signals.terminal.store(true, Ordering::SeqCst);
    if let Err(error) = result {
        if let Some(interruption) = error
            .downcast_ref::<InstallInterruption>()
            .filter(|interruption| !interruption.committed)
            && let Err(default_error) =
                signal_hook::low_level::emulate_default_handler(interruption.signal.as_raw())
        {
            return Err(error.context(format!(
                "replaying deferred Git config signal failed: {default_error}"
            )));
        }
        return Err(error);
    }
    committed_result?;
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
        signals.terminal.store(false, Ordering::SeqCst);
        Ok(signals)
    }

    pub(super) fn enter_critical_phase(&self) -> Result<()> {
        self.check()?;
        self.terminal.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub(super) fn check(&self) -> Result<()> {
        if self.term.load(Ordering::SeqCst) {
            return Err(InstallInterruption::new(InstallSignal::Term, false).into());
        }
        if self.int.load(Ordering::SeqCst) {
            return Err(InstallInterruption::new(InstallSignal::Int, false).into());
        }
        if self.hup.load(Ordering::SeqCst) {
            return Err(InstallInterruption::new(InstallSignal::Hup, false).into());
        }
        Ok(())
    }

    fn check_committed(&self) -> Result<()> {
        if self.term.load(Ordering::SeqCst) {
            return Err(InstallInterruption::new(InstallSignal::Term, true).into());
        }
        if self.int.load(Ordering::SeqCst) {
            return Err(InstallInterruption::new(InstallSignal::Int, true).into());
        }
        if self.hup.load(Ordering::SeqCst) {
            return Err(InstallInterruption::new(InstallSignal::Hup, true).into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum InstallSignal {
    Hup,
    Int,
    Term,
}

impl InstallSignal {
    fn as_raw(self) -> i32 {
        match self {
            Self::Hup => signal_hook::consts::signal::SIGHUP,
            Self::Int => signal_hook::consts::signal::SIGINT,
            Self::Term => signal_hook::consts::signal::SIGTERM,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Hup => "SIGHUP",
            Self::Int => "SIGINT",
            Self::Term => "SIGTERM",
        }
    }
}

#[derive(Debug)]
struct InstallInterruption {
    signal: InstallSignal,
    committed: bool,
}

impl InstallInterruption {
    fn new(signal: InstallSignal, committed: bool) -> Self {
        Self { signal, committed }
    }
}

impl fmt::Display for InstallInterruption {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.committed {
            write!(
                formatter,
                "Git worktree config committed, but installation received {}",
                self.signal.name()
            )
        } else {
            write!(
                formatter,
                "Git config installation interrupted by {}",
                self.signal.name()
            )
        }
    }
}

impl Error for InstallInterruption {}
