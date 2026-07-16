// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Bounded in-memory invocation progress and timing state.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
#[cfg(test)]
use std::sync::Weak;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use rand::RngExt as _;

use jackin_core::JackinPaths;

#[cfg(test)]
mod tests;

const CRASH_EVIDENCE_EXPORT_CAP: usize = 4096;
const MAX_HISTOGRAM_SAMPLES: usize = 1024;

static ACTIVE_RUN: OnceLock<Mutex<Option<Arc<RunDiagnostics>>>> = OnceLock::new();
static HOST_PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();
#[cfg(test)]
static ACTIVE_RUN_BY_DIR: OnceLock<Mutex<HashMap<PathBuf, Weak<RunDiagnostics>>>> = OnceLock::new();

fn active_slot() -> &'static Mutex<Option<Arc<RunDiagnostics>>> {
    ACTIVE_RUN.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
fn active_run_by_dir() -> &'static Mutex<HashMap<PathBuf, Weak<RunDiagnostics>>> {
    ACTIVE_RUN_BY_DIR.get_or_init(|| Mutex::new(HashMap::new()))
}

fn locked<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Debug)]
pub struct RunDiagnostics {
    run_id: String,
    scope: PathBuf,
    debug: bool,
    /// Per-stage start timestamps for wall-clock timing (Defect 47.5).
    stage_starts: Mutex<HashMap<String, Instant>>,
    /// Per-stage tracing spans so all progress events for a launch stage share
    /// a stable span id in the JSONL.
    stage_spans: Mutex<HashMap<crate::DiagnosticStage, tracing::Span>>,
    /// Fine-grained timing starts nested under broad launch stages.
    timing_starts: Mutex<HashMap<String, Instant>>,
    /// Accumulated per-stage durations for the end-of-run summary.
    stage_durations_ms: Mutex<Vec<(String, u64)>>,
    metrics: Mutex<DiagnosticsMetrics>,
}

#[derive(Debug)]
pub struct ActiveRunGuard {
    previous: Option<Arc<RunDiagnostics>>,
    #[cfg(test)]
    active_dir: Option<PathBuf>,
}

impl Drop for ActiveRunGuard {
    fn drop(&mut self) {
        let mut guard = active_slot()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = self.previous.take();
        drop(guard);
        #[cfg(test)]
        if let Some(active_dir) = self.active_dir.take() {
            locked(active_run_by_dir()).remove(&active_dir);
        }
        // Flush OTLP batches here, not at the call sites in `app::run`: the
        // guard is a run-scoped local, so this fires on every exit path —
        // including `?` error early-returns — instead of only the two
        // success returns. No-op unless OTLP export was configured.
        crate::observability::shutdown_otlp();
    }
}

#[derive(Clone, Debug, Default)]
struct DiagnosticsMetrics {
    event_counts: BTreeMap<String, u64>,
    stage_duration_ms: BTreeMap<String, Vec<u64>>,
    timing_duration_ms: BTreeMap<String, Vec<u64>>,
    stage_duration_dropped: BTreeMap<String, u64>,
    timing_duration_dropped: BTreeMap<String, u64>,
    cache_hits: u64,
    cache_misses: u64,
}

impl DiagnosticsMetrics {
    fn push_stage_duration(&mut self, key: String, value: u64) {
        push_capped_sample(
            &mut self.stage_duration_ms,
            &mut self.stage_duration_dropped,
            key,
            value,
        );
    }

    fn push_timing_duration(&mut self, key: String, value: u64) {
        push_capped_sample(
            &mut self.timing_duration_ms,
            &mut self.timing_duration_dropped,
            key,
            value,
        );
    }
}

fn push_capped_sample(
    histograms: &mut BTreeMap<String, Vec<u64>>,
    dropped: &mut BTreeMap<String, u64>,
    key: String,
    value: u64,
) {
    let samples = histograms.entry(key.clone()).or_default();
    if samples.len() < MAX_HISTOGRAM_SAMPLES {
        samples.push(value);
    } else {
        *dropped.entry(key).or_default() += 1;
    }
}

fn display_unclosed_key(key: &str) -> String {
    key.replace('\0', "/")
}

