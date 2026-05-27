use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use rand::RngExt as _;
use serde::Serialize;

use crate::paths::JackinPaths;

const RUN_DIR: &str = "diagnostics/runs";
const MAX_RUN_ARTIFACTS: usize = 200;
const MAX_RUN_ARTIFACT_AGE: Duration = Duration::from_hours(720);

static ACTIVE_RUN: OnceLock<Mutex<Option<Arc<RunDiagnostics>>>> = OnceLock::new();

fn active_slot() -> &'static Mutex<Option<Arc<RunDiagnostics>>> {
    ACTIVE_RUN.get_or_init(|| Mutex::new(None))
}

#[derive(Debug)]
pub struct RunDiagnostics {
    run_id: String,
    path: PathBuf,
    debug: bool,
    writer: Mutex<BufWriter<File>>,
}

#[derive(Debug)]
pub struct ActiveRunGuard {
    previous: Option<Arc<RunDiagnostics>>,
}

impl Drop for ActiveRunGuard {
    fn drop(&mut self) {
        let mut guard = active_slot()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = self.previous.take();
    }
}

#[derive(Debug, Serialize)]
struct JsonEvent<'a> {
    ts_ms: u128,
    run_id: &'a str,
    trace_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    span_id: Option<&'a str>,
    kind: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<&'a str>,
}

impl RunDiagnostics {
    pub fn start(paths: &JackinPaths, debug: bool, command: &str) -> anyhow::Result<Arc<Self>> {
        let run_id = mint_run_id();
        let dir = run_dir(paths);
        fs::create_dir_all(&dir)
            .with_context(|| format!("creating diagnostics run dir {}", dir.display()))?;
        prune_old_runs_in_dir(&dir, None);
        let path = dir.join(format!("{run_id}.jsonl"));
        let mut opts = OpenOptions::new();
        opts.create_new(true).write(true);
        restrict_to_owner(&mut opts);
        let file = opts
            .open(&path)
            .with_context(|| format!("creating diagnostics run artifact {}", path.display()))?;
        let run = Arc::new(Self {
            run_id,
            path,
            debug,
            writer: Mutex::new(BufWriter::new(file)),
        });
        run.compact("run", &format!("command {command} started"));
        Ok(run)
    }

    pub fn activate(self: &Arc<Self>) -> ActiveRunGuard {
        let previous = active_slot()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .replace(Arc::clone(self));
        ActiveRunGuard { previous }
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn path(&self) -> &Path {
        &self.path
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
        let path = self.command_output_path(name);
        let mut opts = OpenOptions::new();
        opts.create(true).truncate(true).write(true);
        restrict_to_owner(&mut opts);
        let mut file = opts.open(&path).ok()?;
        let cwd = cwd.map_or_else(
            || "(current process cwd)".to_string(),
            |path| path.display().to_string(),
        );
        let _ = writeln!(file, "run: {}", self.run_id);
        let _ = writeln!(file, "command: {command}");
        let _ = writeln!(file, "cwd: {cwd}");
        let _ = writeln!(file, "status: {status}");
        let _ = writeln!(file);
        let _ = writeln!(file, "----- stdout -----");
        let stdout = crate::ansi_text::strip_bytes(stdout);
        let _ = file.write_all(&stdout);
        if !stdout.ends_with(b"\n") {
            let _ = writeln!(file);
        }
        let _ = writeln!(file, "----- stderr -----");
        let stderr = crate::ansi_text::strip_bytes(stderr);
        let _ = file.write_all(&stderr);
        if !stderr.ends_with(b"\n") {
            let _ = writeln!(file);
        }
        Some(path)
    }

    pub fn compact(&self, kind: &str, message: &str) {
        self.write(kind, message, None, None, None);
    }

    pub fn stage(&self, kind: &str, stage: &str, message: &str, detail: Option<&str>) {
        // `span_id` is left unset: the `stage` field already identifies the
        // span, so repeating it as the span id is pure duplication.
        self.write(kind, message, Some(stage), detail, None);
    }

    pub fn debug(&self, category: &str, line: &str) -> bool {
        if !self.debug {
            return false;
        }
        self.write("debug", line, None, Some(category), None);
        true
    }

    fn write(
        &self,
        kind: &str,
        message: &str,
        stage: Option<&str>,
        detail: Option<&str>,
        span_id: Option<&str>,
    ) {
        let event = JsonEvent {
            ts_ms: now_ms(),
            run_id: &self.run_id,
            trace_id: &self.run_id,
            span_id,
            kind,
            message,
            stage,
            detail,
        };
        let Ok(line) = serde_json::to_string(&event) else {
            return;
        };
        let mut guard = self
            .writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = writeln!(guard, "{line}");
        let _ = guard.flush();
    }
}

pub fn active_debug(category: &str, line: &str) -> bool {
    active_run().is_some_and(|run| run.debug(category, line))
}

pub fn active_run() -> Option<Arc<RunDiagnostics>> {
    active_slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

pub fn prune_old_runs(paths: &JackinPaths) {
    let active_run_id = active_run().map(|run| run.run_id().to_string());
    prune_old_runs_in_dir(&run_dir(paths), active_run_id.as_deref());
}

pub fn prune_all_runs(paths: &JackinPaths) -> anyhow::Result<()> {
    let dir = run_dir(paths);
    let active_path = active_run().map(|run| run.path().to_path_buf());
    if let Some(active_path) = active_path
        .as_deref()
        .filter(|path| path.parent() == Some(dir.as_path()))
    {
        return prune_all_runs_except(&dir, active_path);
    }
    match fs::remove_dir_all(&dir) {
        Ok(()) => println!("Removed diagnostics runs ({}).", dir.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!("diagnostics runs already empty.");
        }
        Err(error) => {
            return Err(anyhow::Error::from(error).context(format!(
                "failed to remove diagnostics runs at {}",
                dir.display()
            )));
        }
    }
    Ok(())
}

fn prune_all_runs_except(dir: &Path, preserved_path: &Path) -> anyhow::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!("diagnostics runs already empty.");
            return Ok(());
        }
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
    println!(
        "Removed diagnostics runs except active run ({}).",
        dir.display()
    );
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
            let _ = fs::remove_file(path);
        }
    }
}

