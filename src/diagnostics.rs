use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
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
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
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

    pub fn compact(&self, kind: &str, message: &str) {
        self.write(kind, message, None, None, None);
    }

    pub fn stage(&self, kind: &str, stage: &str, message: &str, detail: Option<&str>) {
        // `span_id` is left unset: the `stage` field already identifies the
        // span, so repeating it as the span id is pure duplication.
        self.write(kind, message, Some(stage), detail, None);
    }

    pub fn debug(&self, category: &str, line: &str) {
        if self.debug {
            self.write("debug", line, None, Some(category), None);
        }
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
    active_run().is_some_and(|run| {
        run.debug(category, line);
        true
    })
}

pub fn active_run() -> Option<Arc<RunDiagnostics>> {
    active_slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

pub fn prune_old_runs(paths: &JackinPaths) {
    prune_old_runs_in_dir(&run_dir(paths), None);
}

pub fn prune_all_runs(paths: &JackinPaths) -> anyhow::Result<()> {
    let dir = run_dir(paths);
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

fn run_dir(paths: &JackinPaths) -> PathBuf {
    paths.data_dir.join(RUN_DIR)
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
            let _ = fs::remove_file(path);
        }
    }

    entries.retain(|(path, _)| path.exists());
    entries.sort_by_key(|(_, modified)| *modified);
    let overflow = entries.len().saturating_sub(MAX_RUN_ARTIFACTS);
    for (path, _) in entries.into_iter().take(overflow) {
        let _ = fs::remove_file(path);
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
        run.debug("cmd", "docker ps");

        let contents = fs::read_to_string(run.path()).unwrap();
        assert!(contents.contains("\"run_id\""));
        assert!(contents.contains("\"hello\""));
        assert!(contents.contains("\"debug\""));
    }
}
