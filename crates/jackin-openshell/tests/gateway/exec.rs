// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::num::NonZeroU16;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_core::{EnvName, FileMode, OutputStream, SandboxName, SandboxPath, SecretBytes};
use jackin_openshell::{
    EndReason, Gateway, GatewayError, GatewayOp, ProgramRun, ProgramStdin, SandboxFile, TermSize,
    TerminalEvent, TerminalKind, TerminalRequest,
};
use openshell_sdk::raw::proto::{SandboxPhase, exec_sandbox_input};
use tempfile::TempDir;
use tokio::sync::Barrier;
use tokio_util::sync::CancellationToken;

use super::exec_rpc::{
    Step, exit_event, host_shell, host_shell_aborting, host_shell_gated, out_event, scripted,
};
use super::harness::{Env, WORKSPACE, connect, env, guard, wait_until};
use super::server::ExecCtx;
use super::support::{SANDBOX, add_sandbox, sandbox_name};

const SENTINEL: &str = "Zq7#Xv9!Lm2@Hk4wP";

fn path(text: &str) -> SandboxPath {
    SandboxPath::parse(text).unwrap()
}

fn run(program: &str, args: &[&str]) -> ProgramRun {
    ProgramRun {
        sandbox: sandbox_name(SANDBOX),
        program: path(program),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        workdir: None,
        env: BTreeMap::new(),
        stdin: ProgramStdin::Closed,
        touch_on_success: None,
        timeout: Duration::from_secs(30),
    }
}

fn ready(env: &Env) {
    add_sandbox(
        &mut env.server.state(),
        WORKSPACE,
        SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
}

fn assert_no_sentinel(text: &str) {
    for window in SENTINEL.as_bytes().windows(3) {
        let window = std::str::from_utf8(window).unwrap();
        assert!(!text.contains(window), "{window} found in {text}");
    }
}

#[tokio::test]
async fn run_program_returns_exit_code_stdout_stderr_and_forwards_stdin() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    env.server.state().exec = Some(host_shell(root.path().to_path_buf()));
    ready(&env);
    let gateway = connect(&env, WORKSPACE).await;
    let cancel = CancellationToken::new();
    let mut input = b"\xff\x00line\n".to_vec();
    input.extend_from_slice(SENTINEL.as_bytes());
    let mut first = run("/bin/sh", &["-c", "cat; echo err >&2; exit 7"]);
    first.stdin = ProgramStdin::Data(SecretBytes::new(input.clone()));
    let outcome = guard(gateway.run_program(&first, &cancel)).await.unwrap();
    assert_eq!(outcome.exit_code, 7);
    assert_eq!(outcome.stdout, input);
    assert_eq!(outcome.stderr, b"err\n");

    let closed = run("/bin/sh", &["-c", "cat; echo done"]);
    let outcome = guard(gateway.run_program(&closed, &cancel)).await.unwrap();
    assert_eq!((outcome.exit_code, outcome.stdout), (0, b"done\n".to_vec()));

    let mut shaped = run("/bin/sh", &["-c", "printf '%s|' \"$FOO\"; pwd"]);
    shaped.env = BTreeMap::from([(EnvName::parse("FOO").unwrap(), "bar".to_owned())]);
    shaped.workdir = Some(path("/sandbox/project"));
    let outcome = guard(gateway.run_program(&shaped, &cancel)).await.unwrap();
    let text = String::from_utf8(outcome.stdout).unwrap();
    assert!(
        text.starts_with("bar|") && text.trim_end().ends_with("/sandbox/project"),
        "{text}"
    );

    let starts = env.server.state().exec_starts.clone();
    assert_eq!(starts.len(), 3);
    for start in &starts {
        assert!(!start.tty);
        assert!(start.stdin.is_empty());
        assert_no_sentinel(&format!("{start:?}"));
    }
}