fn run_dir(paths: &JackinPaths) -> PathBuf {
    paths.data_dir.join(RUN_DIR)
}

fn sanitize_artifact_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    out.trim_matches('-').chars().take(64).collect()
}

fn mint_run_id() -> String {
    let mut rng = rand::rng();
    let n: u32 = rng.random();
    format!("jk-run-{:06x}", n & 0x00ff_ffff)
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

/// Owner-only mode for new diagnostics files. The JSONL firehose and the
/// command-output sidecar can carry tokens or credentials captured from
/// external-command stdout, so they must not be world-readable.
#[cfg(unix)]
fn restrict_to_owner(opts: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt as _;
    opts.mode(0o600);
}

#[cfg(not(unix))]
fn restrict_to_owner(_opts: &mut OpenOptions) {}

fn prune_old_runs_in_dir(dir: &Path, active_run: Option<&str>) {
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
            remove_run_sidecars(path);
            let _ = fs::remove_file(path);
        }
    }

    entries.retain(|(path, _)| path.exists());
    entries.sort_by_key(|(_, modified)| *modified);
    let overflow = entries.len().saturating_sub(MAX_RUN_ARTIFACTS);
    for (path, _) in entries.into_iter().take(overflow) {
        remove_run_sidecars(&path);
        let _ = fs::remove_file(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_id_has_operator_handle_shape() {
        let id = mint_run_id();
        assert!(id.starts_with("jk-run-"));
        assert_eq!(id.len(), "jk-run-42f9aa".len());
    }

    #[test]
    fn writes_jsonl_events() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = RunDiagnostics::start(&paths, true, "load").unwrap();
        run.compact("breadcrumb", "hello");
        assert!(run.debug("cmd", "docker ps"));

        let contents = fs::read_to_string(run.path()).unwrap();
        assert!(contents.contains("\"run_id\""));
        assert!(contents.contains("\"hello\""));
        assert!(contents.contains("\"debug\""));
    }

    #[test]
    fn debug_is_not_consumed_when_capture_is_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = RunDiagnostics::start(&paths, false, "load").unwrap();
        assert!(!run.debug("cmd", "docker ps"));

        let contents = fs::read_to_string(run.path()).unwrap();
        assert!(
            !contents.contains("docker ps"),
            "debug line must not be written when debug capture is disabled: {contents}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn command_output_sidecar_strips_ansi_sequences() {
        use std::os::unix::process::ExitStatusExt;

        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = RunDiagnostics::start(&paths, false, "load").unwrap();
        let path = run
            .write_command_output(
                "docker-build",
                "docker build .",
                None,
                ExitStatus::from_raw(1),
                b"\x1b[32mstep ok\x1b[0m\n",
                b"\x1b[31mboom\x1b[0m\n",
            )
            .unwrap();

        let contents = fs::read_to_string(path).unwrap();
        assert!(contents.contains("step ok"));
        assert!(contents.contains("boom"));
        assert!(
            !contents.contains('\x1b'),
            "plain sidecar log should not contain terminal escapes: {contents:?}"
        );
    }

    #[test]
    fn prune_all_runs_except_preserves_active_run_file() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let dir = run_dir(&paths);
        fs::create_dir_all(&dir).unwrap();
        let active = dir.join("jk-run-active.jsonl");
        let stale = dir.join("jk-run-stale.jsonl");
        fs::write(&active, "active").unwrap();
        fs::write(&stale, "stale").unwrap();

        prune_all_runs_except(&dir, &active).unwrap();

        assert!(active.exists(), "active run must remain retrievable");
        assert!(!stale.exists(), "stale run should be pruned");
    }

    #[test]
    fn prune_removes_over_age_run_with_its_sidecar() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let old_jsonl = dir.join("jk-run-old.jsonl");
        let old_log = dir.join("jk-run-old.docker-build.log");
        fs::write(&old_jsonl, "{}").unwrap();
        fs::write(&old_log, "build output").unwrap();
        // Backdate the run past the retention age; the sidecar is matched by
        // stem, not by its own mtime, so only the .jsonl needs an old time.
        let ancient = SystemTime::now() - MAX_RUN_ARTIFACT_AGE - Duration::from_hours(1);
        OpenOptions::new()
            .write(true)
            .open(&old_jsonl)
            .unwrap()
            .set_modified(ancient)
            .unwrap();
        // A fresh run plus sidecar that must survive the prune.
        let keep_jsonl = dir.join("jk-run-keep.jsonl");
        let keep_log = dir.join("jk-run-keep.docker-build.log");
        fs::write(&keep_jsonl, "{}").unwrap();
        fs::write(&keep_log, "keep").unwrap();

        prune_old_runs_in_dir(dir, None);

        assert!(!old_jsonl.exists(), "over-age run pruned");
        assert!(
            !old_log.exists(),
            "over-age run's sidecar must be pruned with it, not orphaned"
        );
        assert!(keep_jsonl.exists(), "fresh run kept");
        assert!(keep_log.exists(), "fresh run's sidecar kept");
    }
}
