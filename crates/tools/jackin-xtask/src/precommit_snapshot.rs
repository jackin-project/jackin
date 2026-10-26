// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Private staged-tree admission for the native pre-commit owner.

mod evidence;
mod git;
mod process;
mod toolchain;

#[derive(Clone, Copy)]
enum OwnerSignal {
    Hup,
    Int,
    Term,
}

struct SignalRoute {
    recovery_armed: Cell<bool>,
    cancellation: Arc<AtomicBool>,
    hup: Arc<AtomicBool>,
    int: Arc<AtomicBool>,
    term: Arc<AtomicBool>,
}

impl SignalRoute {
    #[cfg(test)]
    fn record(&self, signal: OwnerSignal) {
        match signal {
            OwnerSignal::Hup => self.hup.store(true, Ordering::SeqCst),
            OwnerSignal::Int => self.int.store(true, Ordering::SeqCst),
            OwnerSignal::Term => self.term.store(true, Ordering::SeqCst),
        }
        self.cancellation.store(true, Ordering::SeqCst);
    }
}

struct SetupSignals {
    route: SignalRoute,
    terminal: Arc<AtomicBool>,
}

#[derive(Clone)]
struct OwnerBudgets {
    setup: Duration,
    gate: Duration,
    recovery: Duration,
    #[cfg(test)]
    test_git_program: Option<std::path::PathBuf>,
}

impl OwnerBudgets {
    const fn production() -> Self {
        Self {
            setup: Duration::from_mins(10),
            gate: Duration::from_mins(30),
            recovery: Duration::from_mins(10),
            #[cfg(test)]
            test_git_program: None,
        }
    }
}

fn register_signal_flags(
    signal: i32,
    cancellation: &Arc<AtomicBool>,
    signal_status: &Arc<AtomicBool>,
) -> Result<()> {
    signal_hook::flag::register(signal, Arc::clone(signal_status))
        .context("registering native hook signal status")?;
    signal_hook::flag::register(signal, Arc::clone(cancellation))
        .context("registering native hook cancellation")?;
    Ok(())
}

impl SetupSignals {
    fn register() -> Result<Self> {
        let signals = Self {
            terminal: Arc::new(AtomicBool::new(false)),
            route: SignalRoute {
                recovery_armed: Cell::new(false),
                cancellation: Arc::new(AtomicBool::new(false)),
                hup: Arc::new(AtomicBool::new(false)),
                int: Arc::new(AtomicBool::new(false)),
                term: Arc::new(AtomicBool::new(false)),
            },
        };
        for (signal, owner_signal) in [
            (signal_hook::consts::signal::SIGHUP, OwnerSignal::Hup),
            (signal_hook::consts::signal::SIGINT, OwnerSignal::Int),
            (signal_hook::consts::signal::SIGTERM, OwnerSignal::Term),
        ] {
            let status = match owner_signal {
                OwnerSignal::Hup => &signals.route.hup,
                OwnerSignal::Int => &signals.route.int,
                OwnerSignal::Term => &signals.route.term,
            };
            register_signal_flags(signal, &signals.route.cancellation, status)?;
            signal_hook::flag::register_conditional_default(signal, Arc::clone(&signals.terminal))
                .context("registering completed native hook signal termination")?;
        }
        Ok(signals)
    }

    fn setup_gate_controls(&self, budget: Duration) -> process::SetupControls {
        process::SetupControls::new(Arc::clone(&self.route.cancellation), budget)
    }

    fn arm_recovery_with_budget(&self, budget: Duration) -> Result<process::SetupControls> {
        ensure!(
            !self.route.recovery_armed.get(),
            "native hook recovery phase was armed more than once"
        );
        self.route.cancellation.swap(false, Ordering::SeqCst);
        self.route.recovery_armed.set(true);
        Ok(process::SetupControls::new(
            Arc::clone(&self.route.cancellation),
            budget,
        ))
    }