#[tokio::test]
async fn touch_on_success_creates_the_marker_only_after_exit_zero_in_one_exec() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    tokio::fs::create_dir_all(root.path().join("sandbox/.jackin"))
        .await
        .unwrap();
    env.server.state().exec = Some(host_shell(root.path().to_path_buf()));
    ready(&env);
    let gateway = connect(&env, WORKSPACE).await;
    let cancel = CancellationToken::new();
    let ok_marker = root.path().join("sandbox/.jackin/setup-done-ok");
    let bad_marker = root.path().join("sandbox/.jackin/setup-done-bad");
    let mut success = run("/bin/sh", &["-c", "exit 0"]);
    success.touch_on_success = Some(path("/sandbox/.jackin/setup-done-ok"));
    let outcome = guard(gateway.run_program(&success, &cancel)).await.unwrap();
    assert_eq!(outcome.exit_code, 0);
    assert!(ok_marker.exists());
    assert_eq!(env.server.state().exec_streams, 1);
    let mut failure = run("/bin/sh", &["-c", "exit 1"]);
    failure.touch_on_success = Some(path("/sandbox/.jackin/setup-done-bad"));
    let outcome = guard(gateway.run_program(&failure, &cancel)).await.unwrap();
    assert_eq!(outcome.exit_code, 1);
    assert!(!bad_marker.exists());
    assert_eq!(env.server.state().exec_streams, 2);
}

#[tokio::test]
async fn program_timeout_and_cancel_stop_the_remote_process() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    env.server.state().exec = Some(host_shell(root.path().to_path_buf()));
    ready(&env);
    let gateway = Arc::new(connect(&env, WORKSPACE).await);
    let mut sleeper = run("/bin/sh", &["-c", "sleep 300"]);
    sleeper.timeout = Duration::from_secs(5);
    let timed = {
        let gateway = Arc::clone(&gateway);
        tokio::spawn(async move {
            gateway
                .run_program(&sleeper, &CancellationToken::new())
                .await
        })
    };
    guard(env.server.shared.child_started.notified()).await;
    tokio::time::pause();
    let error = timed.await.unwrap().unwrap_err();
    tokio::time::resume();
    assert!(matches!(
        error,
        GatewayError::Timeout { op: GatewayOp::Program, after, .. } if after == Duration::from_secs(5)
    ));
    guard(env.server.shared.child_killed.notified()).await;

    let cancel = CancellationToken::new();
    let cancelled = {
        let gateway = Arc::clone(&gateway);
        let cancel = cancel.clone();
        let sleeper = run("/bin/sh", &["-c", "sleep 300"]);
        tokio::spawn(async move { gateway.run_program(&sleeper, &cancel).await })
    };
    guard(env.server.shared.child_started.notified()).await;
    cancel.cancel();
    let error = guard(cancelled).await.unwrap().unwrap_err();
    assert!(matches!(
        error,
        GatewayError::Cancelled {
            op: GatewayOp::Program,
            ..
        }
    ));
    guard(env.server.shared.child_killed.notified()).await;
}

#[tokio::test]
async fn write_file_applies_mode_and_exact_bytes_and_keeps_content_off_argv() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    env.server.state().exec = Some(host_shell(root.path().to_path_buf()));
    ready(&env);
    let gateway = connect(&env, WORKSPACE).await;
    let mut content: Vec<u8> = (0..65_536_u32)
        .map(|n| u8::try_from(n % 251).unwrap())
        .collect();
    content.extend_from_slice(SENTINEL.as_bytes());
    let file = || SandboxFile {
        path: path("/sandbox/.codex/auth.json"),
        mode: FileMode(0o600),
        content: SecretBytes::new(content.clone()),
    };
    let target = root.path().join("sandbox/.codex/auth.json");
    tokio::fs::create_dir_all(target.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&target, b"old").await.unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644))
            .await
            .unwrap();
    }
    guard(gateway.write_file(&sandbox_name(SANDBOX), &file()))
        .await
        .unwrap();
    guard(gateway.write_file(&sandbox_name(SANDBOX), &file()))
        .await
        .unwrap();
    assert_eq!(tokio::fs::read(&target).await.unwrap(), content);
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = tokio::fs::metadata(&target)
            .await
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let mut entries = tokio::fs::read_dir(target.parent().unwrap()).await.unwrap();
    let mut names = Vec::new();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    assert_eq!(names, vec!["auth.json".to_owned()]);
    let fresh = SandboxFile {
        path: path("/sandbox/.fresh/nested/auth.json"),
        mode: FileMode(0o600),
        content: SecretBytes::new(b"fresh".to_vec()),
    };
    guard(gateway.write_file(&sandbox_name(SANDBOX), &fresh))
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::read(root.path().join("sandbox/.fresh/nested/auth.json"))
            .await
            .unwrap(),
        b"fresh"
    );
    for start in &env.server.state().exec_starts {
        assert_no_sentinel(&format!("{start:?}"));
    }
}

