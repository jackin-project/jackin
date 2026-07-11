//! Run-level diagnostics: write structured JSONL events to `~/.jackin/data/diagnostics/runs/<id>.jsonl`.
//!
//! One `RunDiagnostics` per process, held in a `OnceLock`. Rotates stale run
//! artifacts automatically on init. Not responsible for log formatting shown
//! to the operator — that is `clog!`/`cdebug!`; this writes machine-readable
//! JSONL for post-hoc triage.
//!
//! The on-disk JSONL file is the *fallback* sink, keyed on whether OTLP export
//! is active — not on `--debug`:
//!
//! * OTLP export active (endpoint configured and the exporter installed) → no
//!   file by default; the backend is the sink. Set `JACKIN_DIAGNOSTICS_FILE=1`
//!   to additionally write the file and see telemetry on *both* sides.
//! * OTLP export not active (no endpoint, an unsupported protocol, or a failed
//!   exporter build) → the file is written: it is the only durable sink.
//!
//! `--debug` does not change file creation — it only widens the firehose written
//! into whatever sink is active. Either way the `RunDiagnostics` exists (it
//! carries the run id and powers OTLP export and `active_run`); when the file is
//! off, `writer` is `None`. Failures stay visible regardless via the compact
//! operator-notice channel (`emit_compact_line`), which never depends on the file.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Arguments;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anstyle_parse::{DefaultCharAccumulator, Parser, Perform};
use anyhow::Context;
use owo_colors::OwoColorize;
use rand::RngExt as _;
use serde::Serialize;

use jackin_core::JackinPaths;

#[cfg(test)]
mod tests;

const RUN_DIR: &str = "diagnostics/runs";
pub(crate) const MAX_RUN_ARTIFACTS: usize = 200;
pub(crate) const MAX_RUN_ARTIFACT_AGE: Duration = Duration::from_hours(720);
const CRASH_EVIDENCE_EXPORT_CAP: usize = 4096;
const MAX_HISTOGRAM_SAMPLES: usize = 1024;

static ACTIVE_RUN: OnceLock<Mutex<Option<Arc<RunDiagnostics>>>> = OnceLock::new();
static RUN_REGISTRY: OnceLock<Mutex<HashMap<String, Weak<RunDiagnostics>>>> = OnceLock::new();
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

fn run_registry() -> &'static Mutex<HashMap<String, Weak<RunDiagnostics>>> {
    RUN_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn locked<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn should_flush_immediately(kind: &str, level: &str) -> bool {
    level.eq_ignore_ascii_case("ERROR") || kind.ends_with("_failed")
}

