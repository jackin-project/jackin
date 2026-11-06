// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::cell::Cell;
use std::error::Error;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use jackin_core::{EnvValue, HostPath, OutputLine, ProcessSpec, ProcessStdin, Program};
use tempfile::TempDir;
use tokio::sync::mpsc::Receiver;
use tokio::time::{sleep, timeout};

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) const WAIT_FOR: &str = "wait_for() { i=0; while [ ! -e \"$1\" ] && [ \"$i\" -lt 1000 ]; do i=$((i+1)); sleep 0.02; done; }\n";

pub(crate) const GROUP_ID: &str =
    "ps -o pgid= -p $$ | tr -d ' ' > \"$d/pgid.tmp\"\nmv \"$d/pgid.tmp\" \"$d/pgid\"\n";

pub(crate) const LOOP_DETACHED: &str = "d=$1\n( i=0; while [ \"$i\" -lt 600 ]; do echo x >> \"$d/beat\"; i=$((i+1)); sleep 0.05; done ) >/dev/null 2>&1 &\n";

pub(crate) const LOOP_HOLDING: &str = "d=$1\n( i=0; while [ \"$i\" -lt 600 ]; do echo x >> \"$d/beat\"; i=$((i+1)); sleep 0.05; done ) &\n";

const GUARD: Duration = Duration::from_secs(30);
const RUN_LIMIT: Duration = Duration::from_secs(10);
const HEAVY_GUARD: Duration = Duration::from_secs(120);
const HEAVY_RUN_LIMIT: Duration = Duration::from_secs(60);

pub(crate) async fn guarded<F>(future: F) -> TestResult
where
    F: Future<Output = TestResult>,
{
    bounded(GUARD, future).await
}

pub(crate) async fn guarded_heavy<F>(future: F) -> TestResult
where
    F: Future<Output = TestResult>,
{
    bounded(HEAVY_GUARD, future).await
}

async fn bounded<F>(limit: Duration, future: F) -> TestResult
where
    F: Future<Output = TestResult>,
{
    timeout(limit, future)
        .await
        .map_err(|_| format!("the test exceeded its {} s guard", limit.as_secs()))?
}

pub(crate) async fn next_line(
    rx: &mut Receiver<OutputLine>,
    what: &str,
) -> Result<OutputLine, Box<dyn Error>> {
    match timeout(Duration::from_secs(10), rx.recv()).await {
        Ok(Some(line)) => Ok(line),
        Ok(None) => Err(format!("the sink closed before {what}").into()),
        Err(_) => Err(format!("no line arrived within 10 s: {what}").into()),
    }
}

pub(crate) fn same_bytes(label: &str, actual: &[u8], expected: &[u8]) -> TestResult {
    if actual == expected {
        return Ok(());
    }
    let at = actual
        .iter()
        .zip(expected)
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| actual.len().min(expected.len()));
    Err(format!(
        "{label}: {} bytes against {} expected; first difference at index {at}",
        actual.len(),
        expected.len()
    )
    .into())
}

pub(crate) struct Scratch {
    _dir: TempDir,
    root: PathBuf,
    work: PathBuf,
    ceiling: PathBuf,
    scripts: Cell<usize>,
    run_limit: Duration,
}

impl Scratch {
    pub(crate) fn new() -> Result<Scratch, Box<dyn Error>> {
        Scratch::with_run_limit(RUN_LIMIT)
    }

    pub(crate) fn heavy() -> Result<Scratch, Box<dyn Error>> {
        Scratch::with_run_limit(HEAVY_RUN_LIMIT)
    }

    fn with_run_limit(run_limit: Duration) -> Result<Scratch, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().canonicalize()?;
        let work = root.join("work");
        fs::create_dir(&work)?;
        let ceiling = root
            .parent()
            .ok_or("the temp dir has no parent")?
            .to_owned();
        Ok(Scratch {
            _dir: dir,
            root,
            work,
            ceiling,
            scripts: Cell::new(0),
            run_limit,
        })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn work(&self) -> &Path {
        &self.work
    }

    pub(crate) fn spec(&self, body: &str, args: &[&str]) -> Result<ProcessSpec, Box<dyn Error>> {
        let index = self.scripts.get();
        self.scripts.set(index + 1);
        let script = self.root.join(format!("script-{index}.sh"));
        fs::write(&script, body)?;
        let root = text(&self.root)?;
        let mut argv = vec![
            "-c".to_owned(),
            "alias.t=!sh".to_owned(),
            "t".to_owned(),
            text(&script)?,
            root,
        ];
        argv.extend(args.iter().map(|arg| (*arg).to_owned()));
        Ok(ProcessSpec {
            program: Program::Git,
            args: argv,
            cwd: Some(HostPath::parse(&self.work)?),
            env: vec![(
                "GIT_CEILING_DIRECTORIES".to_owned(),
                EnvValue::Plain(text(&self.ceiling)?),
            )],
            env_remove_prefixes: vec!["GIT_".to_owned()],
            stdin: ProcessStdin::Null,
            timeout: self.run_limit,
        })
    }
}

pub(crate) fn text(path: &Path) -> Result<String, Box<dyn Error>> {
    Ok(path.to_str().ok_or("path is not UTF-8")?.to_owned())
}

fn beat_len(dir: &Path) -> Result<u64, Box<dyn Error>> {
    Ok(fs::metadata(dir.join("beat"))?.len())
}

pub(crate) async fn wait_for_beat(dir: &Path) -> TestResult {
    for _ in 0..500 {
        if beat_len(dir).is_ok_and(|len| len > 0) {
            return Ok(());
        }
        sleep(Duration::from_millis(20)).await;
    }
    Err("the background loop never wrote its first beat".into())
}

pub(crate) async fn assert_no_leftover(dir: &Path) -> TestResult {
    if beat_len(dir)? == 0 {
        return Err("the background loop never ran, so the probe proves nothing".into());
    }
    sleep(Duration::from_millis(300)).await;
    let first = beat_len(dir)?;
    sleep(Duration::from_millis(600)).await;
    let second = beat_len(dir)?;
    if first == second {
        Ok(())
    } else {
        Err(
            format!("a descendant still runs: the beat file grew from {first} to {second} bytes")
                .into(),
        )
    }
}
