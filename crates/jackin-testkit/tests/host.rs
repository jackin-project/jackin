// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::convert::Infallible;
use std::error::Error;
use std::future::Future;
use std::io::ErrorKind;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::task::{Context, Wake, Waker};
use std::time::Duration;

use jackin_core::{
    Clock, DirEntry, DockerSubcommand, EnvValue, FileKind, FileMode, FileSystem, FsError, HostPath,
    OutputLine, OutputStream, Poll, PollError, PollPlan, ProcessError, ProcessRunner, ProcessSpec,
    ProcessStatus, ProcessStdin, Program, RegularFile, RegularFileProblem, RepoPath, Secret,
    SecretBytes, StdFileSystem, Timestamp, UNBOUNDED_READ, poll_until, write_atomic,
};
use jackin_testkit::{
    CapturedFile, FakeClock, FsOp, HostSnapshot, HostState, MemFs, ProcessEffect, ProcessReply,
    RepoFixture, ScriptedProcessRunner, ScriptedProgram, SeenEnv, SeenStdin, TestRunner,
};
use tokio_util::sync::CancellationToken;

type TestResult = Result<(), Box<dyn Error>>;

fn host(path: &str) -> Result<HostPath, Box<dyn Error>> {
    Ok(HostPath::parse(Path::new(path))?)
}

fn child(root: &HostPath, name: &str) -> Result<HostPath, Box<dyn Error>> {
    Ok(HostPath::parse(&root.as_path().join(name))?)
}

fn entry(name: &str, kind: FileKind) -> DirEntry {
    DirEntry {
        name: name.to_owned(),
        kind,
    }
}

async fn names(fs: &dyn FileSystem, dir: &HostPath) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(fs.list(dir).await?.into_iter().map(|e| e.name).collect())
}

async fn fs_contract(fs: &dyn FileSystem, root: &HostPath) -> TestResult {
    fs.create_dir_all(root).await?;
    let file = child(root, "a.txt")?;
    assert_eq!(fs.read(&file).await?, None);
    assert_eq!(fs.stat(&file).await?, None);
    assert_eq!(fs.mode(&file).await?, None);
    fs.write_new(&file, b"hello", FileMode(0o600)).await?;
    assert_eq!(fs.read(&file).await?, Some(b"hello".to_vec()));
    assert_eq!(fs.mode(&file).await?, Some(FileMode(0o600)));
    assert!(matches!(
        fs.write_new(&file, b"other", FileMode(0o600)).await,
        Err(FsError::AlreadyExists(_))
    ));
    assert_eq!(fs.read(&file).await?, Some(b"hello".to_vec()));
    let orphan = child(&child(root, "missing")?, "x")?;
    assert!(matches!(
        fs.write_new(&orphan, b"x", FileMode(0o600)).await,
        Err(FsError::Io {
            kind: ErrorKind::NotFound,
            ..
        })
    ));
    let sub = child(root, "sub/nested")?;
    fs.create_dir_all(&sub).await?;
    assert_eq!(fs.stat(&sub).await?, Some(FileKind::Dir));
    let exec = child(root, "b.txt")?;
    fs.write_new(&exec, b"new", FileMode(0o755)).await?;
    assert_eq!(fs.mode(&exec).await?, Some(FileMode(0o755)));
    assert_eq!(
        fs.list(root).await?,
        [
            entry("a.txt", FileKind::File),
            entry("b.txt", FileKind::File),
            entry("sub", FileKind::Dir),
        ]
    );
    fs.rename(&exec, &file).await?;
    assert_eq!(fs.read(&file).await?, Some(b"new".to_vec()));
    assert_eq!(fs.read(&exec).await?, None);
    assert!(matches!(
        fs.rename(&exec, &file).await,
        Err(FsError::Io { .. })
    ));
    fs.remove_file(&exec).await?;
    fs.remove_file(&file).await?;
    assert_eq!(fs.stat(&file).await?, None);
    let deep = child(&sub, "deep.txt")?;
    fs.write_new(&deep, b"deep", FileMode(0o644)).await?;
    fs.remove_dir_all(&child(root, "sub")?).await?;
    assert_eq!(names(fs, root).await?, Vec::<String>::new());
    assert_eq!(fs.read(&deep).await?, None);
    fs.remove_dir_all(&child(root, "sub")?).await?;
    let lock = child(root, "config.lock")?;
    let held = fs.lock(&lock).await?;
    assert!(matches!(fs.lock(&lock).await, Err(FsError::LockHeld(path)) if path == lock));
    drop(held);
    fs.lock(&lock).await?;
    fs_boundary_extras(fs, root).await
}

async fn fs_boundary_extras(fs: &dyn FileSystem, root: &HostPath) -> TestResult {
    let size = child(root, "size.bin")?;
    assert_eq!(fs.file_len(&size).await?, None);
    fs.write_new(&size, b"12345", FileMode(0o600)).await?;
    assert_eq!(fs.file_len(&size).await?, Some(5));
    assert_eq!(fs.stat_following(&size).await?, Some(FileKind::File));
    assert_eq!(fs.stat_following(root).await?, Some(FileKind::Dir));
    assert_eq!(fs.stat_following(&child(root, "none")?).await?, None);
    let first = fs.identity(&size).await?;
    assert!(first.is_some());
    assert_eq!(fs.identity(&size).await?, first);
    assert_eq!(fs.identity(&child(root, "none")?).await?, None);
    let sibling = child(root, "sibling.bin")?;
    fs.write_new(&sibling, b"1", FileMode(0o600)).await?;
    assert_ne!(fs.identity(&sibling).await?, first);
    assert_ne!(fs.identity(root).await?, first);
    let moved = child(root, "moved.bin")?;
    fs.rename(&size, &moved).await?;
    assert_eq!(fs.identity(&size).await?, None);
    assert_eq!(fs.identity(&moved).await?, first);
    fs.remove_file(&sibling).await?;
    assert_eq!(fs.identity(&sibling).await?, None);
    fs.remove_file(&moved).await?;
    let private = child(root, "priv/inner")?;
    fs.create_private_dir_all(&private).await?;
    assert_eq!(fs.stat(&private).await?, Some(FileKind::Dir));
    assert_eq!(fs.mode(&private).await?, Some(FileMode(0o700)));
    assert_eq!(fs.mode(&child(root, "priv")?).await?, Some(FileMode(0o700)));
    fs.remove_dir_all(&child(root, "priv")?).await?;
    let wide = child(root, "wide")?;
    fs.create_dir_all(&wide).await?;
    fs.create_private_dir_all(&wide).await?;
    assert_eq!(fs.mode(&wide).await?, Some(FileMode(0o700)));
    fs.remove_dir_all(&wide).await?;
    let ranged = child(root, "range.bin")?;
    fs.write_new(&ranged, b"0123456789", FileMode(0o600))
        .await?;
    for (offset, len, expected) in [
        (3, 4, "3456"),
        (0, 100, "0123456789"),
        (10, 5, ""),
        (11, 5, ""),
        (4, 0, ""),
        (9, usize::MAX, "9"),
    ] {
        assert_eq!(
            fs.read_at(&ranged, offset, len).await?,
            expected.as_bytes(),
            "read_at({offset}, {len})"
        );
    }
    assert!(matches!(
        fs.read_at(&child(root, "none.bin")?, 0, 1).await,
        Err(FsError::Io {
            kind: ErrorKind::NotFound,
            ..
        })
    ));
    assert!(matches!(
        fs.read_at(root, 0, 1).await,
        Err(FsError::Io { .. })
    ));
    fs.remove_file(&ranged).await?;
    Ok(())
}