#[derive(Debug)]
pub struct RunDiagnostics {
    run_id: String,
    path: PathBuf,
    debug: bool,
    /// `None` when the run file is gated off (OTLP export is the active sink and
    /// `JACKIN_DIAGNOSTICS_FILE` is unset). Event recording still updates
    /// `metrics`; only the JSONL write is skipped.
    writer: Option<Mutex<BufWriter<File>>>,
    /// Per-stage start timestamps for wall-clock timing (Defect 47.5).
    stage_starts: Mutex<HashMap<String, Instant>>,
    /// Per-stage tracing spans so all progress events for a launch stage share
    /// a stable span id in the JSONL.
    stage_spans: Mutex<HashMap<String, tracing::Span>>,
    /// Fine-grained timing starts nested under broad launch stages.
    timing_starts: Mutex<HashMap<String, Instant>>,
    /// Accumulated per-stage durations for the end-of-run summary.
    stage_durations_ms: Mutex<Vec<(String, u64)>>,
    metrics: Mutex<DiagnosticsMetrics>,
    otlp_internal_notified: AtomicBool,
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
        if let Some(run) = guard.as_ref() {
            run.flush_writer();
        }
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

#[derive(Debug, Serialize)]
struct JsonEvent<'a> {
    ts_ms: u128,
    run_id: &'a str,
    /// `OTel` 32-hex trace id when an OTLP span is active; otherwise the run id
    /// (file-only / offline fallback so the field stays non-empty for schema
    /// stability — not joinable to an OTLP backend in that mode).
    trace_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// `OTel` 16-hex span id when an OTLP span is active; otherwise the
    /// tracing-registry u64 string when a span is entered.
    span_id: Option<&'a str>,
    kind: &'a str,
    #[serde(rename = "event.name")]
    event_name: &'a str,
    #[serde(rename = "event.outcome")]
    event_outcome: &'a str,
    #[serde(rename = "jackin.component")]
    jackin_component: &'a str,
    #[serde(rename = "jackin.operation")]
    jackin_operation: &'a str,
    #[serde(rename = "jackin.category")]
    jackin_category: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<&'a str>,
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

#[cfg(feature = "otlp")]
#[derive(Clone, Debug, Default)]
pub(crate) struct DomainMetricsSnapshot {
    pub event_counts: BTreeMap<String, u64>,
    pub cache_hits: u64,
    pub cache_misses: u64,
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
        let run_id = external_run_id_from_env().unwrap_or_else(mint_run_id);
        // `init_tracing` returns whether OTLP export was actually installed. That
        // drives the file gate: the file is the fallback sink, written whenever
        // the backend is NOT receiving (or forced on with JACKIN_DIAGNOSTICS_FILE).
        let (otlp_active, otlp_error) = match crate::observability::init_tracing(debug, &run_id) {
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
        let persist = !otlp_active || diagnostics_file_forced();
        let dir = run_dir(paths);
        let path = dir.join(format!("{run_id}.jsonl"));
        let writer = if persist {
            fs::create_dir_all(&dir)
                .with_context(|| format!("creating diagnostics run dir {}", dir.display()))?;
            prune_old_runs_in_dir(&dir, None);
            #[expect(
                clippy::disallowed_methods,
                reason = "diagnostics artifact creation is not part of a render loop"
            )]
            let file = restrict_to_owner(OpenOptions::new().create_new(true).write(true))
                .open(&path)
                .with_context(|| format!("creating diagnostics run artifact {}", path.display()))?;
            Some(Mutex::new(BufWriter::new(file)))
        } else {
            None
        };
        let run = Arc::new(Self {
            run_id,
            path,
            debug,
            writer,
            stage_starts: Mutex::new(HashMap::new()),
            stage_spans: Mutex::new(HashMap::new()),
            timing_starts: Mutex::new(HashMap::new()),
            stage_durations_ms: Mutex::new(Vec::new()),
            metrics: Mutex::new(DiagnosticsMetrics::default()),
            otlp_internal_notified: AtomicBool::new(false),
        });
        run_registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(run.run_id.clone(), Arc::downgrade(&run));
        if let Some(error) = otlp_error {
            // Record into the run file (on by construction here, since OTLP is
            // inactive) and emit the compact operator notice (stderr / deferred
            // under a rich TUI). Visibility never depends on the file alone.
            let line = format!("OTLP export disabled: {error}");
            run.compact("otlp", &line);
            crate::logging::emit_compact_line("otlp", &line);
        }
        crate::observability::emit_jsonl_event(
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
        let active_dir = self.path.parent().map(Path::to_path_buf);
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

    /// The run's JSONL path. NOTE: the file exists on disk only when
    /// [`persists`](Self::persists) is true; when OTLP is the active sink and the
    /// file gate is off, this is the path the file *would* have, never created.
    /// Callers that read or display it must gate on `persists()` first.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the run is persisting a JSONL file (and its sidecars). `false`
    /// when OTLP export is the active sink and the file gate is off.
    #[must_use]
    pub fn persists(&self) -> bool {
        self.writer.is_some()
    }

    pub fn command_output_path(&self, name: &str) -> PathBuf {
        self.path.with_file_name(format!(
            "{}.{}.log",
            self.run_id,
            sanitize_artifact_name(name)
        ))
    }

    pub fn write_command_output(
        &self,
        name: &str,
        command: &str,
        cwd: Option<&Path>,
        status: ExitStatus,
        stdout: &[u8],
        stderr: &[u8],
    ) -> Option<PathBuf> {
        // Sidecars share the run file's gate: no file run, no sidecars.
        self.writer.as_ref()?;
        let path = self.command_output_path(name);
        #[expect(
            clippy::disallowed_methods,
            reason = "diagnostics sidecar creation is not part of a render loop"
        )]
        let mut file =
            restrict_to_owner(OpenOptions::new().create(true).truncate(true).write(true))
                .open(&path)
                .ok()?;
        let cwd = cwd.map_or_else(
            || "(current process cwd)".to_owned(),
            |path| path.display().to_string(),
        );
        drop(writeln!(file, "run: {}", self.run_id));
        drop(writeln!(file, "command: {command}"));
        drop(writeln!(file, "cwd: {cwd}"));
        drop(writeln!(file, "status: {status}"));
        drop(writeln!(file));
        drop(writeln!(file, "----- stdout -----"));
        let stdout = strip_bytes(stdout);
        let stdout = String::from_utf8_lossy(&stdout);
        let stdout = crate::secret_scrub::scrub_secrets(&stdout);
        drop(file.write_all(stdout.as_bytes()));
        if !stdout.ends_with('\n') {
            drop(writeln!(file));
        }
        drop(writeln!(file, "----- stderr -----"));
        let stderr = strip_bytes(stderr);
        let stderr = String::from_utf8_lossy(&stderr);
        let stderr = crate::secret_scrub::scrub_secrets(&stderr);
        drop(file.write_all(stderr.as_bytes()));
        if !stderr.ends_with('\n') {
            drop(writeln!(file));
        }
        Some(path)
    }

    pub fn compact(&self, kind: &str, message: &str) {
        crate::observability::emit_jsonl_event(&self.run_id, kind, message, None, None);
    }

    pub fn error(&self, kind: &str, message: &str) {
        crate::observability::emit_jsonl_error(&self.run_id, kind, message, None, None);
    }

    pub fn error_typed(&self, kind: &str, message: &str, error_type: Option<&str>) {
        if let Some(error_type) = error_type {
            crate::metrics::incr_errors(error_type);
        }
        crate::observability::emit_jsonl_error_typed(
            &self.run_id,
            kind,
            message,
            None,
            None,
            error_type,
        );
    }

    pub fn stage(&self, kind: &str, stage: &str, message: &str, detail: Option<&str>) {
        // Track wall-clock stage timings for the end-of-run summary (Defect 47.5).
        // A stage runs on the operator's foreground launch path, so a slow one is a
        // foreground wait the operator is paying for; capture its duration to
        // explain any wait over the threshold (acceptance: explain every
        // foreground wait over 500ms).
        let mut foreground_wait_ms: Option<u64> = None;
        let enriched_detail = match kind {
            "stage_started" => {
                locked(&self.stage_starts).insert(stage.to_owned(), Instant::now());
                locked(&self.stage_spans).insert(stage.to_owned(), launch_stage_span(stage));
                detail.map(String::from)
            }
            "stage_done" => {
                let elapsed_ms = locked(&self.stage_starts)
                    .remove(stage)
                    .map(|t| t.elapsed().as_millis() as u64);
                elapsed_ms.map_or_else(
                    || detail.map(String::from),
                    |ms| {
                        foreground_wait_ms = Some(ms);
                        locked(&self.stage_durations_ms).push((stage.to_owned(), ms));
                        locked(&self.metrics).push_stage_duration(stage.to_owned(), ms);
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
                let _ = locked(&self.stage_starts).remove(stage);
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
            crate::observability::emit_jsonl_error_typed(
                &self.run_id,
                kind,
                message,
                Some(stage),
                enriched_detail.as_deref(),
                None,
            );
        } else {
            crate::observability::emit_jsonl_event(
                &self.run_id,
                kind,
                message,
                Some(stage),
                enriched_detail.as_deref(),
            );
        }
        if let Some(ms) = foreground_wait_ms {
            self.explain_foreground_wait(stage, ms);
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
        crate::observability::emit_jsonl_event(
            &self.run_id,
            "slow_foreground_wait",
            &wait.message,
            Some(label),
            Some(&wait.detail),
        );
    }

    pub fn timing_started(&self, stage: &str, name: &str, detail: Option<&str>) {
        let key = timing_key(stage, name);
        locked(&self.timing_starts).insert(key, Instant::now());
        let event_detail = timing_detail(name, None, detail);
        let span = self.current_stage_span(stage);
        let _entered = span.as_ref().map(tracing::Span::enter);
        crate::observability::emit_jsonl_event(
            &self.run_id,
            "timing_started",
            &format!("{name} started"),
            Some(stage),
            Some(&event_detail),
        );
    }

    pub fn timing_done(&self, stage: &str, name: &str, detail: Option<&str>) {
        let key = timing_key(stage, name);
        let elapsed_ms = locked(&self.timing_starts)
            .remove(&key)
            .map(|start| start.elapsed().as_millis() as u64);
        if let Some(ms) = elapsed_ms {
            locked(&self.metrics).push_timing_duration(format!("{stage}/{name}"), ms);
        }
        let event_detail = timing_detail(name, elapsed_ms, detail);
        let span = self.current_stage_span(stage);
        let _entered = span.as_ref().map(tracing::Span::enter);
        crate::observability::emit_jsonl_event(
            &self.run_id,
            "timing_done",
            &format!("{name} done"),
            Some(stage),
            Some(&event_detail),
        );
        if let Some(ms) = elapsed_ms {
            self.explain_foreground_wait(&key, ms);
        }
    }

    fn current_stage_span(&self, stage: &str) -> Option<tracing::Span> {
        locked(&self.stage_spans).get(stage).cloned()
    }

    fn stage_span_for(&self, kind: &str, stage: &str) -> tracing::Span {
        let mut spans = locked(&self.stage_spans);
        if matches!(kind, "stage_done" | "stage_failed" | "stage_skipped") {
            spans
                .remove(stage)
                .unwrap_or_else(|| launch_stage_span(stage))
        } else {
            spans
                .entry(stage.to_owned())
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
        crate::observability::emit_jsonl_event(
            &self.run_id,
            "run_summary",
            "stage durations and counters",
            None,
            Some(&summary),
        );
        if !unclosed.is_empty() {
            crate::observability::emit_jsonl_event(
                &self.run_id,
                "diagnostics",
                &format!("unclosed: {}", unclosed.join(", ")),
                None,
                None,
            );
        }
        self.flush_writer();
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
                    .map(|key| format!("span:{}", display_unclosed_key(key))),
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

    pub(crate) fn flush_writer(&self) {
        let Some(writer) = &self.writer else {
            return;
        };
        let mut guard = locked(writer);
        drop(guard.flush());
    }

    pub fn debug(&self, category: &str, line: &str) -> bool {
        if !crate::logging::debug_capture_enabled(category, self.debug) {
            return false;
        }
        crate::observability::emit_jsonl_event(&self.run_id, "debug", line, None, Some(category));
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
        crate::observability::emit_jsonl_event(
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
    /// `multiplexer.log` tail — passed in by the caller which already fetched
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
            crate::observability::emit_jsonl_error(
                &self.run_id,
                kind,
                &msg,
                Some(container_name),
                Some(&detail),
            );
        } else {
            crate::observability::emit_jsonl_event(
                &self.run_id,
                kind,
                &msg,
                Some(container_name),
                Some(&detail),
            );
        }
        if let Some(evidence) = crash_evidence.filter(|s| !s.is_empty()) {
            let capped_evidence = cap_crash_evidence_for_export(evidence);
            crate::observability::emit_jsonl_error(
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
        crate::observability::emit_jsonl_event(
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
        crate::observability::emit_jsonl_event(
            &self.run_id,
            "subprocess_done",
            "subprocess exited",
            Some(program),
            Some(&detail),
        );
    }

    pub(crate) fn record_from_layer(
        &self,
        kind: &str,
        message: &str,
        stage: Option<&str>,
        detail: Option<&str>,
        span_id: Option<&str>,
        level: &str,
    ) {
        self.record_direct(kind, message, stage, detail, span_id, level);
        self.flush_writer();
    }

    /// Record an OpenTelemetry-internal diagnostic (an export failure, dropped
    /// batch, partial-success, …) captured from OpenTelemetry's own `tracing`
    /// events. `level` is the SDK event severity (`WARN`/`ERROR`). Written as
    /// `otlp_internal` so "telemetry isn't reaching the backend" is durable in
    /// the run file (its count rides the run summary too). The *first* such event
    /// also emits one compact operator notice — stderr on a plain CLI, deferred
    /// to teardown under a rich TUI — so an export that fails on the wire is
    /// visible even when the file sink is gated off (the common OTLP-active case).
    /// Only the first is announced; the rest are silent to avoid 5-second spam.
    pub(crate) fn record_otlp_internal(&self, level: &str, message: &str) {
        let first = !self.otlp_internal_notified.swap(true, Ordering::Relaxed);
        self.record_direct(
            crate::observability::otel_events::OTLP_INTERNAL,
            message,
            None,
            Some(level),
            None,
            level,
        );
        if first {
            // Terminal-only notice: record_direct already wrote the file, and a
            // tracing emit here would re-enter the subscriber (this runs inside
            // the diagnostics layer).
            crate::logging::emit_operator_notice(&format!(
                "telemetry export issue (run telemetry may be incomplete): {message}"
            ));
        }
    }

    // Sink-only helper. Call directly only from `record_from_layer` and
    // `record_otlp_internal`; normal diagnostics should emit tracing events.
    fn record_direct(
        &self,
        kind: &str,
        message: &str,
        stage: Option<&str>,
        detail: Option<&str>,
        fallback_span_id: Option<&str>,
        level: &str,
    ) {
        self.record_metrics(kind);
        // Counts above always update (they feed the run summary, which OTLP also
        // exports); the JSONL write only happens when the file sink is on.
        let Some(writer) = &self.writer else {
            return;
        };
        let taxonomy =
            crate::observability::event_taxonomy(kind, message, stage, detail, None, level);
        // Prefer live OTel hex ids; fall back to run_id + tracing-registry span.
        let (owned_trace, owned_span) =
            crate::observability::correlation_ids(&self.run_id, fallback_span_id);
        let event = JsonEvent {
            ts_ms: now_ms(),
            run_id: &self.run_id,
            trace_id: owned_trace.as_str(),
            span_id: owned_span.as_deref(),
            kind,
            event_name: &taxonomy.event_name,
            event_outcome: taxonomy.outcome,
            jackin_component: taxonomy.component,
            jackin_operation: &taxonomy.operation,
            jackin_category: &taxonomy.category,
            message,
            stage,
            detail,
        };
        let Ok(line) = serde_json::to_string(&event) else {
            return;
        };
        let mut guard = writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        drop(writeln!(guard, "{line}"));
        if should_flush_immediately(kind, level) {
            // Routine records flush when the run guard drops or the summary is
            // emitted. Error-tier records force a flush so a crash immediately
            // after the error still leaves evidence in the local fallback file.
            drop(guard.flush());
        }
    }

    fn record_metrics(&self, kind: &str) {
        let mut metrics = locked(&self.metrics);
        *metrics.event_counts.entry(kind.to_owned()).or_default() += 1;
        if kind.contains("cache_hit") {
            metrics.cache_hits += 1;
        }
        if kind.contains("cache_miss") {
            metrics.cache_misses += 1;
        }
    }

    #[cfg(feature = "otlp")]
    pub(crate) fn domain_metrics_snapshot(&self) -> DomainMetricsSnapshot {
        let metrics = locked(&self.metrics);
        DomainMetricsSnapshot {
            event_counts: metrics.event_counts.clone(),
            cache_hits: metrics.cache_hits,
            cache_misses: metrics.cache_misses,
        }
    }
}

pub fn active_debug(category: &str, line: &str) -> bool {
    active_run().is_some_and(|run| run.debug(category, line))
}

pub fn active_timing_started(stage: &str, name: &str, detail: Option<&str>) {
    if let Some(run) = active_run() {
        run.timing_started(stage, name, detail);
    }
}

pub fn active_timing_done(stage: &str, name: &str, detail: Option<&str>) {
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
            .get(&run_dir(paths))
            .and_then(Weak::upgrade);
    }

    #[cfg(not(test))]
    {
        let run = active_run()?;
        run.path.starts_with(run_dir(paths)).then_some(run)
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

pub(crate) fn run_by_id(run_id: &str) -> Option<Arc<RunDiagnostics>> {
    run_registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(run_id)
        .and_then(Weak::upgrade)
}

pub fn prune_old_runs(paths: &JackinPaths) {
    let active_run_id = active_run().map(|run| run.run_id().to_owned());
    prune_old_runs_in_dir(&run_dir(paths), active_run_id.as_deref());
}

pub fn prune_all_runs(paths: &JackinPaths) -> anyhow::Result<()> {
    let dir = run_dir(paths);
    section("Diagnostics", "removing diagnostic runs");
    let row = start("Deleting", "diagnostics");

    let active_path = active_run().map(|run| run.path().to_path_buf());
    let result = active_path
        .as_deref()
        .filter(|path| path.parent() == Some(dir.as_path()))
        .map_or_else(
            || prune_runs_all(&dir),
            |active| prune_runs_preserving(&dir, active),
        );

    row.complete(result, |error| {
        format!("could not remove diagnostics: {error}")
    })
}

fn prune_runs_all(dir: &Path) -> anyhow::Result<()> {
    match fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(anyhow::Error::from(error).context(format!(
            "failed to remove diagnostics runs at {}",
            dir.display()
        ))),
    }
}

pub(crate) fn prune_runs_preserving(dir: &Path, preserved_path: &Path) -> anyhow::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(anyhow::Error::from(error).context(format!(
                "failed to read diagnostics runs at {}",
                dir.display()
            )));
        }
    };

    for entry in entries {
        let entry =
            entry.with_context(|| format!("reading diagnostics run in {}", dir.display()))?;
        let path = entry.path();
        if path == preserved_path {
            continue;
        }
        remove_run_entry(&path)
            .with_context(|| format!("removing diagnostics run {}", path.display()))?;
    }
    Ok(())
}

fn remove_run_entry(path: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)
    } else {
        if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            remove_run_sidecars(path);
        }
        fs::remove_file(path)
    }
}

/// Remove a diagnostics run's `.jsonl` plus its `{stem}.*` sidecars. The caller
/// has already established `path` is a run `.jsonl`, so unlike `remove_run_entry`
/// this skips the dir/file stat.
fn remove_jsonl_run(path: &Path) {
    remove_run_sidecars(path);
    drop(fs::remove_file(path));
}

fn remove_run_sidecars(run_path: &Path) {
    let Some(dir) = run_path.parent() else {
        return;
    };
    let Some(stem) = run_path.file_stem().and_then(|stem| stem.to_str()) else {
        return;
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let prefix = format!("{stem}.");
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with(&prefix) && path != run_path {
            drop(fs::remove_file(path));
        }
    }
}

pub(crate) fn run_dir(paths: &JackinPaths) -> PathBuf {
    paths.data_dir.join(RUN_DIR)
}

/// Characters allowed verbatim in a run id or run-artifact filename: ASCII
/// alphanumerics plus `-`/`_`. Everything else is dropped or replaced,
/// depending on the caller.
fn is_run_id_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'
}

fn sanitize_artifact_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if is_run_id_char(ch) {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    out.trim_matches('-').chars().take(64).collect()
}

/// Whether an env-flag string is truthy: `1`/`true`/`yes`/`on`, case- and
/// whitespace-insensitive. Pure so the vocabulary can be unit-tested without
/// touching process env.
pub(crate) fn flag_is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Whether the operator forced the JSONL run file on via
/// `JACKIN_DIAGNOSTICS_FILE`. Truthy values per [`flag_is_truthy`]. When OTLP
/// export is inactive the file is written regardless (it is the only sink); this
/// gate only matters when OTLP is active and the operator also wants the file.
fn diagnostics_file_forced() -> bool {
    std::env::var("JACKIN_DIAGNOSTICS_FILE").is_ok_and(|value| flag_is_truthy(&value))
}

pub(crate) fn mint_run_id() -> String {
    let mut rng = rand::rng();
    let n: u32 = rng.random();
    // A bare unique value — no prefix; six lowercase hex digits.
    format!("{:06x}", n & 0x00ff_ffff)
}

fn external_run_id_from_env() -> Option<String> {
    std::env::var("OTEL_RESOURCE_ATTRIBUTES")
        .ok()
        .and_then(|attrs| external_run_id_from_resource_attributes(&attrs))
        .or_else(|| {
            std::env::var("PARALLAX_RUN_ID")
                .ok()
                .and_then(|id| normalize_external_run_id(&id))
        })
}

pub(crate) fn external_run_id_from_resource_attributes(attrs: &str) -> Option<String> {
    attrs
        .split(',')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(key, value)| {
            (key.trim() == "parallax.run.id")
                .then(|| normalize_external_run_id(value))
                .flatten()
        })
}

