// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Private staged-tree admission for the native pre-commit owner.

mod evidence;
mod git;
mod install;
mod process;
mod toolchain;

/// Validate bytes produced by the provisioner against the runtime's exact
/// immutable-install manifest parser.
pub(crate) fn validate_provision_manifest(bytes: &[u8]) -> Result<()> {
    install::validate_provision_manifest(bytes)
}

/// Require the tools used by the bootstrap to resolve exactly as the staged
/// source's pinned Mise and Rust configuration specifies.
pub(crate) fn verify_provision_tool_paths(
    source_root: &std::path::Path,
    cargo: &std::path::Path,
    rustc: &std::path::Path,
) -> Result<()> {
    toolchain::verify_provision_tool_paths(source_root, cargo, rustc)
}

struct SetupSignals {
    cancellation: Arc<AtomicBool>,
    hup: Arc<AtomicBool>,
    int: Arc<AtomicBool>,
    term: Arc<AtomicBool>,
}

impl SetupSignals {
    fn register() -> Result<Self> {
        let signals = Self {
            cancellation: Arc::new(AtomicBool::new(false)),
            hup: Arc::new(AtomicBool::new(false)),
            int: Arc::new(AtomicBool::new(false)),
            term: Arc::new(AtomicBool::new(false)),
        };
        for (signal, flag) in [
            (signal_hook::consts::signal::SIGHUP, &signals.hup),
            (signal_hook::consts::signal::SIGINT, &signals.int),
            (signal_hook::consts::signal::SIGTERM, &signals.term),
            (signal_hook::consts::signal::SIGHUP, &signals.cancellation),
            (signal_hook::consts::signal::SIGINT, &signals.cancellation),
            (signal_hook::consts::signal::SIGTERM, &signals.cancellation),
        ] {
            flag::register(signal, Arc::clone(flag))
                .context("registering native hook cancellation")?;
        }
        Ok(signals)
    }

    fn signal_status(&self) -> Option<u8> {
        if self.term.load(std::sync::atomic::Ordering::Relaxed) {
            Some(143)
        } else if self.int.load(std::sync::atomic::Ordering::Relaxed) {
            Some(130)
        } else if self.hup.load(std::sync::atomic::Ordering::Relaxed) {
            Some(129)
        } else {
            None
        }
    }

    fn exit_code(&self) -> Option<ExitCode> {
        self.signal_status().map(ExitCode::from)
    }
}

use std::{
    ffi::OsString,
    io::Write,
    process::ExitCode,
    sync::{Arc, atomic::AtomicBool},
};

use anyhow::{Context, Result};
use signal_hook::flag;

pub(crate) fn run_installed_from_hook(
    generation: &std::path::Path,
    arguments: &[OsString],
) -> Result<ExitCode> {
    // Install handlers before repository capture or private snapshot setup can
    // allocate resources. Setup commands remain synchronous; each phase checks
    // cancellation after its child has exited and been reaped.
    let signals = SetupSignals::register()?;
    if let Some(code) = signals.exit_code() {
        return Ok(code);
    }
    let live = git::LiveRepository::capture()?;
    if let Some(code) = signals.exit_code() {
        return Ok(code);
    }
    let mut installation = install::NativeInstall::open(&live.common_git_dir, generation)?;
    installation.finalize_if_committed(live.head_tree.as_deref())?;
    if let Some(code) = signals.exit_code() {
        return Ok(code);
    }
    let mut snapshot = git::OwnedSnapshot::create(&live)?;
    let result = (|| -> Result<ExitCode> {
        if let Some(code) = signals.exit_code() {
            return Ok(code);
        }
        let tree = snapshot.capture_staged_tree()?;
        if let Some(code) = signals.exit_code() {
            return Ok(code);
        }
        installation.check_pending_tree(&tree.to_string_lossy())?;
        snapshot.materialize(&tree, &live)?;
        if let Some(code) = signals.exit_code() {
            return Ok(code);
        }
        snapshot.admit(&tree)?;
        if let Some(code) = signals.exit_code() {
            return Ok(code);
        }
        let tools = toolchain::qualify(&snapshot)?;
        if let Some(code) = signals.exit_code() {
            return Ok(code);
        }
        installation.verify_tool_hashes(&tools.cargo, &tools.rustc)?;
        if let Some(code) = signals.exit_code() {
            return Ok(code);
        }
        let completion = snapshot.run_gate(&tools, arguments, &signals.cancellation)?;
        if !completion.cleanup.group_drained || completion.cleanup.recovery.is_some() {
            drop(completion.cleanup.recovery);
            anyhow::bail!("native hook process cleanup is uncertain");
        }
        git::OwnedSnapshot::verify_live_index(&live)?;
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
        let fixes = snapshot.collect_fixes(&tree)?;
        let evidence = if fixes.is_empty() {
            None
        } else {
            write_private_fixes_spool(&snapshot, &fixes)?;
            Some(evidence::persist(&live.common_git_dir, &fixes)?)
        };
        drop(completion.cleanup.recovery);
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
            if completion.code == Some(0) {
                return Ok(ExitCode::FAILURE);
            }
        }
        if let Some(code) = signals.exit_code() {
            return Ok(code);
        }
        let exit_code =
            completion_exit_code(completion.success, completion.code, completion.signal)?;
        Ok(ExitCode::from(exit_code))
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
) -> Result<std::path::PathBuf> {
    anyhow::ensure!(!fixes.is_empty(), "refusing to spool empty hook fixes");
    let path = snapshot.scratch_root.join("fixes.patch");
    let mut file = process::create_private_new(&path).context("creating private fixes spool")?;
    file.write_all(fixes)
        .context("writing private fixes spool")?;
    file.sync_all().context("syncing private fixes spool")?;
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