#[tokio::test]
async fn memfs_and_stdfilesystem_pass_the_same_filesystem_contract() -> TestResult {
    let dir = tempfile::tempdir()?;
    fs_contract(&StdFileSystem::default(), &HostPath::parse(dir.path())?).await?;
    let mem = MemFs::new();
    fs_contract(&mem, &host("/mem/root")?).await?;
    let note = host("/mem/note.txt")?;
    mem.write_new(&note, "text".as_bytes(), FileMode(0o600))
        .await?;
    assert_eq!(mem.read_string(&note), Some("text".to_owned()));
    assert_eq!(mem.read_string(&host("/mem/none")?), None);
    Ok(())
}

#[tokio::test]
async fn memfs_read_regular_returns_bytes_and_mode_and_gives_the_typed_refusal_otherwise()
-> TestResult {
    let fs = MemFs::new();
    let root = host("/mem/tree")?;
    fs.create_dir_all(&child(&root, "dir")?).await?;
    fs.write_new(&child(&root, "run.sh")?, b"#!/bin/sh\n", FileMode(0o755))
        .await?;
    fs.write_new(
        &child(&child(&root, "dir")?, "inner.txt")?,
        b"inner",
        FileMode(0o644),
    )
    .await?;
    let read = |name: &str| {
        let fs = &fs;
        let root = &root;
        let file = RepoPath::parse(name);
        async move {
            fs.read_regular(root, &file?, UNBOUNDED_READ)
                .await
                .map_err(Box::<dyn Error>::from)
        }
    };
    assert_eq!(
        read("run.sh").await?,
        Ok(RegularFile {
            bytes: b"#!/bin/sh\n".to_vec(),
            mode: FileMode(0o755),
        })
    );
    assert_eq!(
        read("dir/inner.txt").await?,
        Ok(RegularFile {
            bytes: b"inner".to_vec(),
            mode: FileMode(0o644),
        })
    );
    let capped = fs
        .read_regular(&root, &RepoPath::parse("dir/inner.txt")?, 2)
        .await?
        .map_err(|problem| format!("{problem:?}"))?;
    assert_eq!(capped.bytes, b"inn");
    assert_eq!(read("dir").await?, Err(RegularFileProblem::NotARegularFile));
    assert_eq!(read("gone.txt").await?, Err(RegularFileProblem::Missing));
    assert_eq!(
        read("gone/deeper.txt").await?,
        Err(RegularFileProblem::Missing)
    );
    assert_eq!(
        read("run.sh/below").await?,
        Err(RegularFileProblem::Missing)
    );
    Ok(())
}

const OPS: [FsOp; 16] = [
    FsOp::Read,
    FsOp::Stat,
    FsOp::StatFollowing,
    FsOp::FileLen,
    FsOp::Identity,
    FsOp::List,
    FsOp::CreateDirAll,
    FsOp::CreatePrivateDirAll,
    FsOp::WriteNew,
    FsOp::Rename,
    FsOp::RemoveFile,
    FsOp::RemoveDirAll,
    FsOp::Mode,
    FsOp::SyncDir,
    FsOp::Lock,
    FsOp::ReadAt,
];

async fn invoke(fs: &MemFs, op: FsOp, root: &HostPath, round: u32) -> Result<(), FsError> {
    let file = HostPath::parse(&root.as_path().join("f")).map_err(|e| FsError::Io {
        path: root.clone(),
        kind: ErrorKind::InvalidInput,
        message: e.to_string(),
    })?;
    let absent = HostPath::parse(&root.as_path().join("g")).map_err(|e| FsError::Io {
        path: root.clone(),
        kind: ErrorKind::InvalidInput,
        message: e.to_string(),
    })?;
    match op {
        FsOp::Read => fs.read(&file).await.map(drop),
        FsOp::Stat => fs.stat(&file).await.map(drop),
        FsOp::StatFollowing => fs.stat_following(&file).await.map(drop),
        FsOp::FileLen => fs.file_len(&file).await.map(drop),
        FsOp::Identity => fs.identity(&file).await.map(drop),
        FsOp::List => fs.list(root).await.map(drop),
        FsOp::CreateDirAll => fs.create_dir_all(root).await,
        FsOp::CreatePrivateDirAll => fs.create_private_dir_all(root).await,
        FsOp::WriteNew => {
            let fresh =
                HostPath::parse(&root.as_path().join(format!("new{round}"))).map_err(|e| {
                    FsError::Io {
                        path: root.clone(),
                        kind: ErrorKind::InvalidInput,
                        message: e.to_string(),
                    }
                })?;
            fs.write_new(&fresh, b"x", FileMode(0o600)).await
        }
        FsOp::Rename => fs.rename(&file, &file).await,
        FsOp::RemoveFile => fs.remove_file(&absent).await,
        FsOp::RemoveDirAll => fs.remove_dir_all(&absent).await,
        FsOp::Mode => fs.mode(&file).await.map(drop),
        FsOp::SyncDir => fs.sync_dir(root).await,
        FsOp::Lock => fs.lock(&file).await.map(drop),
        FsOp::ReadAt => fs.read_at(&file, 0, 1).await.map(drop),
    }
}

#[tokio::test]
async fn fail_at_fails_the_nth_call_of_each_operation_once() -> TestResult {
    let root = host("/mem")?;
    for op in OPS {
        let fs = MemFs::new();
        fs.create_dir_all(&root).await?;
        fs.write_new(&child(&root, "f")?, b"data", FileMode(0o600))
            .await?;
        fs.fail_at(
            op,
            2,
            FsError::Io {
                path: root.clone(),
                kind: ErrorKind::StorageFull,
                message: "injected".to_owned(),
            },
        );
        invoke(&fs, op, &root, 1).await?;
        for other in OPS.into_iter().filter(|other| *other != op) {
            invoke(&fs, other, &root, 1).await?;
        }
        let failed = invoke(&fs, op, &root, 2).await;
        assert!(
            matches!(
                failed,
                Err(FsError::Io {
                    kind: ErrorKind::StorageFull,
                    ..
                })
            ),
            "{op:?}: {failed:?}"
        );
        invoke(&fs, op, &root, 3).await?;
    }
    Ok(())
}