/// Normalize an externally-supplied run id: trim, strip the `run_` prefix, keep
/// only run-id chars, cap at 64. Returns `None` when nothing usable remains, so
/// the empty-string invariant lives here rather than at each call site.
fn normalize_external_run_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let id: String = trimmed
        .strip_prefix("run_")
        .unwrap_or(trimmed)
        .chars()
        .filter(|&ch| is_run_id_char(ch))
        .take(64)
        .collect();
    (!id.is_empty()).then_some(id)
}

/// A fresh session id for the capsule's `session.id`. One daemon run is one
/// session; the id groups all of its OTLP telemetry into a single timeline.
#[must_use]
pub fn mint_session_id() -> String {
    let mut rng = rand::rng();
    let n: u32 = rng.random();
    format!("jk-session-{:06x}", n & 0x00ff_ffff)
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn timing_key(stage: &str, name: &str) -> String {
    format!("{stage}\0{name}")
}

fn launch_stage_span(stage: &str) -> tracing::Span {
    let otel_name = format!("launch.{}", normalize_stage_name(stage));
    let span = tracing::info_span!(
        "launch_stage",
        stage = stage,
        otel.name = otel_name.as_str(),
        otel.status_code = tracing::field::Empty,
        otel.status_description = tracing::field::Empty,
    );
    // Derived-image build is a peer subsystem of launch: link it to the active
    // launch span so the BuildKit subtrace is not an unparented peer (plan 044).
    #[cfg(feature = "otlp")]
    if stage == "derived image" {
        use opentelemetry::trace::TraceContextExt as _;
        use tracing_opentelemetry::OpenTelemetrySpanExt as _;
        let parent_ctx = tracing::Span::current().context();
        let span_ctx = parent_ctx.span().span_context().clone();
        if span_ctx.is_valid() {
            span.add_link(span_ctx);
        }
    }
    span
}

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

/// Owner-only mode for new diagnostics files. The JSONL firehose and the
/// command-output sidecar can carry tokens or credentials captured from
/// external-command stdout, so they must not be world-readable.
#[cfg(unix)]
fn restrict_to_owner(opts: &mut OpenOptions) -> &mut OpenOptions {
    use std::os::unix::fs::OpenOptionsExt as _;
    opts.mode(0o600)
}

#[cfg(not(unix))]
fn restrict_to_owner(opts: &mut OpenOptions) -> &mut OpenOptions {
    opts
}

pub(crate) fn prune_old_runs_in_dir(dir: &Path, active_run: Option<&str>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    let mut entries: Vec<(PathBuf, SystemTime)> = read_dir
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                return None;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if active_run == Some(stem) {
                return None;
            }
            let modified = entry.metadata().and_then(|m| m.modified()).ok()?;
            Some((path, modified))
        })
        .collect();

    for (path, modified) in &entries {
        if now
            .duration_since(*modified)
            .is_ok_and(|age| age > MAX_RUN_ARTIFACT_AGE)
        {
            remove_jsonl_run(path);
        }
    }

    entries.retain(|(path, _)| path.exists());
    entries.sort_by_key(|(_, modified)| *modified);
    let overflow = entries.len().saturating_sub(MAX_RUN_ARTIFACTS);
    for (path, _) in entries.into_iter().take(overflow) {
        remove_jsonl_run(&path);
    }
}

