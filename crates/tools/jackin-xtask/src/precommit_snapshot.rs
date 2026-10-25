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

#[derive(Clone, Copy)]
enum OwnerSignal {
    Hup,
    Int,
    Term,
}

#[derive(Clone)]
struct SignalRoute {
    recovery_armed: Arc<AtomicBool>,
    setup_gate_cancellation: Arc<AtomicBool>,
    recovery_cancellation: Arc<AtomicBool>,
    hup: Arc<AtomicBool>,
    int: Arc<AtomicBool>,
    term: Arc<AtomicBool>,
}

impl SignalRoute {
    fn record(&self, signal: OwnerSignal) {
        let recovery_phase = self.recovery_armed.load(Ordering::SeqCst);
        if recovery_phase {
            self.recovery_cancellation.store(true, Ordering::SeqCst);
        } else {
            self.setup_gate_cancellation.store(true, Ordering::SeqCst);
        }
        match signal {
            OwnerSignal::Hup => self.hup.store(true, Ordering::SeqCst),
            OwnerSignal::Int => self.int.store(true, Ordering::SeqCst),
            OwnerSignal::Term => self.term.store(true, Ordering::SeqCst),
        }
    }
}

struct SetupSignals {
    route: SignalRoute,
}

#[expect(
    unsafe_code,
    reason = "signal callback touches only preallocated lock-free atomic flags"
)]
fn register_phase_routed_signal(
    signal: i32,
    route: SignalRoute,
    owner_signal: OwnerSignal,
) -> Result<()> {
    // SAFETY: signal-hook requires the callback to be async-signal-safe. It
    // reads and writes only preallocated lock-free atomic flags; it cannot
    // panic, allocate, lock, or manage a process.
    unsafe { signal_hook::low_level::register(signal, move || route.record(owner_signal)) }
        .context("registering native hook cancellation")?;
    Ok(())
}