fn injected(root: &HostPath) -> FsError {
    FsError::Io {
        path: root.clone(),
        kind: ErrorKind::StorageFull,
        message: "injected".to_owned(),
    }
}

async fn crash(faults: &[(FsOp, u32)], expect_new: bool, expect_temp: bool) -> TestResult {
    let root = host("/mem")?;
    let fs = MemFs::new();
    fs.create_dir_all(&root).await?;
    let target = child(&root, "f")?;
    fs.write_new(&target, b"old", FileMode(0o600)).await?;
    for (op, nth) in faults {
        fs.fail_at(*op, *nth, injected(&root));
    }
    let result = write_atomic(&fs, &target, b"new", FileMode(0o600)).await;
    assert!(result.is_err());
    let expected: &[u8] = if expect_new { b"new" } else { b"old" };
    assert_eq!(fs.read(&target).await?.as_deref(), Some(expected));
    let listing = names(&fs, &root).await?;
    let temps = |listing: &[String]| {
        listing
            .iter()
            .filter(|name| name.starts_with(".f.") && name.rsplit('.').next() == Some("tmp"))
            .count()
    };
    assert_eq!(temps(&listing), usize::from(expect_temp), "{listing:?}");
    if expect_temp {
        write_atomic(&fs, &target, b"new", FileMode(0o600)).await?;
        assert_eq!(fs.read(&target).await?.as_deref(), Some(&b"new"[..]));
        let after = names(&fs, &root).await?;
        assert_eq!(temps(&after), 1, "{after:?}");
    }
    Ok(())
}

#[tokio::test]
async fn write_atomic_crash_at_every_step_keeps_the_old_file_or_the_complete_new_one() -> TestResult
{
    crash(&[(FsOp::WriteNew, 1)], false, false).await?;
    crash(&[(FsOp::Rename, 1)], false, false).await?;
    crash(&[(FsOp::Rename, 1), (FsOp::RemoveFile, 2)], false, true).await?;
    crash(&[(FsOp::SyncDir, 1)], true, false).await
}

#[derive(Default)]
struct Wakes(AtomicUsize);

impl Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

type Sleeper = Pin<Box<dyn Future<Output = ()> + Send>>;

fn sleeper(clock: &Arc<FakeClock>) -> Sleeper {
    let clock = Arc::clone(clock);
    Box::pin(async move { clock.sleep(Duration::from_secs(10)).await })
}

fn poll_n(future: &mut Sleeper, waker: &Waker, polls: usize) -> Option<usize> {
    let mut cx = Context::from_waker(waker);
    (1..=polls).find(|_| future.as_mut().poll(&mut cx).is_ready())
}

fn polls_to_wake_unheld() -> Option<usize> {
    let clock = Arc::new(FakeClock::new());
    let waker = Waker::from(Arc::new(Wakes::default()));
    poll_n(&mut sleeper(&clock), &waker, 10_000)
}

#[tokio::test]
async fn a_guard_taken_one_poll_before_a_sleep_wakes_holds_time_until_it_drops() {
    let wake_at = polls_to_wake_unheld().expect("an unheld sleep wakes");
    let clock = Arc::new(FakeClock::new());
    let start = clock.now();
    let waker = Waker::from(Arc::new(Wakes::default()));
    let mut sleeping = sleeper(&clock);
    assert_eq!(
        poll_n(&mut sleeping, &waker, wake_at - 1),
        None,
        "not yet awake"
    );
    let guard = clock.working();
    assert_eq!(
        poll_n(&mut sleeping, &waker, 1_000),
        None,
        "woke under a guard"
    );
    assert_eq!(clock.now(), start, "time moved under a guard");
    drop(guard);
    assert!(
        poll_n(&mut sleeping, &waker, 2 * wake_at).is_some(),
        "never woke"
    );
    assert_eq!(clock.now(), Timestamp(start.0 + 10));
}

#[tokio::test]
async fn a_sleep_begun_under_a_guard_stays_asleep_until_the_guard_drops() {
    let wake_at = polls_to_wake_unheld().expect("an unheld sleep wakes");
    let clock = Arc::new(FakeClock::new());
    let start = clock.now();
    let waker = Waker::from(Arc::new(Wakes::default()));
    let guard = clock.working();
    let mut sleeping = sleeper(&clock);
    assert_eq!(
        poll_n(&mut sleeping, &waker, 1_000),
        None,
        "woke under a guard"
    );
    assert_eq!(clock.now(), start, "time moved under a guard");
    drop(guard);
    assert!(
        poll_n(&mut sleeping, &waker, 2 * wake_at).is_some(),
        "never woke"
    );
    assert_eq!(clock.now(), Timestamp(start.0 + 10));
}

#[tokio::test]
async fn a_sleep_waits_for_the_last_of_two_guards() {
    let wake_at = polls_to_wake_unheld().expect("an unheld sleep wakes");
    let clock = Arc::new(FakeClock::new());
    let waker = Waker::from(Arc::new(Wakes::default()));
    let first = clock.working();
    let second = clock.working();
    let mut sleeping = sleeper(&clock);
    assert_eq!(
        poll_n(&mut sleeping, &waker, 500),
        None,
        "woke under two guards"
    );
    drop(first);
    assert_eq!(
        poll_n(&mut sleeping, &waker, 1_000),
        None,
        "woke under one guard"
    );
    drop(second);
    assert!(
        poll_n(&mut sleeping, &waker, 2 * wake_at).is_some(),
        "never woke"
    );
}

#[tokio::test]
async fn a_sleep_interrupted_by_a_guard_counts_its_quiet_yields_again_from_zero() {
    let wake_at = polls_to_wake_unheld().expect("an unheld sleep wakes");
    let clock = Arc::new(FakeClock::new());
    let waker = Waker::from(Arc::new(Wakes::default()));
    let mut sleeping = sleeper(&clock);
    assert_eq!(
        poll_n(&mut sleeping, &waker, wake_at - 20),
        None,
        "not yet awake"
    );
    let guard = clock.working();
    assert_eq!(
        poll_n(&mut sleeping, &waker, 100),
        None,
        "woke under a guard"
    );
    drop(guard);
    assert_eq!(
        poll_n(&mut sleeping, &waker, 2 * wake_at),
        Some(wake_at),
        "the quiet yields before the guard still counted"
    );
}