async fn names_in(directory: &std::path::Path) -> Vec<String> {
    let mut entries = tokio::fs::read_dir(directory).await.unwrap();
    let mut names = Vec::new();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names
}

#[tokio::test]
async fn concurrent_writes_to_one_target_each_succeed_and_the_last_rename_wins() {
    const LEN: usize = 512 * 1024;
    let env = env().await;
    let root = TempDir::new().unwrap();
    env.server.state().exec = Some(host_shell_gated(
        root.path().to_path_buf(),
        8,
        Arc::new(Barrier::new(2)),
    ));
    ready(&env);
    let gateway = connect(&env, WORKSPACE).await;
    let file = |byte: u8| SandboxFile {
        path: path("/sandbox/.codex/auth.json"),
        mode: FileMode(0o600),
        content: SecretBytes::new(vec![byte; LEN]),
    };
    let name = sandbox_name(SANDBOX);
    let (a, b) = (file(0xAA), file(0xBB));
    let (first, second) =
        guard(async { tokio::join!(gateway.write_file(&name, &a), gateway.write_file(&name, &b)) })
            .await;
    first.unwrap();
    second.unwrap();
    let target = root.path().join("sandbox/.codex/auth.json");
    let stored = tokio::fs::read(&target).await.unwrap();
    assert!(
        stored == vec![0xAA; LEN] || stored == vec![0xBB; LEN],
        "the target is neither writer's content"
    );
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = tokio::fs::metadata(&target)
            .await
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert_eq!(
        names_in(target.parent().unwrap()).await,
        vec!["auth.json".to_owned()]
    );
}

#[tokio::test]
async fn write_file_to_a_name_near_the_filesystem_limit_succeeds() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    env.server.state().exec = Some(host_shell(root.path().to_path_buf()));
    ready(&env);
    let gateway = connect(&env, WORKSPACE).await;
    let base = "a".repeat(240);
    let file = SandboxFile {
        path: path(&format!("/sandbox/{base}")),
        mode: FileMode(0o600),
        content: SecretBytes::new(b"near the limit".to_vec()),
    };
    guard(gateway.write_file(&sandbox_name(SANDBOX), &file))
        .await
        .unwrap();
    let directory = root.path().join("sandbox");
    assert_eq!(
        tokio::fs::read(directory.join(&base)).await.unwrap(),
        b"near the limit"
    );
    assert_eq!(names_in(&directory).await, vec![base]);
}

#[tokio::test]
async fn interrupted_write_leaves_the_old_file_intact() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    env.server.state().exec = Some(host_shell_aborting(root.path().to_path_buf(), Some(2)));
    ready(&env);
    let gateway = connect(&env, WORKSPACE).await;
    let target = root.path().join("sandbox/.codex/auth.json");
    tokio::fs::create_dir_all(target.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&target, b"old bytes").await.unwrap();
    let file = SandboxFile {
        path: path("/sandbox/.codex/auth.json"),
        mode: FileMode(0o600),
        content: SecretBytes::new(vec![7_u8; 200 * 1024]),
    };
    let error = guard(gateway.write_file(&sandbox_name(SANDBOX), &file))
        .await
        .unwrap_err();
    assert!(
        matches!(error, GatewayError::Unreachable { .. }),
        "{error:?}"
    );
    guard(env.server.shared.child_killed.notified()).await;
    assert_eq!(tokio::fs::read(&target).await.unwrap(), b"old bytes");
}

fn secret_file(target: &str) -> SandboxFile {
    SandboxFile {
        path: path(target),
        mode: FileMode(0o600),
        content: SecretBytes::new(SENTINEL.as_bytes().to_vec()),
    }
}

#[tokio::test]
async fn a_failed_write_names_the_path_and_the_exit_code_and_carries_no_sandbox_output() {
    for code in [1, 73] {
        let env = env().await;
        env.server.state().exec = Some(scripted(move |_start, stdin| {
            vec![
                Step::Err(stdin.to_vec()),
                Step::Out(stdin.to_vec()),
                Step::Exit(code),
            ]
        }));
        ready(&env);
        let gateway = connect(&env, WORKSPACE).await;
        let target = "/sandbox/.jackin/docker/key.pem";
        let error = guard(gateway.write_file(&sandbox_name(SANDBOX), &secret_file(target)))
            .await
            .unwrap_err();
        match (code, &error) {
            (73, GatewayError::UnsafePath { path }) => assert_eq!(path.as_str(), target),
            (1, GatewayError::Protocol { message }) => {
                assert!(
                    message.contains(target) && message.contains("exit code 1"),
                    "{message}"
                );
            }
            (_, other) => panic!("exit {code}: unexpected {other:?}"),
        }
        assert!(error.to_string().contains(target), "{error}");
        assert_no_sentinel(&error.to_string());
        assert_no_sentinel(&format!("{error:?}"));
    }
}