// Local copies of the presentation helpers that were moved to `jackin-tui` in
// A3. Inlined here so this crate (L1, depended on by 8 L0/L1/L2 crates) does
// not need to pull `jackin-tui` (L3) for the two small helpers `prune_all_runs`
// uses. The implementations are byte-identical to the originals in
// `jackin_core::{ansi_text, prune_output}` before their A3 move.

#[must_use]
fn strip_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut parser = Parser::<DefaultCharAccumulator>::default();
    let mut performer = PlainPerformer { output: Vec::new() };
    for &byte in bytes {
        parser.advance(&mut performer, byte);
    }
    performer.output
}

struct PlainPerformer {
    output: Vec<u8>,
}

impl Perform for PlainPerformer {
    fn print(&mut self, c: char) {
        let mut buf = [0u8; 4];
        self.output
            .extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    }

    fn execute(&mut self, byte: u8) {
        if matches!(byte, b'\n' | b'\r' | b'\t') {
            self.output.push(byte);
        }
    }
}

const STATUS_COLUMN: usize = 78;

fn flush_stdout() {
    drop(std::io::stdout().flush());
}

fn stdout_line(args: Arguments<'_>) {
    let mut stdout = std::io::stdout().lock();
    drop(writeln!(stdout, "{args}"));
}