#[tokio::test]
async fn a_sleep_blocked_by_a_guard_does_not_wake_itself_and_wakes_when_the_guard_drops() {
    let wake_at = polls_to_wake_unheld().expect("an unheld sleep wakes");
    let clock = Arc::new(FakeClock::new());
    let wakes = Arc::new(Wakes::default());
    let waker = Waker::from(Arc::clone(&wakes));
    let guard = clock.working();
    let mut sleeping = sleeper(&clock);
    assert_eq!(poll_n(&mut sleeping, &waker, 1), None);
    let settled = wakes.0.load(Ordering::SeqCst);
    assert_eq!(poll_n(&mut sleeping, &waker, 100), None);
    tokio::task::yield_now().await;
    assert_eq!(
        wakes.0.load(Ordering::SeqCst),
        settled,
        "a blocked sleep woke its own task"
    );
    drop(guard);
    assert!(
        wakes.0.load(Ordering::SeqCst) > settled,
        "dropping the last guard did not wake the sleep"
    );
    assert!(
        poll_n(&mut sleeping, &waker, 2 * wake_at).is_some(),
        "never woke"
    );
}

#[tokio::test]
async fn fake_clock_sleep_advances_time_records_each_sleep_and_yields() -> TestResult {
    let started = std::time::Instant::now();
    let clock = FakeClock::new();
    let start = clock.now();
    clock.advance(Duration::from_secs(90));
    assert_eq!(clock.now(), Timestamp(start.0 + 90));
    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    tokio::spawn(async move { flag.store(true, Ordering::SeqCst) });
    clock.sleep(Duration::from_secs(10)).await;
    assert!(ran.load(Ordering::SeqCst));
    assert_eq!(clock.now(), Timestamp(start.0 + 100));
    assert_eq!(clock.slept(), [Duration::from_secs(10)]);

    let plan = PollPlan {
        interval: Duration::from_secs(1),
        timeout: Duration::from_secs(10),
    };
    let never = FakeClock::new();
    let pending = poll_until(&never, plan, &CancellationToken::new(), || async {
        Ok::<_, Infallible>(Poll::<()>::Pending)
    })
    .await;
    assert!(matches!(
        pending,
        Err(PollError::Timeout { after }) if after == Duration::from_secs(10)
    ));
    assert_eq!(
        never.slept().iter().sum::<Duration>(),
        Duration::from_secs(10)
    );

    let fourth = FakeClock::new();
    let calls = AtomicU32::new(0);
    let ready = poll_until(&fourth, plan, &CancellationToken::new(), || {
        let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
        async move {
            Ok::<_, Infallible>(if count == 4 {
                Poll::Ready("done")
            } else {
                Poll::Pending
            })
        }
    })
    .await;
    assert!(matches!(ready, Ok("done")));
    assert_eq!(fourth.slept(), [Duration::from_secs(1); 3]);
    assert!(started.elapsed() < Duration::from_secs(5));
    Ok(())
}

fn spec(program: Program, args: &[&str]) -> ProcessSpec {
    ProcessSpec {
        program,
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        cwd: None,
        env: Vec::new(),
        env_remove_prefixes: Vec::new(),
        stdin: ProcessStdin::Null,
        timeout: Duration::from_secs(5),
    }
}