    fn signal_status(&self) -> Option<u8> {
        if self.route.term.load(Ordering::SeqCst) {
            Some(143)
        } else if self.route.int.load(Ordering::SeqCst) {
            Some(130)
        } else if self.route.hup.load(Ordering::SeqCst) {
            Some(129)
        } else {
            None
        }
    }
}

use std::{
    cell::Cell,
    ffi::OsString,
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, ensure};

pub(crate) fn run_from_hook() -> Result<i32> {
    if std::env::var_os("HK").as_deref() == Some(std::ffi::OsStr::new("0")) {
        return Ok(0);
    }
    let arguments = std::env::args_os().skip(2).collect::<Vec<_>>();
    run_with_budgets(&arguments, OwnerBudgets::production()).map(i32::from)
}

fn run_with_budgets(arguments: &[OsString], budgets: OwnerBudgets) -> Result<u8> {
    let signals = SetupSignals::register()?;
    let result = if let Some(code) = signals.signal_status() {
        Ok(code)
    } else {
        run_with_signals(arguments, &signals, budgets)
    };
    let result = match result {
        Err(error) if process::is_clean_cancellation(&error) => {
            let _write_result = writeln!(
                std::io::stderr().lock(),
                "native hook recovery stopped after strict child drain; {}",
                format_args!("{error:#}")
            );
            if let Some(code) = signals.signal_status() {
                Ok(code)
            } else {
                Err(error)
            }
        }
        result => result,
    };
    signals.terminal.store(true, Ordering::SeqCst);
    match result {
        Ok(code) => Ok(signals.signal_status().unwrap_or(code)),
        Err(error) => Err(error),
    }
}

fn run_with_signals(
    arguments: &[OsString],
    signals: &SetupSignals,
    budgets: OwnerBudgets,
) -> Result<u8> {
    let setup_controls = signals.setup_gate_controls(budgets.setup);
    if let Some(code) = signals.signal_status() {
        return Ok(code);
    }
    let live = git::LiveRepository::capture(&setup_controls)?;
    if let Some(code) = signals.signal_status() {
        return Ok(code);
    }
    let mut snapshot = git::OwnedSnapshot::create(&live)?;
    #[cfg(test)]
    if let Some(program) = budgets.test_git_program.as_deref() {
        snapshot.set_test_git_program(program)?;
    }
    let result = (|| -> Result<u8> {
        if let Some(code) = signals.signal_status() {
            setup_controls.check_start()?;
            return Ok(code);
        }
        let tree = snapshot.capture_staged_tree()?;
        if let Some(code) = signals.signal_status() {
            setup_controls.check_start()?;
            return Ok(code);
        }
        snapshot.materialize(&tree, &live)?;
        if let Some(code) = signals.signal_status() {
            setup_controls.check_start()?;
            return Ok(code);
        }
        snapshot.admit(&tree)?;
        if let Some(code) = signals.signal_status() {
            setup_controls.check_start()?;
            return Ok(code);
        }
        let tools = toolchain::qualify(&snapshot)?;
        if let Some(code) = signals.signal_status() {
            setup_controls.check_start()?;
            return Ok(code);
        }
        let gate_controls = signals.setup_gate_controls(budgets.gate);
        let mut completion = snapshot.run_gate(&tools, arguments, &gate_controls)?;
        if !completion.cleanup.group_drained || completion.cleanup.recovery.is_some() {
            drop(completion.cleanup.recovery);
            anyhow::bail!("native hook process cleanup is uncertain");
        }
        if completion.execution.timed_out {
            anyhow::bail!("native pre-commit gate exceeded its phase deadline");
        }
        ensure!(
            completion.capture.pipes.both_reached_eof(),
            "native pre-commit gate output pipes did not reach EOF"
        );
        ensure!(
            completion.failures.is_empty(),
            "native pre-commit gate capture reported failures: {:?}",
            completion.failures
        );
        if !completion.execution.cancelled {
            ensure!(
                completion.capture.complete,
                "native pre-commit gate capture did not complete"
            );
        }
        if let Some(path) = retain_unsuccessful_completion(&mut snapshot, completion.success) {
            let _write_result = writeln!(
                std::io::stderr().lock(),
                "native pre-commit gate was unsuccessful; private snapshot retained at {}",
                path.display()
            );
        }
        if !completion.success {
            snapshot.report_gate_output();
        }
        let recovery_controls = signals.arm_recovery_with_budget(budgets.recovery)?;
        git::OwnedSnapshot::verify_live_index(&live, &recovery_controls)?;
        let fixes = snapshot.collect_fixes(&tree, &recovery_controls)?;
        let evidence = if fixes.is_empty() {
            None
        } else {
            write_private_fixes_spool(&snapshot, &fixes, &recovery_controls)?;
            Some(evidence::persist(
                &live.common_git_dir,
                &fixes,
                &recovery_controls,
            )?)
        };
        drop(completion.cleanup.recovery.take());
        let staged_fixes = evidence.is_some();
        if let Some(path) = evidence {
            let mut stderr = std::io::stderr().lock();
            writeln!(
                stderr,
                "error: hook changed staged source; review private patch: {}",
                path.display()
            )
            .context("reporting retained staged-source patch")?;
            writeln!(
                stderr,
                "Run mise x -- hk fix, review and stage the fixes, then retry the commit."
            )
            .context("reporting staged-source recovery instructions")?;
        }
        git::OwnedSnapshot::verify_live_index(&live, &recovery_controls)?;
        if let Some(code) = signals.signal_status() {
            return Ok(code);
        }
        if staged_fixes && completion.code == Some(0) {
            return Ok(1);
        }
        let exit_code =
            completion_exit_code(completion.success, completion.code, completion.signal)?;
        Ok(exit_code)
    })();

    result.map_err(|error| retain_snapshot_failure(&mut snapshot, error))
}