#[tokio::test]
async fn write_file_refuses_a_symlinked_directory_or_a_directory_target_and_replaces_a_link_leaf() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    env.server.state().exec = Some(host_shell(root.path().to_path_buf()));
    ready(&env);
    let gateway = connect(&env, WORKSPACE).await;
    let sandbox = root.path().join("sandbox");
    let project = sandbox.join("project");
    tokio::fs::create_dir_all(&project).await.unwrap();
    let refused = |target: &'static str, watched: PathBuf| {
        let gateway = &gateway;
        async move {
            let result =
                guard(gateway.write_file(&sandbox_name(SANDBOX), &secret_file(target))).await;
            assert_eq!(
                names_in(&watched).await,
                Vec::<String>::new(),
                "{target}: {result:?}"
            );
            match result {
                Err(GatewayError::UnsafePath { path }) => assert_eq!(path.as_str(), target),
                other => panic!("{target}: {other:?}"),
            }
        }
    };
    let link = |target: &str, at: PathBuf| std::os::unix::fs::symlink(target, at).unwrap();
    let key = "/sandbox/.jackin/docker/key.pem";

    link("project", sandbox.join(".jackin"));
    for target in [
        key,
        "/sandbox/.jackin/key.pem",
        "/sandbox/.jackin/docker/x/key.pem",
    ] {
        refused(target, project.clone()).await;
    }
    tokio::fs::remove_file(sandbox.join(".jackin"))
        .await
        .unwrap();

    tokio::fs::create_dir(sandbox.join(".jackin"))
        .await
        .unwrap();
    link("../project", sandbox.join(".jackin/docker"));
    refused(key, project.clone()).await;
    tokio::fs::remove_file(sandbox.join(".jackin/docker"))
        .await
        .unwrap();

    link("missing", sandbox.join(".jackin/docker"));
    refused(key, project.clone()).await;
    tokio::fs::remove_file(sandbox.join(".jackin/docker"))
        .await
        .unwrap();

    let leaf = sandbox.join(".jackin/docker/key.pem");
    tokio::fs::create_dir_all(&leaf).await.unwrap();
    refused(key, leaf.clone()).await;
    tokio::fs::remove_dir(&leaf).await.unwrap();

    let victim = project.join("victim");
    tokio::fs::write(&victim, b"untouched").await.unwrap();
    link("../../project/victim", leaf.clone());
    guard(gateway.write_file(&sandbox_name(SANDBOX), &secret_file(key)))
        .await
        .unwrap();
    let text = |bytes: Vec<u8>| String::from_utf8(bytes).unwrap();
    assert_eq!(text(tokio::fs::read(&victim).await.unwrap()), "untouched");
    assert_eq!(text(tokio::fs::read(&leaf).await.unwrap()), SENTINEL);
    assert!(
        !tokio::fs::symlink_metadata(&leaf)
            .await
            .unwrap()
            .is_symlink()
    );
    assert_eq!(names_in(&project).await, vec!["victim".to_owned()]);
    assert_eq!(
        names_in(leaf.parent().unwrap()).await,
        vec!["key.pem".to_owned()]
    );
}

#[tokio::test]
async fn write_file_refuses_a_target_outside_the_sandbox_home_without_running_anything() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    env.server.state().exec = Some(host_shell(root.path().to_path_buf()));
    ready(&env);
    let gateway = connect(&env, WORKSPACE).await;
    for target in ["/tmp/key.pem", "/sandbox", "/sandboxes/key.pem", "/"] {
        let error = guard(gateway.write_file(&sandbox_name(SANDBOX), &secret_file(target)))
            .await
            .unwrap_err();
        assert!(
            matches!(error, GatewayError::InvalidArgument { ref field, .. } if field == "path"),
            "{target}: {error:?}"
        );
    }
    assert!(env.server.state().exec_starts.is_empty());
}