async fn stdout(runner: &dyn ProcessRunner, spec: &ProcessSpec) -> Result<String, ProcessError> {
    let output = runner.run(spec, None, &CancellationToken::new()).await?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[tokio::test]
async fn scripted_runner_answers_in_order_records_every_spec_and_returns_each_reply_form()
-> TestResult {
    let runner = ScriptedProcessRunner::new();
    runner.expect(
        ScriptedProgram::Openshell,
        &["gateway", "list"],
        ProcessReply::exited(0, b"A", b""),
    );
    runner.expect(
        ScriptedProgram::Openshell,
        &["gateway", "list"],
        ProcessReply::exited(0, b"B", b""),
    );
    runner.expect(
        ScriptedProgram::Op,
        &["read"],
        ProcessReply::exited(0, b"op", b""),
    );
    runner.expect(
        ScriptedProgram::Ssh,
        &[],
        ProcessReply::exited(0, b"ssh", b""),
    );
    runner.expect_docker(
        DockerSubcommand::ImageInspect,
        &["--format"],
        ProcessReply::exited(0, b"inspect", b""),
    );
    runner.expect(
        ScriptedProgram::Openshell,
        &["timeout"],
        ProcessReply::timeout(),
    );
    runner.expect(
        ScriptedProgram::Openshell,
        &["cancel"],
        ProcessReply::cancelled(),
    );
    runner.expect(
        ScriptedProgram::Openshell,
        &["spawn"],
        ProcessReply::spawn("no binary"),
    );
    let call = |program, args: &[&str]| spec(program, args);
    let mut answers = Vec::new();
    for _ in 0..3 {
        answers.push(
            stdout(
                &runner,
                &call(Program::Openshell, &["gateway", "list", "-o", "json"]),
            )
            .await?,
        );
    }
    assert_eq!(answers, ["A", "B", "B"]);
    assert_eq!(
        stdout(&runner, &call(Program::Op, &["read", "x"])).await?,
        "op"
    );
    assert_eq!(
        stdout(&runner, &call(Program::Ssh, &["host"])).await?,
        "ssh"
    );
    let inspect = call(
        Program::Docker(DockerSubcommand::ImageInspect),
        &["--format", "x"],
    );
    assert_eq!(stdout(&runner, &inspect).await?, "inspect");
    let push = call(Program::Docker(DockerSubcommand::Push), &["--format", "x"]);
    assert!(matches!(
        stdout(&runner, &push).await,
        Err(ProcessError::Spawn { .. })
    ));
    assert!(matches!(
        stdout(&runner, &call(Program::Openshell, &["timeout"])).await,
        Err(ProcessError::Timeout { program: Program::Openshell, after, group: None }) if after == Duration::from_secs(5)
    ));
    assert!(matches!(
        stdout(&runner, &call(Program::Openshell, &["cancel"])).await,
        Err(ProcessError::Cancelled {
            program: Program::Openshell,
            group: None
        })
    ));
    assert!(matches!(
        stdout(&runner, &call(Program::Openshell, &["spawn"])).await,
        Err(ProcessError::Spawn { program: Program::Openshell, ref message }) if message == "no binary"
    ));
    let seen = runner.seen();
    assert_eq!(seen.len(), 10);
    assert_eq!(seen[0].program, Program::Openshell);
    assert_eq!(seen[0].args, ["gateway", "list", "-o", "json"]);
    assert_eq!(seen[0].cwd, None);
    assert_eq!(seen[0].stdin, SeenStdin::Null);
    assert_eq!(seen[3].args, ["read", "x"]);
    assert_eq!(
        seen[5].program,
        Program::Docker(DockerSubcommand::ImageInspect)
    );
    Ok(())
}

#[tokio::test]
async fn scripted_runner_rejects_git_and_unscripted_programs_naming_them() -> TestResult {
    let scripted = ScriptedProcessRunner::new();
    for program in [
        ScriptedProgram::Ssh,
        ScriptedProgram::Op,
        ScriptedProgram::Docker,
        ScriptedProgram::Openshell,
    ] {
        scripted.expect(program, &[], ProcessReply::exited(0, b"", b""));
    }
    let git = scripted
        .run(
            &spec(Program::Git, &["status"]),
            None,
            &CancellationToken::new(),
        )
        .await;
    assert!(matches!(
        git,
        Err(ProcessError::Spawn { program: Program::Git, ref message }) if message.contains("git")
    ));
    assert_eq!(scripted.seen().len(), 1);
    assert_eq!(scripted.seen()[0].program, Program::Git);

    let empty = ScriptedProcessRunner::new();
    for program in [
        Program::Docker(DockerSubcommand::Push),
        Program::Ssh,
        Program::Op,
        Program::Openshell,
    ] {
        let result = empty
            .run(
                &spec(program, &["unknown"]),
                None,
                &CancellationToken::new(),
            )
            .await;
        assert!(
            matches!(&result, Err(ProcessError::Spawn { program: p, message }) if *p == program && message.contains(program.binary())),
            "{result:?}"
        );
    }
    empty.expect(
        ScriptedProgram::Openshell,
        &["known"],
        ProcessReply::exited(0, b"", b""),
    );
    let mismatch = empty
        .run(
            &spec(Program::Openshell, &["other"]),
            None,
            &CancellationToken::new(),
        )
        .await;
    assert!(matches!(
        mismatch,
        Err(ProcessError::Spawn {
            program: Program::Openshell,
            ..
        })
    ));
    assert_eq!(empty.seen().len(), 5);
    let variants = |program: ScriptedProgram| match program {
        ScriptedProgram::Docker => 0,
        ScriptedProgram::Openshell => 1,
        ScriptedProgram::Ssh => 2,
        ScriptedProgram::Op => 3,
        ScriptedProgram::Security => 4,
    };
    assert_eq!(variants(ScriptedProgram::Op), 3);
    Ok(())
}

#[tokio::test]
async fn test_runner_runs_git_for_real_and_scripts_the_other_four() -> TestResult {
    let dir = tempfile::tempdir()?;
    let runner = TestRunner::new();
    let mut init = spec(Program::Git, &["init", "-q"]);
    init.cwd = Some(HostPath::parse(dir.path())?);
    let cancel = CancellationToken::new();
    assert_eq!(
        runner.run(&init, None, &cancel).await?.status,
        ProcessStatus::Exited(0)
    );
    let mut parse = spec(Program::Git, &["rev-parse", "--git-dir"]);
    parse.cwd = init.cwd.clone();
    let output = runner.run(&parse, None, &cancel).await?;
    assert_eq!(output.status, ProcessStatus::Exited(0));
    assert_eq!(output.stdout, b".git\n");
    assert!(runner.scripted().seen().is_empty());
    runner.scripted().expect(
        ScriptedProgram::Openshell,
        &[],
        ProcessReply::exited(0, b"scripted", b""),
    );
    assert_eq!(
        runner
            .run(&spec(Program::Openshell, &["x"]), None, &cancel)
            .await?
            .stdout,
        b"scripted"
    );
    let unscripted = runner
        .run(&spec(Program::Op, &["read"]), None, &cancel)
        .await;
    assert!(matches!(
        unscripted,
        Err(ProcessError::Spawn { program: Program::Op, ref message }) if message.contains("op")
    ));
    Ok(())
}

#[tokio::test]
async fn a_scripted_git_version_answers_only_git_version_and_any_other_git_call_runs_for_real()
-> TestResult {
    const SCRIPTED: &str = "git version 0.0.1-scripted\n";
    let dir = tempfile::tempdir()?;
    let runner = TestRunner::new();
    let cancel = CancellationToken::new();
    let mut init = spec(Program::Git, &["init", "-q"]);
    init.cwd = Some(HostPath::parse(dir.path())?);
    runner.run(&init, None, &cancel).await?;
    let version = spec(Program::Git, &["--version"]);
    let mut parse = spec(Program::Git, &["rev-parse", "--git-dir"]);
    parse.cwd = init.cwd.clone();

    let real = stdout(&runner, &version).await?;
    assert!(real.starts_with("git version "), "{real}");
    assert_ne!(real, SCRIPTED);

    runner
        .scripted()
        .expect_git_version(ProcessReply::exited(0, SCRIPTED.as_bytes(), b""));
    assert_eq!(stdout(&runner, &version).await?, SCRIPTED);
    assert_eq!(stdout(&runner, &parse).await?, ".git\n");
    let seen: Vec<Vec<String>> = runner
        .scripted()
        .seen()
        .into_iter()
        .map(|call| call.args)
        .collect();
    assert_eq!(seen, [["--version"]]);
    Ok(())
}

fn lines(items: &[(OutputStream, &str)]) -> Vec<(OutputStream, String)> {
    items.iter().map(|(s, t)| (*s, (*t).to_owned())).collect()
}

#[tokio::test]
async fn the_captured_context_records_links_as_links_and_never_follows_them() -> TestResult {
    let dir = tempfile::tempdir()?;
    let context = dir.path().join("ctx");
    tokio::fs::create_dir_all(context.join("real")).await?;
    tokio::fs::write(context.join("file.txt"), b"alpha").await?;
    tokio::fs::write(context.join("real/f.txt"), b"inside").await?;
    for (name, target) in [
        ("to_file", "file.txt"),
        ("dangling", "nowhere.txt"),
        ("ldir", "real"),
    ] {
        tokio::fs::symlink(target, context.join(name)).await?;
    }
    let runner = ScriptedProcessRunner::new();
    runner.expect(
        ScriptedProgram::Docker,
        &["build"],
        ProcessReply::exited(0, b"", b"").with_effect(ProcessEffect::CaptureContext),
    );
    let build = spec(
        Program::Docker(DockerSubcommand::BuildxBuild),
        &["build", &context.display().to_string()],
    );
    runner.run(&build, None, &CancellationToken::new()).await?;
    let captured = runner.captured_contexts();
    assert_eq!(captured.len(), 1);
    let mut expected = std::collections::BTreeMap::new();
    expected.insert("file.txt".to_owned(), CapturedFile::file(b"alpha", 0o644));
    expected.insert(
        "real/f.txt".to_owned(),
        CapturedFile::file(b"inside", 0o644),
    );
    expected.insert("to_file".to_owned(), CapturedFile::link("file.txt"));
    expected.insert("dangling".to_owned(), CapturedFile::link("nowhere.txt"));
    expected.insert("ldir".to_owned(), CapturedFile::link("real"));
    assert_eq!(captured[0].files, expected);
    Ok(())
}

#[tokio::test]
async fn scripted_runner_streams_lines_applies_effects_and_holds_until_cancel() -> TestResult {
    let dir = tempfile::tempdir()?;
    let context = dir.path().join("ctx");
    tokio::fs::create_dir_all(context.join("nested")).await?;
    tokio::fs::write(context.join("a.txt"), b"alpha").await?;
    tokio::fs::write(context.join("nested/run.sh"), b"#!/bin/sh\n").await?;
    let mut perms = tokio::fs::metadata(context.join("nested/run.sh"))
        .await?
        .permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    tokio::fs::set_permissions(context.join("nested/run.sh"), perms).await?;
    let iidfile = dir.path().join("iid");

    let runner = ScriptedProcessRunner::new();
    let streamed = lines(&[
        (OutputStream::Stdout, "one"),
        (OutputStream::Stdout, "two"),
        (OutputStream::Stderr, "three"),
        (OutputStream::Stdout, "four"),
    ]);
    runner.expect(
        ScriptedProgram::Docker,
        &["build"],
        ProcessReply::lines(ProcessStatus::Exited(0), streamed)
            .with_effect(ProcessEffect::WriteArgFile {
                prefix: "--iidfile=".to_owned(),
                bytes: b"sha256:abc\n".to_vec(),
            })
            .with_effect(ProcessEffect::CaptureContext),
    );
    let mut build = spec(
        Program::Docker(DockerSubcommand::BuildxBuild),
        &[
            "build",
            &format!("--iidfile={}", iidfile.display()),
            &context.display().to_string(),
        ],
    );
    build.env = vec![
        ("PLAIN".to_owned(), EnvValue::Plain("v".to_owned())),
        (
            "TOKEN".to_owned(),
            EnvValue::Secret(Secret::new("s3cret".to_owned())),
        ),
    ];
    build.env_remove_prefixes = vec!["OPENSHELL_".to_owned()];
    build.stdin = ProcessStdin::Bytes(SecretBytes::new(b"FROM scratch\n".to_vec()));
    let (sink, mut received) = tokio::sync::mpsc::channel::<OutputLine>(16);
    let output = runner
        .run(&build, Some(&sink), &CancellationToken::new())
        .await?;
    let mut order = Vec::new();
    while let Ok(line) = received.try_recv() {
        order.push((line.stream, line.text));
    }
    assert_eq!(
        order,
        lines(&[
            (OutputStream::Stdout, "one"),
            (OutputStream::Stdout, "two"),
            (OutputStream::Stderr, "three"),
            (OutputStream::Stdout, "four")
        ])
    );
    assert_eq!(output.stdout, b"one\ntwo\nfour\n");
    assert_eq!(output.stderr, b"three\n");
    assert_eq!(tokio::fs::read(&iidfile).await?, b"sha256:abc\n");
    let captured = runner.captured_contexts();
    assert_eq!(captured.len(), 1);
    assert!(matches!(
        &captured[0].files["a.txt"],
        CapturedFile::File { bytes, .. } if bytes == b"alpha"
    ));
    assert!(matches!(
        &captured[0].files["nested/run.sh"],
        CapturedFile::File {
            mode: FileMode(0o755),
            ..
        }
    ));
    assert_eq!(captured[0].files.len(), 2);
    let seen = runner.seen();
    assert_eq!(seen[0].stdin, SeenStdin::Bytes(b"FROM scratch\n".to_vec()));
    assert_eq!(seen[0].timeout, Duration::from_secs(5));
    assert_eq!(seen[0].env_remove_prefixes, ["OPENSHELL_"]);
    assert_eq!(
        seen[0].env,
        [
            ("PLAIN".to_owned(), SeenEnv::Plain("v".to_owned())),
            ("TOKEN".to_owned(), SeenEnv::Secret)
        ]
    );
    assert!(!format!("{seen:?}").contains("s3cret"));

    runner.expect_fn(ScriptedProgram::Op, &["read"], |call| {
        ProcessReply::exited(0, call.args.join("+").as_bytes(), b"")
    });
    assert_eq!(
        stdout(&runner, &spec(Program::Op, &["read", "ref"])).await?,
        "read+ref"
    );

    runner.expect(
        ScriptedProgram::Ssh,
        &[],
        ProcessReply::until_cancelled(lines(&[(OutputStream::Stdout, "started")])),
    );
    let cancel = CancellationToken::new();
    let (sink, mut received) = tokio::sync::mpsc::channel::<OutputLine>(4);
    let held = spec(Program::Ssh, &["host"]);
    let mut pending = Box::pin(runner.run(&held, Some(&sink), &cancel));
    let early = tokio::time::timeout(Duration::from_millis(50), &mut pending).await;
    assert!(early.is_err(), "returned before the token fired");
    assert_eq!(received.try_recv()?.text, "started");
    cancel.cancel();
    assert!(matches!(
        pending.await,
        Err(ProcessError::Cancelled {
            program: Program::Ssh,
            group: None
        })
    ));
    Ok(())
}

#[tokio::test]
async fn concurrent_sleepers_wake_at_their_own_deadlines_and_do_not_add_their_durations()
-> TestResult {
    let clock = FakeClock::new();
    let start = clock.now();
    let five = Duration::from_secs(5);
    tokio::join!(clock.sleep(five), clock.sleep(five));
    assert_eq!(clock.now(), Timestamp(start.0 + 5));
    assert_eq!(clock.slept(), [five, five]);

    let clock = FakeClock::new();
    let start = clock.now();
    let ((), seen) = tokio::join!(clock.sleep(five), async {
        clock.sleep(Duration::from_secs(3)).await;
        clock.now()
    });
    assert_eq!(seen, Timestamp(start.0 + 3));
    assert_eq!(clock.now(), Timestamp(start.0 + 5));
    let mut slept = clock.slept();
    slept.sort();
    assert_eq!(slept, [Duration::from_secs(3), five]);
    Ok(())
}

#[tokio::test]
async fn a_dropped_sleeper_leaves_no_trace_and_a_lone_race_still_moves_time() -> TestResult {
    let clock = FakeClock::new();
    let start = clock.now();
    let plan = PollPlan {
        interval: Duration::from_secs(1),
        timeout: Duration::from_secs(30),
    };
    let yields = AtomicU32::new(0);
    let ready = poll_until(&clock, plan, &CancellationToken::new(), || async {
        if yields.fetch_add(1, Ordering::SeqCst) == 0 {
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
        }
        Ok::<_, Infallible>(Poll::Ready("done"))
    })
    .await;
    assert!(matches!(ready, Ok("done")), "{ready:?}");
    assert_eq!(clock.now(), start);
    assert!(clock.slept().is_empty());

    let thirty = Duration::from_secs(30);
    tokio::select! {
        () = clock.sleep(thirty) => {}
        () = std::future::pending::<()>() => {}
    }
    assert_eq!(clock.now(), Timestamp(start.0 + 30));
    assert_eq!(clock.slept(), [thirty]);
    Ok(())
}

#[tokio::test]
async fn explicit_advancement_wakes_an_elapsed_sleep_while_work_remains_held() -> TestResult {
    let clock = Arc::new(FakeClock::new());
    let start = clock.now();
    let working = clock.working();
    let sleeper_clock = Arc::clone(&clock);
    let mut sleeper = tokio::spawn(async move {
        sleeper_clock.sleep(Duration::from_secs(10)).await;
    });
    while clock.sleepers() == 0 {
        tokio::task::yield_now().await;
    }
    assert!(
        !sleeper.is_finished(),
        "the deadline passed before advancement"
    );

    clock.advance(Duration::from_secs(5));
    tokio::task::yield_now().await;
    assert!(
        !sleeper.is_finished(),
        "a partial advancement completed the sleep"
    );
    assert_eq!(clock.now(), Timestamp(start.0 + 5));

    clock.advance(Duration::from_secs(5));
    tokio::time::timeout(Duration::from_secs(2), &mut sleeper).await??;
    assert_eq!(clock.sleepers(), 0, "a completed sleep remains registered");
    assert_eq!(clock.now(), Timestamp(start.0 + 10));
    assert_eq!(clock.slept(), [Duration::from_secs(10)]);
    drop(working);
    Ok(())
}

#[tokio::test]
async fn explicit_advancement_wakes_each_deadline_without_completing_later_sleepers() -> TestResult
{
    let clock = Arc::new(FakeClock::new());
    let start = clock.now();
    let working = clock.working();
    let short_clock = Arc::clone(&clock);
    let mut short = tokio::spawn(async move {
        short_clock.sleep(Duration::from_secs(3)).await;
    });
    let long_clock = Arc::clone(&clock);
    let mut long = tokio::spawn(async move {
        long_clock.sleep(Duration::from_secs(7)).await;
    });
    while clock.sleepers() != 2 {
        tokio::task::yield_now().await;
    }

    clock.advance(Duration::from_secs(3));
    tokio::time::timeout(Duration::from_secs(2), &mut short).await??;
    assert!(
        !long.is_finished(),
        "the later deadline completed too early"
    );
    assert_eq!(clock.sleepers(), 1, "the later sleep was removed early");
    assert_eq!(clock.now(), Timestamp(start.0 + 3));

    clock.advance(Duration::from_secs(4));
    tokio::time::timeout(Duration::from_secs(2), &mut long).await??;
    assert_eq!(clock.sleepers(), 0, "completed sleeps remain registered");
    assert_eq!(clock.now(), Timestamp(start.0 + 7));
    let mut slept = clock.slept();
    slept.sort();
    assert_eq!(slept, [Duration::from_secs(3), Duration::from_secs(7)]);
    drop(working);
    Ok(())
}

#[tokio::test]
async fn test_runner_starts_git_with_a_cleared_environment_plus_the_spec_env() -> TestResult {
    let runner = TestRunner::new();
    let mut run = spec(Program::Git, &["-c", "alias.e=!env", "e"]);
    run.env = vec![("JACKIN_F2".to_owned(), EnvValue::Plain("1".to_owned()))];
    let printed = stdout(&runner, &run).await?;
    let lines: Vec<&str> = printed.lines().collect();
    assert!(lines.contains(&"JACKIN_F2=1"), "the spec env is missing");
    let inherited: Vec<&&str> = lines
        .iter()
        .filter(|line| line.starts_with("HOME=") || line.starts_with("CARGO_MANIFEST_DIR="))
        .collect();
    assert!(inherited.is_empty(), "inherited: {inherited:?}");
    Ok(())
}

#[tokio::test]
async fn a_pre_cancelled_git_call_is_cancelled_before_git_is_started() -> TestResult {
    let dir = tempfile::tempdir()?;
    let runner = TestRunner::new();
    let mut run = spec(Program::Git, &["--version"]);
    run.cwd = Some(HostPath::parse(&dir.path().join("absent"))?);

    let started = runner.run(&run, None, &CancellationToken::new()).await;
    assert!(
        matches!(
            started,
            Err(ProcessError::Spawn {
                program: Program::Git,
                ..
            })
        ),
        "{started:?}"
    );

    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let plain = runner.run(&run, None, &cancelled).await;
    assert!(
        matches!(
            plain,
            Err(ProcessError::Cancelled {
                program: Program::Git,
                group: None
            })
        ),
        "{plain:?}"
    );
    let secret = runner.run_secret(&run, &cancelled).await;
    assert!(
        matches!(
            secret,
            Err(ProcessError::Cancelled {
                program: Program::Git,
                group: None
            })
        ),
        "{secret:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_pre_cancelled_call_is_cancelled_before_any_reply_is_chosen_or_any_effect_runs()
-> TestResult {
    let dir = tempfile::tempdir()?;
    let iidfile = dir.path().join("iid");
    let runner = ScriptedProcessRunner::new();
    runner.expect(
        ScriptedProgram::Docker,
        &["build"],
        ProcessReply::exited(0, b"one", b"").with_effect(ProcessEffect::WriteArgFile {
            prefix: "--iidfile=".to_owned(),
            bytes: b"sha256:abc\n".to_vec(),
        }),
    );
    runner.expect(
        ScriptedProgram::Docker,
        &["build"],
        ProcessReply::exited(0, b"two", b""),
    );
    let build = spec(
        Program::Docker(DockerSubcommand::BuildxBuild),
        &["build", &format!("--iidfile={}", iidfile.display())],
    );
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let run = runner.run(&build, None, &cancelled).await;
    assert!(
        matches!(
            run,
            Err(ProcessError::Cancelled {
                program: Program::Docker(_),
                group: None
            })
        ),
        "{run:?}"
    );
    let secret = runner.run_secret(&build, &cancelled).await;
    assert!(
        matches!(
            secret,
            Err(ProcessError::Cancelled {
                program: Program::Docker(_),
                group: None
            })
        ),
        "{secret:?}"
    );
    assert!(!tokio::fs::try_exists(&iidfile).await?);
    assert!(runner.seen().is_empty());

    assert_eq!(stdout(&runner, &build).await?, "one");
    assert_eq!(tokio::fs::read(&iidfile).await?, b"sha256:abc\n");
    assert_eq!(runner.seen().len(), 1);
    Ok(())
}

async fn snapshot_fixture() -> Result<(RepoFixture, std::path::PathBuf), Box<dyn Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"same\n").await?;
    repo.file("b.txt", b"same\n").await?;
    repo.file("dir/c.txt", b"c\n").await?;
    repo.ignore("link\ndangling\nempty\n").await?;
    repo.commit().await?;
    let root = repo.root().as_path().to_path_buf();
    std::os::unix::fs::symlink("a.txt", root.join("link"))?;
    Ok((repo, root))
}

async fn rest(repo: &RepoFixture) -> Result<HostState, Box<dyn Error>> {
    Ok(HostSnapshot::take(repo.root()).await?.rest)
}

fn changed_fields(after: &HostState, before: &HostState) -> Vec<&'static str> {
    [
        ("index", after.index != before.index),
        ("refs", after.refs != before.refs),
        ("head", after.head != before.head),
        ("status", after.status != before.status),
        ("config", after.config != before.config),
        ("files", after.files != before.files),
        ("links", after.links != before.links),
        ("empty_dirs", after.empty_dirs != before.empty_dirs),
    ]
    .into_iter()
    .filter_map(|(field, changed)| changed.then_some(field))
    .collect()
}