fn retain_snapshot_failure(
    snapshot: &mut git::OwnedSnapshot,
    error: anyhow::Error,
) -> anyhow::Error {
    let retained = snapshot
        .retain()
        .unwrap_or_else(|| snapshot.scratch_root.clone());
    error.context(format!(
        "private snapshot retained at {}",
        retained.display()
    ))
}

fn retain_unsuccessful_completion(
    snapshot: &mut git::OwnedSnapshot,
    success: bool,
) -> Option<std::path::PathBuf> {
    if success {
        return None;
    }
    Some(
        snapshot
            .retain()
            .unwrap_or_else(|| snapshot.scratch_root.clone()),
    )
}

fn write_private_fixes_spool(
    snapshot: &git::OwnedSnapshot,
    fixes: &[u8],
    controls: &process::SetupControls,
) -> Result<std::path::PathBuf> {
    controls.check_start()?;
    anyhow::ensure!(!fixes.is_empty(), "refusing to spool empty hook fixes");
    let path = snapshot.scratch_root.join("fixes.patch");
    let mut file = process::create_private_new(&path).context("creating private fixes spool")?;
    evidence::write_controlled(&mut file, fixes, controls)
        .context("writing private fixes spool")?;
    controls.check_start()?;
    file.sync_all().context("syncing private fixes spool")?;
    controls.check_start()?;
    Ok(path)
}

fn completion_exit_code(success: bool, code: Option<i32>, signal: Option<i32>) -> Result<u8> {
    if success {
        return Ok(0);
    }

    if let Some(code) = code.filter(|code| (0..=255).contains(code)) {
        let code = u8::try_from(code).context("validating hook exit code")?;
        if code == 0 {
            return Ok(1);
        }
        return Ok(code);
    }

    if let Some(signal) = signal {
        let code = signal.saturating_add(128).clamp(1, 255);
        return u8::try_from(code).context("validating hook signal status");
    }

    anyhow::bail!("native pre-commit gate did not complete successfully")
}

#[cfg(test)]
mod tests;