impl RunDiagnostics {
    pub fn start(paths: &JackinPaths, debug: bool, command: &str) -> anyhow::Result<Arc<Self>> {
        // Mint before subscriber init: the OTLP resource carries the run id.
        let run_id = jackin_telemetry::identity::current_invocation().map_or_else(
            || jackin_telemetry::identity::InvocationId::mint().to_string(),
            |id| id.to_string(),
        );
        // Install direct OTLP export when configured.
        let identity = match command {
            "console" => crate::observability::ServiceIdentity::HOST_INTERACTIVE,
            "daemon" => crate::observability::ServiceIdentity::DAEMON,
            "role" => crate::observability::ServiceIdentity::ROLE,
            _ => crate::observability::ServiceIdentity::HOST_ONE_SHOT,
        };
        let (otlp_active, otlp_error) =
            match crate::observability::init_tracing_for(debug, &run_id, identity) {
                Ok(active) => (active, None),
                // "already installed" is benign (a test harness set its own
                // subscriber); treat as inactive with no operator-facing error.
                Err(error) if error.to_string().contains("already installed") => (false, None),
                // A configured endpoint whose exporter fails / unsupported protocol
                // is a real loss of telemetry the operator asked for: fall back to
                // the file and surface one compact breadcrumb.
                Err(error) => (false, Some(error.to_string())),
            };
        // Export not installed, no build error, yet endpoint vars ARE set: the
        // config is incomplete (e.g. a metrics endpoint with no traces/logs base,
        // which can't satisfy the mandatory traces+logs signals). Surface it as a
        // breadcrumb rather than silently writing the file as if export was never
        // requested — the exact silent-no-deliver this observability work closes.
        let otlp_error = otlp_error.or_else(|| {
            (!otlp_active && crate::observability::otlp_endpoint_configured()).then(|| {
                "OTLP endpoint configured but incomplete (traces and logs endpoints required)"
                    .to_owned()
            })
        });
        let run = Arc::new(Self {
            run_id,
            scope: paths.data_dir.clone(),
            debug,
            stage_starts: Mutex::new(HashMap::new()),
            stage_spans: Mutex::new(HashMap::new()),
            timing_starts: Mutex::new(HashMap::new()),
            stage_durations_ms: Mutex::new(Vec::new()),
            metrics: Mutex::new(DiagnosticsMetrics::default()),
        });
        if let Some(error) = otlp_error {
            // Record into the run file (on by construction here, since OTLP is
            // inactive) and emit the compact operator notice (stderr / deferred
            // under a rich TUI). Visibility never depends on the file alone.
            let line = format!("OTLP export disabled: {error}");
            run.compact("otlp", &line);
            crate::logging::emit_compact_line("otlp", &line);
        }
        crate::observability::emit_progress_event(
            &run.run_id,
            "run",
            &format!("command {command} started"),
            None,
            None,
        );
        Ok(run)
    }