#[track_caller]
fn assert_same(step: &str, after: &HostState, before: &HostState) {
    assert_eq!(
        changed_fields(after, before),
        Vec::<&str>::new(),
        "{step}: the snapshot differs in these fields"
    );
}

#[track_caller]
fn assert_changed(step: &str, after: &HostState, before: &HostState) {
    assert!(
        !changed_fields(after, before).is_empty(),
        "{step}: the snapshot did not change"
    );
}

#[tokio::test]
async fn a_host_snapshot_is_equal_without_a_change_and_equal_again_after_a_change_is_reverted()
-> TestResult {
    let (repo, root) = snapshot_fixture().await?;
    let base = rest(&repo).await?;
    assert_same(
        "a second snapshot of the same tree",
        &rest(&repo).await?,
        &base,
    );
    std::fs::create_dir(root.join("empty"))?;
    std::fs::remove_dir(root.join("empty"))?;
    std::fs::remove_file(root.join("link"))?;
    std::os::unix::fs::symlink("a.txt", root.join("link"))?;
    assert_same("after the changes are reverted", &rest(&repo).await?, &base);
    Ok(())
}

#[tokio::test]
async fn a_host_snapshot_sees_an_added_empty_directory() -> TestResult {
    let (repo, root) = snapshot_fixture().await?;
    let base = rest(&repo).await?;
    std::fs::create_dir(root.join("empty"))?;
    assert_changed("after the change", &rest(&repo).await?, &base);
    std::fs::remove_dir(root.join("empty"))?;
    assert_same("after the change is reverted", &rest(&repo).await?, &base);
    Ok(())
}