impl SetupSignals {
    fn register() -> Result<Self> {
        let signals = Self {
            route: SignalRoute {
                recovery_armed: Arc::new(AtomicBool::new(false)),
                setup_gate_cancellation: Arc::new(AtomicBool::new(false)),
                recovery_cancellation: Arc::new(AtomicBool::new(false)),
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
            let route = signals.route.clone();
            register_phase_routed_signal(signal, route, owner_signal)?;
        }
        Ok(signals)
    }

    fn setup_gate_controls(&self, budget: Duration) -> process::SetupControls {
        process::SetupControls::new(Arc::clone(&self.route.setup_gate_cancellation), budget)
    }

    fn arm_recovery(&self) -> Result<process::SetupControls> {
        self.arm_recovery_with_budget(Duration::from_mins(10))
    }

    fn arm_recovery_with_budget(&self, budget: Duration) -> Result<process::SetupControls> {
        ensure!(
            self.route
                .recovery_armed
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok(),
            "native hook recovery phase was armed more than once"
        );
        Ok(process::SetupControls::new(
            Arc::clone(&self.route.recovery_cancellation),
            budget,
        ))
    }

    fn signal_status(&self) -> Option<u8> {
        if self.route.term.load(Ordering::Relaxed) {
            Some(143)
        } else if self.route.int.load(Ordering::Relaxed) {
            Some(130)
        } else if self.route.hup.load(Ordering::Relaxed) {
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
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, ensure};

pub(crate) fn run_installed_from_hook(
    generation: &std::path::Path,
    arguments: &[OsString],
) -> Result<ExitCode> {
    let signals = SetupSignals::register()?;
    if let Some(code) = signals.exit_code() {
        return Ok(code);
    }
    match run_installed_with_signals(generation, arguments, &signals) {
        Err(error) if process::is_clean_cancellation(&error) => {
            let _write_result = writeln!(
                std::io::stderr().lock(),
                "native hook recovery stopped after strict child drain; {}",
                format_args!("{error:#}")
            );
            if let Some(code) = signals.exit_code() {
                Ok(code)
            } else {
                Err(error)
            }
        }
        result => result,
    }
}

fn run_installed_with_signals(
    generation: &std::path::Path,
    arguments: &[OsString],
    signals: &SetupSignals,
) -> Result<ExitCode> {
    let setup_controls = signals.setup_gate_controls(Duration::from_mins(10));
    if let Some(code) = signals.exit_code() {
        return Ok(code);
    }
    let live = git::LiveRepository::capture(&setup_controls)?;
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
            setup_controls.check_start()?;
            return Ok(code);
        }
        let tree = snapshot.capture_staged_tree()?;
        if let Some(code) = signals.exit_code() {
            setup_controls.check_start()?;
            return Ok(code);
        }
        installation.check_pending_tree(&tree.to_string_lossy())?;
        snapshot.materialize(&tree, &live)?;
        if let Some(code) = signals.exit_code() {
            setup_controls.check_start()?;
            return Ok(code);
        }
        snapshot.admit(&tree)?;
        if let Some(code) = signals.exit_code() {
            setup_controls.check_start()?;
            return Ok(code);
        }
        let tools = toolchain::qualify(&snapshot)?;
        if let Some(code) = signals.exit_code() {
            setup_controls.check_start()?;
            return Ok(code);
        }
        installation.verify_tool_hashes(&tools.cargo, &tools.rustc)?;
        if let Some(code) = signals.exit_code() {
            setup_controls.check_start()?;
            return Ok(code);
        }
        let gate_controls = signals.setup_gate_controls(Duration::from_mins(30));
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
        let recovery_controls = signals.arm_recovery()?;
        git::OwnedSnapshot::verify_live_index(&live)?;
        let fixes = snapshot.collect_fixes(&tree, &recovery_controls)?;
        let evidence = if fixes.is_empty() {
            None
        } else {
            write_private_fixes_spool(&snapshot, &fixes)?;
            Some(evidence::persist(&live.common_git_dir, &fixes)?)
        };
        drop(completion.cleanup.recovery.take());
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
mod tests {
    use std::{
        ffi::OsStr,
        fs,
        path::{Path, PathBuf},
        process::Command,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    use super::{
        OwnerSignal, SetupSignals, SignalRoute, completion_exit_code, retain_snapshot_failure,
        retain_unsuccessful_completion, write_private_fixes_spool,
    };
    use crate::precommit_snapshot::{evidence, git, process, toolchain::QualifiedTools};

    fn fixture() -> (tempfile::TempDir, git::LiveRepository, git::OwnedSnapshot) {
        let home = fs::canonicalize(std::env::var_os("HOME").expect("HOME is set"))
            .expect("canonicalizing HOME");
        let repository = tempfile::Builder::new()
            .prefix("jackin-f2-retention-")
            .tempdir_in(home)
            .expect("creating fixture repository");
        git_command(repository.path(), &["init", "--quiet"]);
        git_command(
            repository.path(),
            &["config", "user.name", "Retention Test"],
        );
        git_command(
            repository.path(),
            &["config", "user.email", "retention@example.invalid"],
        );
        fs::write(repository.path().join("tracked.txt"), b"fixture\n")
            .expect("writing fixture source");
        git_command(repository.path(), &["add", "tracked.txt"]);
        git_command(repository.path(), &["commit", "--quiet", "-m", "fixture"]);

        let live = git::LiveRepository::capture_at(repository.path()).expect("capturing fixture");
        let snapshot = git::OwnedSnapshot::create(&live).expect("creating private snapshot");
        (repository, live, snapshot)
    }

    fn git_command(root: &Path, arguments: &[&str]) {
        let status = Command::new("git")
            .args(arguments)
            .current_dir(root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .status()
            .expect("running fixture Git command");
        assert!(status.success(), "git {arguments:?} failed: {status}");
    }

    fn seed_diagnostics(snapshot: &git::OwnedSnapshot) -> PathBuf {
        fs::write(
            snapshot.scratch_root.join("stdout.log"),
            b"original stdout\n",
        )
        .expect("writing stdout fixture");
        fs::write(
            snapshot.scratch_root.join("stderr.log"),
            b"original stderr\n",
        )
        .expect("writing stderr fixture");
        write_private_fixes_spool(snapshot, b"original fixes\n").expect("spooling fixes")
    }

    fn assert_retained_diagnostics(root: &Path, fixes: &[u8]) {
        assert_eq!(
            fs::read(root.join("stdout.log")).expect("retained stdout"),
            b"original stdout\n"
        );
        assert_eq!(
            fs::read(root.join("stderr.log")).expect("retained stderr"),
            b"original stderr\n"
        );
        assert_eq!(
            fs::read(root.join("fixes.patch")).expect("retained fixes"),
            fixes
        );
    }

    fn test_signals() -> SetupSignals {
        SetupSignals {
            route: SignalRoute {
                recovery_armed: Arc::new(AtomicBool::new(false)),
                setup_gate_cancellation: Arc::new(AtomicBool::new(false)),
                recovery_cancellation: Arc::new(AtomicBool::new(false)),
                hup: Arc::new(AtomicBool::new(false)),
                int: Arc::new(AtomicBool::new(false)),
                term: Arc::new(AtomicBool::new(false)),
            },
        }
    }

    #[test]
    fn signal_route_classifies_one_event_before_and_after_recovery_arm() {
        let signals = test_signals();

        signals.route.record(OwnerSignal::Hup);
        assert!(signals.route.setup_gate_cancellation.load(Ordering::SeqCst));
        assert!(!signals.route.recovery_cancellation.load(Ordering::SeqCst));

        let recovery = signals
            .arm_recovery_with_budget(std::time::Duration::from_secs(30))
            .expect("recovery phase arms once");
        assert!(!recovery.cancellation().load(Ordering::SeqCst));
        signals.route.record(OwnerSignal::Term);
        assert!(signals.route.setup_gate_cancellation.load(Ordering::SeqCst));
        assert!(recovery.cancellation().load(Ordering::SeqCst));
        assert_eq!(signals.signal_status(), Some(143));
    }

    #[test]
    fn an_initial_sticky_signal_does_not_mask_recovery_deadline_failure() {
        let signals = test_signals();
        signals.route.record(OwnerSignal::Term);
        let recovery = signals
            .arm_recovery_with_budget(std::time::Duration::ZERO)
            .expect("recovery phase arms");
        let error = recovery
            .check_start()
            .expect_err("expired recovery phase remains an error");
        assert_eq!(
            error
                .downcast_ref::<process::CaptureFailure>()
                .unwrap()
                .kind,
            process::CaptureFailureKind::Deadline
        );
        assert_eq!(signals.signal_status(), Some(143));
        assert!(!process::is_clean_cancellation(&error));
    }

    #[test]
    fn completion_exit_code_preserves_strict_outcome_precedence() {
        assert_eq!(completion_exit_code(false, Some(0), None).unwrap(), 1);
        assert_eq!(completion_exit_code(true, Some(0), None).unwrap(), 0);
        assert_eq!(completion_exit_code(false, Some(23), None).unwrap(), 23);
        assert_eq!(completion_exit_code(false, Some(23), Some(15)).unwrap(), 23);
        assert_eq!(completion_exit_code(false, None, Some(15)).unwrap(), 143);
        assert_eq!(
            completion_exit_code(false, Some(256), Some(15)).unwrap(),
            143
        );
    }

    #[test]
    fn completion_exit_code_errors_without_valid_code_or_signal() {
        assert_eq!(
            completion_exit_code(false, None, None)
                .unwrap_err()
                .to_string(),
            "native pre-commit gate did not complete successfully"
        );
    }

    #[test]
    fn live_index_error_retains_original_diagnostics_after_owner_drop() {
        let (_repository, live, mut snapshot) = fixture();
        let root = snapshot.scratch_root.clone();
        let fixes = b"original fixes\n";
        seed_diagnostics(&snapshot);
        fs::write(&live.index, b"changed index").expect("changing fixture index");

        let error = git::OwnedSnapshot::verify_live_index(&live).unwrap_err();
        let error = retain_snapshot_failure(&mut snapshot, error);
        let detail = format!("{error:#}");
        assert!(detail.contains("live Git index or split-index backing file changed"));
        assert!(detail.contains("private snapshot retained at"));
        drop(snapshot);

        assert_retained_diagnostics(&root, fixes);
        fs::remove_dir_all(root).expect("removing retained index-error snapshot");
    }

    #[test]
    fn fixes_collection_error_retains_new_paths_and_diagnostics_after_owner_drop() {
        let (_repository, _live, mut snapshot) = fixture();
        let root = snapshot.scratch_root.clone();
        let fixes = b"original fixes\n";
        seed_diagnostics(&snapshot);
        let error = snapshot
            .collect_fixes(OsStr::new("not-a-tree-object"), &snapshot.controls)
            .unwrap_err();
        let error = retain_snapshot_failure(&mut snapshot, error);
        assert!(format!("{error:#}").contains("private snapshot retained at"));
        drop(snapshot);

        assert!(root.join("new-paths").is_file());
        assert_retained_diagnostics(&root, fixes);
        fs::remove_dir_all(root).expect("removing retained fixes-error snapshot");
    }

    #[test]
    fn evidence_persist_error_retains_fixes_before_owner_drop() {
        let (_repository, live, mut snapshot) = fixture();
        let root = snapshot.scratch_root.clone();
        let fixes = b"diff --git a/tracked.txt b/tracked.txt\n";
        let spool = write_private_fixes_spool(&snapshot, fixes).expect("spooling collected fixes");
        let collision = live.common_git_dir.join("jackin-hook-evidence");
        fs::write(collision, b"not a directory").expect("creating evidence-path collision");

        let error = evidence::persist(&live.common_git_dir, fixes).unwrap_err();
        let error = retain_snapshot_failure(&mut snapshot, error);
        let detail = format!("{error:#}");
        assert!(detail.contains("hook evidence root is not a physical directory"));
        assert!(detail.contains("private snapshot retained at"));
        drop(snapshot);

        assert_eq!(fs::read(spool).expect("retained fixes spool"), fixes);
        fs::remove_dir_all(root).expect("removing retained evidence-error snapshot");
    }

    #[test]
    fn gate_spawn_error_without_recovery_retains_snapshot_after_owner_drop() {
        let (repository, _live, mut snapshot) = fixture();
        let root = snapshot.scratch_root.clone();
        let fixes = b"original fixes\n";
        let spool = write_private_fixes_spool(&snapshot, fixes).expect("spooling fixes");
        let tools = QualifiedTools {
            gate: repository.path().join("missing-gate"),
            cargo: PathBuf::new(),
            rustc: PathBuf::new(),
            environment: std::collections::BTreeMap::new(),
        };

        let controls = snapshot.controls.clone();
        let Err(error) = snapshot.run_gate(&tools, &[], &controls) else {
            panic!("missing gate unexpectedly completed");
        };
        let original = format!("{error:#}");
        assert!(!original.is_empty());
        let error = retain_snapshot_failure(&mut snapshot, error);
        let detail = format!("{error:#}");
        assert!(detail.contains(&original));
        assert!(detail.contains("private snapshot retained at"));
        drop(snapshot);

        assert!(root.join("stdout.log").is_file());
        assert!(root.join("stderr.log").is_file());
        assert_eq!(fs::read(spool).expect("retained fixes spool"), fixes);
        fs::remove_dir_all(root).expect("removing retained gate-error snapshot");
    }

    #[test]
    fn unsuccessful_completion_retains_spools_after_owner_drop() {
        let (_repository, live, mut snapshot) = fixture();
        let root = snapshot.scratch_root.clone();
        let fixes = b"original fixes\n";
        seed_diagnostics(&snapshot);

        let retained = retain_unsuccessful_completion(&mut snapshot, false)
            .expect("retaining unsuccessful completion");
        assert_eq!(retained, root);
        drop(snapshot);

        assert_retained_diagnostics(&root, fixes);
        fs::remove_dir_all(root).expect("removing unsuccessful-completion snapshot");

        let mut successful_snapshot = git::OwnedSnapshot::create(&live).expect("new snapshot");
        let successful_root = successful_snapshot.scratch_root.clone();
        assert!(retain_unsuccessful_completion(&mut successful_snapshot, true).is_none());
        drop(successful_snapshot);
        assert!(!successful_root.exists());
    }
}