    pub fn activate(self: &Arc<Self>) -> ActiveRunGuard {
        #[cfg(test)]
        let active_dir = Some(self.scope.clone());
        #[cfg(test)]
        if let Some(dir) = &active_dir {
            locked(active_run_by_dir()).insert(dir.clone(), Arc::downgrade(self));
        }
        let previous = active_slot()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .replace(Arc::clone(self));
        ActiveRunGuard {
            previous,
            #[cfg(test)]
            active_dir,
        }
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn compact(&self, kind: &str, message: &str) {
        crate::observability::emit_progress_event(&self.run_id, kind, message, None, None);
    }

    pub fn error(&self, kind: &str, message: &str) {
        crate::observability::emit_progress_error(&self.run_id, kind, message, None, None);
    }

    pub fn error_typed(&self, kind: &str, message: &str, error_type: Option<&str>) {
        if let Some(error_type) = error_type {
            crate::metrics::incr_errors(error_type);
        }
        crate::observability::emit_progress_error_typed(
            &self.run_id,
            kind,
            message,
            None,
            None,
            error_type,
        );
    }

    pub fn stage(
        &self,
        kind: &str,
        stage: crate::DiagnosticStage,
        message: &str,
        detail: Option<&str>,
    ) {
        let stage_label = stage.as_str();
        // Track wall-clock stage timings for the end-of-run summary (Defect 47.5).
        // A stage runs on the operator's foreground launch path, so a slow one is a
        // foreground wait the operator is paying for; capture its duration to
        // explain any wait over the threshold (acceptance: explain every
        // foreground wait over 500ms).
        let mut foreground_wait_ms: Option<u64> = None;
        let enriched_detail = match kind {
            "stage_started" => {
                locked(&self.stage_starts).insert(stage_label.to_owned(), Instant::now());
                locked(&self.stage_spans).insert(stage, launch_stage_span(stage));
                detail.map(String::from)
            }
            "stage_done" => {
                let elapsed_ms = locked(&self.stage_starts)
                    .remove(stage_label)
                    .map(|t| t.elapsed().as_millis() as u64);
                elapsed_ms.map_or_else(
                    || detail.map(String::from),
                    |ms| {
                        foreground_wait_ms = Some(ms);
                        locked(&self.stage_durations_ms).push((stage_label.to_owned(), ms));
                        locked(&self.metrics).push_stage_duration(stage_label.to_owned(), ms);
                        let base = detail.unwrap_or("");
                        if base.is_empty() {
                            Some(format!("{{\"duration_ms\":{ms}}}"))
                        } else {
                            Some(format!("{{\"duration_ms\":{ms},\"detail\":{base:?}}}"))
                        }
                    },
                )
            }
            "stage_failed" | "stage_skipped" => {
                let _ = locked(&self.stage_starts).remove(stage_label);
                detail.map(String::from)
            }
            _ => detail.map(String::from),
        };
        let span = self.stage_span_for(kind, stage);
        if kind == "stage_failed" {
            span.record("otel.status_code", "ERROR");
            span.record("otel.status_description", message);
        }
        let _entered = span.enter();
        if kind.ends_with("_failed") {
            crate::observability::emit_progress_error_typed(
                &self.run_id,
                kind,
                message,
                Some(stage_label),
                enriched_detail.as_deref(),
                None,
            );
        } else {
            crate::observability::emit_progress_event(
                &self.run_id,
                kind,
                message,
                Some(stage_label),
                enriched_detail.as_deref(),
            );
        }
        if let Some(ms) = foreground_wait_ms {
            self.explain_foreground_wait(stage_label, ms);
        }
    }

    /// Threshold above which a foreground wait is explicitly explained in the
    /// diagnostics stream, not merely timed. The operator is blocked at the
    /// terminal for the whole foreground launch path, so any single stage that
    /// holds them longer than this gets a typed `slow_foreground_wait` event
    /// naming the stage and its cost (acceptance: explain every foreground wait
    /// over 500ms).
    pub const FOREGROUND_WAIT_EXPLAIN_THRESHOLD_MS: u64 = 500;

    /// Emit a `slow_foreground_wait` diagnostic when a foreground stage/timing
    /// exceeds [`Self::FOREGROUND_WAIT_EXPLAIN_THRESHOLD_MS`]. No-op below the
    /// threshold so the stream stays quiet for the fast path.
    fn explain_foreground_wait(&self, label: &str, ms: u64) {
        let Some(wait) =
            slow_foreground_wait_payload(label, ms, Self::FOREGROUND_WAIT_EXPLAIN_THRESHOLD_MS)
        else {
            return;
        };
        crate::observability::emit_progress_event(
            &self.run_id,
            "slow_foreground_wait",
            &wait.message,
            Some(label),
            Some(&wait.detail),
        );
    }

    pub fn timing_started(&self, stage: crate::DiagnosticStage, name: &str, detail: Option<&str>) {
        let stage_label = stage.as_str();
        let key = timing_key(stage_label, name);
        locked(&self.timing_starts).insert(key, Instant::now());
        let event_detail = timing_detail(name, None, detail);
        let span = self.current_stage_span(stage);
        let _entered = span.as_ref().map(tracing::Span::enter);
        crate::observability::emit_progress_event(
            &self.run_id,
            "timing_started",
            &format!("{name} started"),
            Some(stage_label),
            Some(&event_detail),
        );
    }

    pub fn timing_done(&self, stage: crate::DiagnosticStage, name: &str, detail: Option<&str>) {
        let stage_label = stage.as_str();
        let key = timing_key(stage_label, name);
        let elapsed_ms = locked(&self.timing_starts)
            .remove(&key)
            .map(|start| start.elapsed().as_millis() as u64);
        if let Some(ms) = elapsed_ms {
            locked(&self.metrics).push_timing_duration(format!("{stage_label}/{name}"), ms);
        }
        let event_detail = timing_detail(name, elapsed_ms, detail);
        let span = self.current_stage_span(stage);
        let _entered = span.as_ref().map(tracing::Span::enter);
        crate::observability::emit_progress_event(
            &self.run_id,
            "timing_done",
            &format!("{name} done"),
            Some(stage_label),
            Some(&event_detail),
        );
        if let Some(ms) = elapsed_ms {
            self.explain_foreground_wait(&key, ms);
        }
    }

    fn current_stage_span(&self, stage: crate::DiagnosticStage) -> Option<tracing::Span> {
        locked(&self.stage_spans).get(&stage).cloned()
    }

    fn stage_span_for(&self, kind: &str, stage: crate::DiagnosticStage) -> tracing::Span {
        let mut spans = locked(&self.stage_spans);
        if matches!(kind, "stage_done" | "stage_failed" | "stage_skipped") {
            spans
                .remove(&stage)
                .unwrap_or_else(|| launch_stage_span(stage))
        } else {
            spans
                .entry(stage)
                .or_insert_with(|| launch_stage_span(stage))
                .clone()
        }
    }

    /// Emit a summary event at the end of the run with per-stage wall-clock durations.
    pub fn emit_run_summary(&self) {
        let durations_snapshot: Vec<(String, u64)> = {
            let durs = locked(&self.stage_durations_ms);
            durs.clone()
        };
        let stage_durations: serde_json::Value = durations_snapshot
            .iter()
            .map(|(s, ms)| (s.clone(), serde_json::Value::from(*ms)))
            .collect::<serde_json::Map<_, _>>()
            .into();
        let metrics = locked(&self.metrics).clone();
        let unclosed = self.drain_unclosed_keys();
        let summary = serde_json::json!({
            "stage_durations_ms": stage_durations,
            "stage_duration_histograms_ms": metrics.stage_duration_ms,
            "timing_duration_histograms_ms": metrics.timing_duration_ms,
            "stage_duration_dropped": metrics.stage_duration_dropped,
            "timing_duration_dropped": metrics.timing_duration_dropped,
            "event_counts": metrics.event_counts,
            "cache_hits": metrics.cache_hits,
            "cache_misses": metrics.cache_misses,
        })
        .to_string();
        crate::observability::emit_progress_event(
            &self.run_id,
            "run_summary",
            "stage durations and counters",
            None,
            Some(&summary),
        );
        if !unclosed.is_empty() {
            crate::observability::emit_progress_event(
                &self.run_id,
                "diagnostics",
                &format!("unclosed: {}", unclosed.join(", ")),
                None,
                None,
            );
        }
    }

    fn drain_unclosed_keys(&self) -> Vec<String> {
        let mut unclosed = BTreeSet::new();
        {
            let mut starts = locked(&self.stage_starts);
            unclosed.extend(
                starts
                    .keys()
                    .map(|key| format!("stage:{}", display_unclosed_key(key))),
            );
            starts.clear();
        }
        {
            let mut spans = locked(&self.stage_spans);
            unclosed.extend(
                spans
                    .keys()
                    .map(|key| format!("span:{}", display_unclosed_key(key.as_str()))),
            );
            spans.clear();
        }
        {
            let mut starts = locked(&self.timing_starts);
            unclosed.extend(
                starts
                    .keys()
                    .map(|key| format!("timing:{}", display_unclosed_key(key))),
            );
            starts.clear();
        }
        unclosed.into_iter().collect()
    }

    pub fn debug(&self, category: &str, line: &str) -> bool {
        if !crate::logging::debug_capture_enabled(category, self.debug) {
            return false;
        }
        crate::observability::emit_progress_event(
            &self.run_id,
            "debug",
            line,
            None,
            Some(category),
        );
        true
    }

    /// Emit a structured `container_started` event.
    ///
    /// Call this immediately after the `docker run -d` succeeds. Records the
    /// container name and the host path of the capsule diagnostics log so an
    /// agent reading the run JSONL can follow the pointer without knowing the
    /// on-disk layout.
    pub fn container_started(&self, container_name: &str, capsule_log_path: &str) {
        let detail = serde_json::json!({
            "container_name": container_name,
            "capsule_log": capsule_log_path,
        })
        .to_string();
        crate::observability::emit_progress_event(
            &self.run_id,
            "container_started",
            &format!("container {container_name} started"),
            Some(container_name),
            Some(&detail),
        );
    }

    /// Emit a structured `container_exited` or `container_crash` event.
    ///
    /// Call this when the container exits non-normally (pre-attach crash,
    /// OOM kill, or non-zero post-attach exit). For clean `exit 0` post-attach
    /// shutdowns, no event is needed.
    ///
    /// `crash_evidence` is the last N lines of `docker logs` or the
    /// Capsule process-log tail — passed in by the caller which already fetched
    /// it for the user-facing error message. When `crash_evidence` is `Some`,
    /// an additional `container_crash_log` event is written so the full cause
    /// is self-contained in the run JSONL.
    pub fn container_exited(
        &self,
        container_name: &str,
        exit_code: i64,
        oom_killed: bool,
        capsule_log_path: &str,
        crash_evidence: Option<&str>,
    ) {
        let detail = serde_json::json!({
            "container_name": container_name,
            "exit_code": exit_code,
            "oom_killed": oom_killed,
            "capsule_log": capsule_log_path,
        })
        .to_string();
        let kind = if exit_code != 0 || oom_killed {
            "container_crash"
        } else {
            "container_exited"
        };
        let msg = if oom_killed {
            format!("container {container_name} OOM killed")
        } else {
            format!("container {container_name} exited (exit {exit_code})")
        };
        if kind == "container_crash" {
            crate::observability::emit_progress_error(
                &self.run_id,
                kind,
                &msg,
                Some(container_name),
                Some(&detail),
            );
        } else {
            crate::observability::emit_progress_event(
                &self.run_id,
                kind,
                &msg,
                Some(container_name),
                Some(&detail),
            );
        }
        if let Some(evidence) = crash_evidence.filter(|s| !s.is_empty()) {
            let capped_evidence = cap_crash_evidence_for_export(evidence);
            crate::observability::emit_progress_error(
                &self.run_id,
                "container_crash_log",
                &format!("container {container_name} crash evidence"),
                Some(container_name),
                Some(&capped_evidence),
            );
        }
    }

    pub fn docker_build_step(
        &self,
        step: &str,
        label: &str,
        duration_ms: Option<u64>,
        cached: bool,
    ) {
        let detail = serde_json::json!({
            "step": step,
            "label": label,
            "duration_ms": duration_ms,
            "cached": cached,
        })
        .to_string();
        crate::observability::emit_progress_event(
            &self.run_id,
            "docker_build_step",
            &format!("docker build step {step} {label}"),
            Some("derived image"),
            Some(&detail),
        );
    }

    pub fn subprocess_done(&self, program: &str, elapsed_ms: u64, exit_code: Option<i32>) {
        let detail = serde_json::json!({
            "program": program,
            "elapsed_ms": elapsed_ms,
            "exit_code": exit_code,
        })
        .to_string();
        crate::observability::emit_progress_event(
            &self.run_id,
            "subprocess_done",
            "subprocess exited",
            Some(program),
            Some(&detail),
        );
    }
}

pub fn active_debug(category: &str, line: &str) -> bool {
    active_run().is_some_and(|run| run.debug(category, line))
}

pub fn active_timing_started(stage: crate::DiagnosticStage, name: &str, detail: Option<&str>) {
    if let Some(run) = active_run() {
        run.timing_started(stage, name, detail);
    }
}

pub fn active_timing_done(stage: crate::DiagnosticStage, name: &str, detail: Option<&str>) {
    if let Some(run) = active_run() {
        run.timing_done(stage, name, detail);
    }
}

pub fn active_subprocess_done(program: &str, elapsed_ms: u64, exit_code: Option<i32>) {
    if let Some(run) = active_run() {
        run.subprocess_done(program, elapsed_ms, exit_code);
    }
}

pub fn active_run() -> Option<Arc<RunDiagnostics>> {
    active_slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

pub fn active_run_for_paths(paths: &JackinPaths) -> Option<Arc<RunDiagnostics>> {
    #[cfg(test)]
    {
        return locked(active_run_by_dir())
            .get(&paths.data_dir)
            .and_then(Weak::upgrade);
    }

    #[cfg(not(test))]
    {
        let run = active_run()?;
        (run.scope == paths.data_dir).then_some(run)
    }
}

pub fn install_host_panic_hook() {
    let () = HOST_PANIC_HOOK_INSTALLED.get_or_init(|| {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Some(run) = active_run() {
                run.error_typed("panic", &format!("PANIC: {info}"), Some("panic"));
            }
            crate::observability::shutdown_otlp();
            default_hook(info);
        }));
    });
}