#[tokio::test]
async fn a_host_snapshot_sees_a_link_retargeted_to_a_file_with_identical_content() -> TestResult {
    let (repo, root) = snapshot_fixture().await?;
    let base = rest(&repo).await?;
    std::fs::remove_file(root.join("link"))?;
    std::os::unix::fs::symlink("b.txt", root.join("link"))?;
    assert_changed("after the change", &rest(&repo).await?, &base);
    std::fs::remove_file(root.join("link"))?;
    std::os::unix::fs::symlink("a.txt", root.join("link"))?;
    assert_same("after the change is reverted", &rest(&repo).await?, &base);
    Ok(())
}

#[tokio::test]
async fn a_host_snapshot_sees_a_link_replaced_by_a_regular_file_with_identical_content()
-> TestResult {
    let (repo, root) = snapshot_fixture().await?;
    let base = rest(&repo).await?;
    std::fs::remove_file(root.join("link"))?;
    std::fs::write(root.join("link"), b"same\n")?;
    assert_changed("after the change", &rest(&repo).await?, &base);
    std::fs::remove_file(root.join("link"))?;
    std::os::unix::fs::symlink("a.txt", root.join("link"))?;
    assert_same("after the change is reverted", &rest(&repo).await?, &base);
    Ok(())
}

#[tokio::test]
async fn a_host_snapshot_sees_an_added_dangling_link() -> TestResult {
    let (repo, root) = snapshot_fixture().await?;
    let base = rest(&repo).await?;
    std::os::unix::fs::symlink("nowhere", root.join("dangling"))?;
    assert_changed("after the change", &rest(&repo).await?, &base);
    std::fs::remove_file(root.join("dangling"))?;
    assert_same("after the change is reverted", &rest(&repo).await?, &base);
    Ok(())
}