#[tokio::test]
async fn read_files_returns_one_entry_per_path_and_none_for_missing() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    env.server.state().exec = Some(host_shell(root.path().to_path_buf()));
    ready(&env);
    let gateway = connect(&env, WORKSPACE).await;
    let base = root.path().join("sandbox");
    tokio::fs::create_dir_all(&base).await.unwrap();
    let binary = vec![0xff, 0x00, 0xfe, b'\n', 0x80];
    let big: Vec<u8> = (0..100 * 1024_u32)
        .map(|n| u8::try_from(n % 253).unwrap())
        .collect();
    tokio::fs::write(base.join("a"), &binary).await.unwrap();
    tokio::fs::write(base.join("empty"), b"").await.unwrap();
    tokio::fs::write(base.join("big"), &big).await.unwrap();
    let paths = [
        "/sandbox/a",
        "/sandbox/missing",
        "/sandbox/empty",
        "/sandbox/big",
        "/sandbox/a",
    ]
    .map(path);
    let files = guard(gateway.read_files(&sandbox_name(SANDBOX), &paths))
        .await
        .unwrap();
    assert_eq!(
        files,
        vec![
            Some(binary.clone()),
            None,
            Some(Vec::new()),
            Some(big),
            Some(binary)
        ]
    );
}

fn shell_request(sandbox: &SandboxName) -> TerminalRequest {
    TerminalRequest {
        sandbox: sandbox.clone(),
        kind: TerminalKind::Shell {
            workdir: path("/sandbox"),
            env: BTreeMap::from([(EnvName::parse("JACKIN_PANE").unwrap(), "shell-1".to_owned())]),
        },
        size: TermSize {
            cols: NonZeroU16::new(100).unwrap(),
            rows: NonZeroU16::new(30).unwrap(),
        },
    }
}

async fn record_until_resize(mut ctx: ExecCtx, log: Arc<Mutex<Vec<exec_sandbox_input::Payload>>>) {
    while let Some(payload) = ctx.inputs.recv().await {
        let resize = matches!(payload, exec_sandbox_input::Payload::Resize(_));
        log.lock().unwrap().push(payload);
        if resize {
            break;
        }
    }
    for event in [
        out_event(b"a\xff\x1b[31m".to_vec()),
        out_event(b"b\r\n".to_vec()),
        exit_event(3),
    ] {
        if ctx.events.send(Ok(event)).await.is_err() {
            return;
        }
    }
}

#[tokio::test]
async fn shell_terminal_stream_shape_and_raw_output() {
    let env = env().await;
    ready(&env);
    let seen: Arc<Mutex<Vec<exec_sandbox_input::Payload>>> = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    env.server.state().exec = Some(Arc::new(move |ctx| {
        Box::pin(record_until_resize(ctx, Arc::clone(&log)))
    }));
    let gateway = connect(&env, WORKSPACE).await;
    let mut session = guard(gateway.open_terminal(&shell_request(&sandbox_name(SANDBOX))))
        .await
        .unwrap();
    assert_eq!(
        guard(session.next_event()).await,
        Some(TerminalEvent::Attached)
    );
    guard(session.input(b"echo hi\n".to_vec())).await.unwrap();
    let big: Vec<u8> = (0..10_000_u32)
        .map(|n| u8::try_from(n % 251).unwrap())
        .collect();
    guard(session.input(big.clone())).await.unwrap();
    guard(session.resize(TermSize {
        cols: NonZeroU16::new(120).unwrap(),
        rows: NonZeroU16::new(40).unwrap(),
    }))
    .await
    .unwrap();
    let expected = [
        TerminalEvent::Output(b"a\xff\x1b[31m".to_vec()),
        TerminalEvent::Output(b"b\r\n".to_vec()),
        TerminalEvent::Exited(Some(3)),
        TerminalEvent::Ended(EndReason::StreamClosed),
    ];
    for event in expected {
        assert_eq!(guard(session.next_event()).await, Some(event));
    }
    assert_eq!(guard(session.next_event()).await, None);

    let start = env.server.state().exec_starts[0].clone();
    assert!(start.tty);
    assert_eq!((start.cols, start.rows), (100, 30));
    assert_eq!(start.workdir, "/sandbox");
    assert_eq!(start.environment["JACKIN_PANE"], "shell-1");
    assert!(!start.command.is_empty());
    let observed = seen.lock().unwrap();
    let mut stdin = Vec::new();
    let mut chunk_sizes = Vec::new();
    let mut resize = None;
    for payload in observed.iter() {
        match payload {
            exec_sandbox_input::Payload::Stdin(bytes) => {
                chunk_sizes.push(bytes.len());
                stdin.extend_from_slice(bytes);
            }
            exec_sandbox_input::Payload::Resize(size) => resize = Some((size.cols, size.rows)),
            exec_sandbox_input::Payload::Start(_) => panic!("a second start"),
        }
    }
    let mut expected_stdin = b"echo hi\n".to_vec();
    expected_stdin.extend_from_slice(&big);
    assert_eq!(stdin, expected_stdin);
    assert!(chunk_sizes.iter().all(|size| *size <= 4096));
    assert_eq!(resize, Some((120, 40)));
}