/// A fresh session id for the capsule's `session.id`. One daemon run is one
/// session; the id groups all of its OTLP telemetry into a single timeline.
#[must_use]
pub fn mint_session_id() -> String {
    let mut rng = rand::rng();
    let n: u32 = rng.random();
    format!("jk-session-{:06x}", n & 0x00ff_ffff)
}

fn timing_key(stage: &str, name: &str) -> String {
    format!("{stage}\0{name}")
}

fn launch_stage_span(stage: crate::DiagnosticStage) -> tracing::Span {
    // Registered span names only (plan 007) — free-form stage strings cannot
    // invent unbounded `launch.{token}` names.
    let otel_name = crate::registry::launch_stage_span_name(stage);
    let span = tracing::info_span!(
        "launch_stage",
        "launch.stage.name" = stage.as_str(),
        otel.name = otel_name,
        otel.status_code = tracing::field::Empty,
        otel.status_description = tracing::field::Empty,
    );
    {
        use tracing_opentelemetry::OpenTelemetrySpanExt as _;
        // Derived-image build is a peer subsystem of launch: link it to the active
        // launch span so the BuildKit trace is not a peer without a parent (plan 044).
        if stage == crate::DiagnosticStage::DerivedImage {
            use opentelemetry::trace::TraceContextExt as _;
            let parent_ctx = tracing::Span::current().context();
            let span_ctx = parent_ctx.span().span_context().clone();
            if span_ctx.is_valid() {
                span.add_link(span_ctx);
            }
        }
    }
    span
}

