// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod a1;
mod args;
mod clock;
mod engine;
mod host;
mod ledger;
mod probe;
mod production;
mod pty;
mod redact;
mod report;
mod run;
mod scratch;
mod setup;
mod steps;
mod sweep;
mod verdict;
mod world;

use std::collections::BTreeMap;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use jackin_openshell::Gateway;
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;

pub use a1::{A1_LOCK_FILE, A1Error, A1Lock, A1Login, fixture_login, host_login_state};
pub use args::E2eArgs;
pub use clock::{Budgets, Clock, SystemClock};
pub use engine::{HostEngine, SocketEngine};
pub use host::{Exit, HostCommand, HostError, HostOutput, HostProcess, RealHost};
pub use probe::{
    ProbeCheck, ProbeError, ProbeMode, ProbeReport, ProbeRequest, ProbeResult, ProbeStep,
    SCHEMA_TEXT, schema_step_drift,
};
pub use production::{ProductionError, production};
pub use pty::{PortablePty, PtyError, PtyHost, PtyLink, PtySession, PtySpec};
pub use redact::Redactor;
pub use report::{Capture, Sink};
pub use scratch::{Scratch, ScratchError, base_env, child_env, snapshot_tree};
pub use verdict::{ExitStatus, LineId, STEP_IDS, Status, StepId, StepLine, Verdict, render};

use report::{OrderedPrinter, Printer};
use steps::{Outcome, PLAN, Phase};
use sweep::{SweepError, sweep};

#[derive(Clone, Debug)]
pub struct Boundaries {
    pub host: Arc<dyn HostProcess>,
    pub pty: Arc<dyn PtyHost>,
    pub gateway: Arc<dyn Gateway>,
    pub engine: Arc<dyn HostEngine>,
    pub clock: Arc<dyn Clock>,
    pub budgets: Budgets,
    pub env: BTreeMap<String, String>,
    pub jackin: PathBuf,
    pub repo_root: PathBuf,
    pub codex_home: PathBuf,
    pub scratch_root: PathBuf,
    pub log_dir: PathBuf,
    pub lock_dir: PathBuf,
    pub tag: String,
    pub cancel: CancellationToken,
    pub stdout: Sink,
    pub stderr: Sink,
}

struct Signals {
    task: tokio::task::JoinHandle<()>,
}

impl Signals {
    fn start(cancel: CancellationToken, printer: Printer) -> Option<Signals> {
        let mut interrupt = signal(SignalKind::interrupt()).ok()?;
        let mut terminate = signal(SignalKind::terminate()).ok()?;
        let task = tokio::spawn(async move {
            let mut seen = 0_u32;
            loop {
                tokio::select! {
                    _ = interrupt.recv() => {}
                    _ = terminate.recv() => {}
                }
                if seen == 0 {
                    cancel.cancel();
                } else {
                    printer.progress("cleanup in progress");
                }
                seen += 1;
            }
        });
        Some(Signals { task })
    }
}

impl Drop for Signals {
    fn drop(&mut self) {
        self.task.abort();
    }
}

type PanicHook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>;

struct QuietPanics {
    previous: Option<PanicHook>,
}

impl QuietPanics {
    fn install() -> QuietPanics {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        QuietPanics {
            previous: Some(previous),
        }
    }
}

impl Drop for QuietPanics {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            std::panic::set_hook(previous);
        }
    }
}

pub async fn run_e2e(args: &E2eArgs, boundaries: Boundaries) -> ExitStatus {
    let printer = Printer::new(boundaries.stdout.clone(), boundaries.stderr.clone());
    let _quiet = QuietPanics::install();
    let _signals = Signals::start(boundaries.cancel.clone(), printer.clone());
    execute(args, &boundaries, &printer).await
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned())
}

async fn guarded<T>(
    work: impl Future<Output = T>,
    cap: Duration,
    on_failure: impl Fn(String) -> T,
) -> (T, bool) {
    match tokio::time::timeout(cap, AssertUnwindSafe(work).catch_unwind()).await {
        Ok(Ok(done)) => (done, false),
        Ok(Err(payload)) => (
            on_failure(format!(
                "the step panicked: {}",
                panic_text(payload.as_ref())
            )),
            false,
        ),
        Err(_) => (
            on_failure(format!("timed out after {}s", cap.as_secs_f64())),
            true,
        ),
    }
}