#[tokio::test]
async fn shell_failure_refusal_and_drop() {
    let env = env().await;
    ready(&env);
    env.server.state().exec = Some(Arc::new(|ctx| {
        Box::pin(async move {
            drop(ctx.events.send(Ok(out_event(b"x".to_vec()))).await);
        })
    }));
    let gateway = connect(&env, WORKSPACE).await;
    let mut session = guard(gateway.open_terminal(&shell_request(&sandbox_name(SANDBOX))))
        .await
        .unwrap();
    let mut events = Vec::new();
    while let Some(event) = guard(session.next_event()).await {
        events.push(event);
    }
    assert!(matches!(events.first(), Some(TerminalEvent::Attached)));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, TerminalEvent::Exited(_)))
    );
    assert!(matches!(
        events.last(),
        Some(TerminalEvent::Ended(EndReason::Failed(_)))
    ));
    wait_until(|| env.server.state().observed.request_stream_ended).await;

    add_sandbox(
        &mut env.server.state(),
        WORKSPACE,
        "jackin-off",
        SandboxPhase::Stopped,
        None,
        &[],
    );
    let error = guard(gateway.open_terminal(&shell_request(&sandbox_name("jackin-off"))))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        GatewayError::NotReady {
            phase: jackin_core::Phase::Stopped
        }
    ));

    env.server.state().exec = Some(Arc::new(|mut ctx| {
        Box::pin(async move {
            while ctx.inputs.recv().await.is_some() {}
            std::future::pending::<()>().await;
        })
    }));
    env.server.state().observed.request_stream_ended = false;
    let session = guard(gateway.open_terminal(&shell_request(&sandbox_name(SANDBOX))))
        .await
        .unwrap();
    drop(session);
    wait_until(|| env.server.state().observed.request_stream_ended).await;
}

const CAP: usize = 16 * 1024 * 1024;

fn emit(stdout: usize, stderr: usize, then: &str) -> ProgramRun {
    run(
        "/bin/sh",
        &[
            "-c",
            &format!("head -c {stdout} /dev/zero; head -c {stderr} /dev/zero >&2; {then}"),
        ],
    )
}

#[tokio::test]
async fn collected_output_is_capped_per_stream() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    env.server.state().exec = Some(host_shell(root.path().to_path_buf()));
    ready(&env);
    let gateway = connect(&env, WORKSPACE).await;
    let cancel = CancellationToken::new();

    let at_cap = guard(gateway.run_program(&emit(CAP, CAP, "exit 0"), &cancel))
        .await
        .unwrap();
    assert_eq!(
        (at_cap.exit_code, at_cap.stdout.len(), at_cap.stderr.len()),
        (0, CAP, CAP)
    );

    for (stdout, stderr, stream) in [
        (CAP + 1, 0, OutputStream::Stdout),
        (0, CAP + 1, OutputStream::Stderr),
        (CAP, CAP + 1, OutputStream::Stderr),
    ] {
        let error = guard(gateway.run_program(&emit(stdout, stderr, "sleep 300"), &cancel))
            .await
            .unwrap_err();
        assert!(
            matches!(
                error,
                GatewayError::OutputTooLarge { op: GatewayOp::Program, stream: found, limit }
                    if found == stream && limit == CAP
            ),
            "{error:?}"
        );
        assert!(!error.is_retryable());
        guard(env.server.shared.child_killed.notified()).await;
    }

    let big = root.path().join("sandbox/big.bin");
    tokio::fs::create_dir_all(big.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&big, vec![0_u8; 17 * 1024 * 1024])
        .await
        .unwrap();
    let error = guard(gateway.read_files(&sandbox_name(SANDBOX), &[path("/sandbox/big.bin")]))
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            GatewayError::OutputTooLarge { op: GatewayOp::ReadFiles, stream: OutputStream::Stdout, limit }
                if limit == CAP
        ),
        "{error:?}"
    );
}