#[tokio::test]
async fn git_reads_a_file_stdin_from_the_offset_and_a_missing_file_never_starts_it() -> TestResult {
    let dir = tempfile::tempdir()?;
    let runner = TestRunner::new();
    let file = HostPath::parse(&dir.path().join("payload.bin"))?;
    std::fs::write(file.as_path(), b"prefix:slice")?;
    let hash = |stdin: ProcessStdin| {
        let mut hash = spec(Program::Git, &["hash-object", "--stdin"]);
        hash.cwd = Some(HostPath::parse(dir.path()).unwrap());
        hash.stdin = stdin;
        hash
    };
    let cancel = CancellationToken::new();
    let tail = runner
        .run(
            &hash(ProcessStdin::File {
                path: file,
                offset: 7,
            }),
            None,
            &cancel,
        )
        .await?;
    let expected = runner
        .run(
            &hash(ProcessStdin::Bytes(SecretBytes::new(b"slice".to_vec()))),
            None,
            &cancel,
        )
        .await?;
    assert_eq!(tail.status, ProcessStatus::Exited(0));
    assert_eq!(tail.stdout, expected.stdout);
    let missing = HostPath::parse(&dir.path().join("gone.bin"))?;
    let refused = runner
        .run(
            &hash(ProcessStdin::File {
                path: missing,
                offset: 0,
            }),
            None,
            &cancel,
        )
        .await;
    assert!(
        matches!(refused, Err(ProcessError::Spawn { .. })),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_scripted_program_records_a_file_stdin_with_its_path_and_offset() -> TestResult {
    let runner = ScriptedProcessRunner::new();
    runner.expect(
        ScriptedProgram::Openshell,
        &[],
        ProcessReply::exited(0, b"", b""),
    );
    let path = host("/mem/pack.bin")?;
    let mut call = spec(Program::Openshell, &["x"]);
    call.stdin = ProcessStdin::File {
        path: path.clone(),
        offset: 12,
    };
    runner.run(&call, None, &CancellationToken::new()).await?;
    assert_eq!(runner.seen()[0].stdin, SeenStdin::File { path, offset: 12 });
    Ok(())
}