fn stdout_fragment(args: Arguments<'_>) {
    let mut stdout = std::io::stdout().lock();
    drop(write!(stdout, "{args}"));
}

fn section(label: &str, detail: impl std::fmt::Display) {
    stdout_line(format_args!(""));
    stdout_line(format_args!("  {} {}", label.bold(), detail.dimmed()));
    flush_stdout();
}

fn ok_label() {
    stdout_line(format_args!(" {}", "OK".green().bold()));
}

fn failed_label(detail: impl std::fmt::Display) {
    stdout_line(format_args!(" {}", "FAILED".red().bold()));
    stdout_line(format_args!("      {detail}"));
}

fn start(action: &str, target: impl std::fmt::Display) -> PendingRow {
    let (prefix, dots) = pending_parts(action, target);
    stdout_fragment(format_args!("    {} {}", prefix.bold(), dots.dimmed()));
    flush_stdout();
    PendingRow { finalized: false }
}

fn pending_parts(action: &str, target: impl std::fmt::Display) -> (String, String) {
    let (prefix, prefix_chars) = fit_prefix(format!("{action} {target}"));
    let dots = ".".repeat(STATUS_COLUMN.saturating_sub(prefix_chars).max(3));
    (prefix, dots)
}

fn fit_prefix(prefix: String) -> (String, usize) {
    let max = STATUS_COLUMN.saturating_sub(4);
    let keep = max.saturating_sub(3);
    let mut total = 0usize;
    let mut truncate_at: Option<usize> = None;
    for (idx, _) in prefix.char_indices() {
        if total == keep && truncate_at.is_none() {
            truncate_at = Some(idx);
        }
        if total > max {
            let cut = truncate_at.unwrap_or(idx);
            let mut fitted = prefix[..cut].to_string();
            fitted.push_str("...");
            return (fitted, keep + 3);
        }
        total += 1;
    }
    (prefix, total)
}