/// Normalize a free-form stage label for offline tooling / tests.
/// Launch span names use [`crate::registry::launch_stage_span_name`] instead.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "offline/tooling helper retained for tests")
)]
pub(crate) fn normalize_stage_name(stage: &str) -> String {
    let mut normalized = String::with_capacity(stage.len());
    let mut last_was_separator = false;
    for ch in stage.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            last_was_separator = false;
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !last_was_separator {
            normalized.push('_');
            last_was_separator = true;
        }
    }
    normalized.trim_matches('_').to_owned()
}

/// A `slow_foreground_wait` diagnostic ready to emit. Named fields instead of a
/// `(String, String)` tuple so the operator message and the JSON detail — both
/// `String` — can't be transposed at the call site.
struct SlowForegroundWait {
    message: String,
    detail: String,
}

/// Build the `slow_foreground_wait` payload when `ms` exceeds `threshold`; `None`
/// at or below it. Pure so the threshold decision and payload shape are
/// unit-testable without a tracing subscriber.
fn slow_foreground_wait_payload(
    label: &str,
    ms: u64,
    threshold: u64,
) -> Option<SlowForegroundWait> {
    if ms <= threshold {
        return None;
    }
    let message =
        format!("{label} held the foreground launch path for {ms}ms (over {threshold}ms)");
    let detail = serde_json::json!({
        "label": label,
        "duration_ms": ms,
        "threshold_ms": threshold,
    })
    .to_string();
    Some(SlowForegroundWait { message, detail })
}

fn timing_detail(name: &str, duration_ms: Option<u64>, detail: Option<&str>) -> String {
    let mut value = serde_json::json!({ "name": name });
    if let Some(ms) = duration_ms {
        value["duration_ms"] = serde_json::Value::from(ms);
    }
    if let Some(detail) = detail.filter(|detail| !detail.is_empty()) {
        value["detail"] = serde_json::Value::from(detail);
    }
    value.to_string()
}

fn cap_crash_evidence_for_export(evidence: &str) -> String {
    crate::redact::redact_and_cap(evidence, CRASH_EVIDENCE_EXPORT_CAP)
}

impl jackin_core::LaunchDiagnostics for RunDiagnostics {
    fn run_id(&self) -> &str {
        &self.run_id
    }

    fn compact(&self, kind: &str, message: &str) {
        self.compact(kind, message);
    }

    fn error(&self, kind: &str, message: &str, error_type: Option<&str>) {
        self.error_typed(kind, message, error_type);
    }

    fn stage(
        &self,
        kind: &str,
        stage: jackin_core::LaunchStage,
        message: &str,
        detail: Option<&str>,
    ) {
        self.stage(kind, stage.into(), message, detail);
    }
}