async fn execute(args: &E2eArgs, b: &Boundaries, printer: &Printer) -> ExitStatus {
    if args.sweep {
        return run_sweep(args, b, printer).await;
    }
    let mut run = match setup::prepare(args, b, printer).await {
        Ok(run) => run,
        Err(error) => {
            printer.progress(&format!("xtask e2e: {}", error.0));
            return ExitStatus::Setup;
        }
    };
    if let Err(error) = run.configure().await {
        printer.progress(&format!("xtask e2e: {}", error.0));
        drop(run.cleanup().await);
        return ExitStatus::Setup;
    }
    let started = b.clock.now();
    let mut ordered = OrderedPrinter::new(printer.clone());
    drive(&mut run, &mut ordered, started).await;
    let interrupted = b.cancel.is_cancelled();
    ordered.skip_missing(if interrupted {
        "interrupted"
    } else {
        "not run"
    });
    let clean_started = b.clock.now();
    let cleanup = tokio::time::timeout(b.budgets.cleanup_total, run.cleanup()).await;
    let verdict = cleanup.unwrap_or_else(|_| Verdict::fail("cleanup exceeded its total budget"));
    printer.progress(&format!("CLEAN {}", verdict.status().word()));
    ordered.submit(StepLine {
        id: LineId::Clean,
        verdict,
        elapsed: b.clock.now().saturating_sub(clean_started),
    });
    let login = host_login_state(&b.codex_home, &run.a1);
    ordered.result(b.clock.now().saturating_sub(started), login);
    ExitStatus::of(&ordered.lines(), interrupted)
}

async fn drive(run: &mut run::Run<'_>, ordered: &mut OrderedPrinter, started: Duration) {
    let b = run.b;
    for phase in PLAN {
        if b.cancel.is_cancelled() || b.clock.now().saturating_sub(started) > b.budgets.global {
            break;
        }
        match phase {
            Phase::Act(id) => {
                if let Some(by) = run.blocked_by(id) {
                    record(
                        run,
                        ordered,
                        id,
                        Verdict::skip(format!("blocked by {by} FAIL")),
                        Duration::ZERO,
                    );
                    continue;
                }
                let begun = b.clock.now();
                let cap = b.budgets.step(id);
                let (outcome, timed_out) =
                    guarded(run.act(id), cap, |text| Outcome::Done(Verdict::fail(text))).await;
                let elapsed = if timed_out {
                    cap
                } else {
                    b.clock.now().saturating_sub(begun)
                };
                if b.cancel.is_cancelled() {
                    break;
                }
                match outcome {
                    Outcome::Done(verdict) => record(run, ordered, id, verdict, elapsed),
                    Outcome::Held(verdict) => {
                        if timed_out {
                            record(run, ordered, id, verdict, elapsed);
                        } else {
                            run.w.held.insert(id, (verdict, elapsed));
                        }
                    }
                }
            }
            Phase::Settle(id) => {
                let Some((held, spent)) = run.w.held.remove(&id) else {
                    continue;
                };
                let begun = b.clock.now();
                let (verdict, _) =
                    guarded(run.settle(id, held), b.budgets.step(id), Verdict::fail).await;
                if b.cancel.is_cancelled() {
                    break;
                }
                let total = spent + b.clock.now().saturating_sub(begun);
                record(run, ordered, id, verdict, total);
            }
        }
    }
}

fn record(
    run: &mut run::Run<'_>,
    ordered: &mut OrderedPrinter,
    id: StepId,
    verdict: Verdict,
    elapsed: Duration,
) {
    run.printer.progress(&format!(
        "{id} {} {:.1}s",
        verdict.status().word(),
        elapsed.as_secs_f64()
    ));
    run.w.verdicts.insert(id, verdict.clone());
    ordered.submit(StepLine {
        id: LineId::Step(id),
        verdict,
        elapsed,
    });
}

async fn run_sweep(args: &E2eArgs, b: &Boundaries, printer: &Printer) -> ExitStatus {
    match sweep(args, b, &base_env(&b.env)).await {
        Ok(done) => {
            printer.out(&format!("SWEEP PASS {done}"));
            ExitStatus::Pass
        }
        Err(SweepError::Failed(error)) => {
            printer.out(&format!("SWEEP FAIL {error}"));
            ExitStatus::Failed
        }
        Err(SweepError::Unavailable(error)) => {
            printer.progress(&format!("xtask e2e: {error}"));
            ExitStatus::Setup
        }
    }
}