#[derive(Debug)]
pub struct PendingRow {
    #[expect(
        dead_code,
        reason = "Drop guard: closed in Drop impl if caller forgets to finalize"
    )]
    finalized: bool,
}

impl PendingRow {
    /// Finalize the row from a `Result`: print `OK` on success, `FAILED` on error.
    pub fn complete<T, E, F>(self, result: Result<T, E>, message: F) -> Result<T, E>
    where
        F: FnOnce(&E) -> String,
    {
        match result {
            Ok(value) => {
                ok_label();
                Ok(value)
            }
            Err(error) => {
                failed_label(message(&error));
                Err(error)
            }
        }
    }
}

impl jackin_core::launch_progress::LaunchDiagnostics for RunDiagnostics {
    fn run_id(&self) -> &str {
        &self.run_id
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn persists(&self) -> bool {
        self.persists()
    }

    fn command_output_path(&self, name: &str) -> PathBuf {
        self.command_output_path(name)
    }

    fn compact(&self, kind: &str, message: &str) {
        self.compact(kind, message);
    }

    fn error(&self, kind: &str, message: &str, error_type: Option<&str>) {
        self.error_typed(kind, message, error_type);
    }

    fn stage(&self, kind: &str, stage: &str, message: &str, detail: Option<&str>) {
        self.stage(kind, stage, message, detail);
    }
}
