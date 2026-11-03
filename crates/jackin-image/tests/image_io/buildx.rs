// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    BuilderName, DockerSubcommand, FileMode, GroupFault, HostPath, ImageId, ImageTag, OutputStream,
    ProcessError, ProcessRunner, ProcessStatus, Program, RepoPath, StdFileSystem,
};
use jackin_image::{
    BuildStage, BuildxEngine, ContextEntry, ContextProblem, ContextSource, EngineError,
    ImageEngine, ImageEvent, ImagePlan, MaterializedFile, PlannedBuild,
};
use jackin_testkit::{CapturedFile, ProcessReply, ScriptedProcessRunner, SeenCall, SeenStdin};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::support::{
    CONSTRUCT_DOCKERFILE, DERIVED_DOCKERFILE, IidFile, JACKIN_START, Outcome, ROLE_DOCKERFILE,
    SCAN, Script, Stack, TestResult, assert_local_only, guarded, tag_id,
};

fn still_alive() -> GroupFault {
    GroupFault::Alive {
        after: Duration::from_secs(5),
    }
}

fn fixed_id() -> Result<ImageId, Box<dyn Error>> {
    Ok(ImageId::parse(&format!("sha256:{}", "2".repeat(64)))?)
}

fn plan_for(stack: &Stack) -> Result<(ImagePlan, jackin_image::FinalBuilds), Box<dyn Error>> {
    let plan = ImagePlan::new(&stack.fixture.request()?)?;
    let finals = plan.final_builds(&ImageId::parse(&tag_id("construct"))?);
    Ok((plan, finals))
}

fn path_arg<'a>(call: &'a SeenCall, prefix: &str) -> Result<&'a str, Box<dyn Error>> {
    call.args
        .iter()
        .find_map(|arg| arg.strip_prefix(prefix))
        .ok_or_else(|| format!("no {prefix} argument").into())
}

#[tokio::test]
async fn buildx_passes_plan_args_context_and_stdin_to_docker_and_reads_the_image_id() -> TestResult
{
    guarded(async {
        let stack = Stack::new()?;
        for stage in [BuildStage::Construct, BuildStage::Role, BuildStage::Derived] {
            let iid = IidFile::Text(format!("sha256:{}\n", "2".repeat(64)));
            stack.model.script(
                stage,
                Script {
                    iid,
                    ..Script::default()
                },
            );
        }
        let (plan, finals) = plan_for(&stack)?;
        let builds = [plan.construct_build(), &finals.role, &finals.derived];
        let (tx, _rx) = mpsc::channel(256);
        let cancel = CancellationToken::new();
        for build in builds {
            let id = stack.engine.build(build, &tx, &cancel).await?;
            assert_eq!(id, fixed_id()?, "{} ID", build.stage);
        }
        let calls = stack.calls(DockerSubcommand::BuildxBuild);
        assert_eq!(calls.len(), 3);
        let work = stack.fixture.work.as_path();
        for (call, build) in calls.iter().zip(builds) {
            let count = build.args.len();
            assert_eq!(
                &call.args[..count],
                build.args.as_slice(),
                "{} args",
                build.stage
            );
            assert_eq!(call.args.len(), count + 2, "{} extra tokens", build.stage);
            let iid = path_arg(call, "--iidfile=")?;
            assert_eq!(
                call.args[count],
                format!("--iidfile={iid}"),
                "iidfile position"
            );
            let context = &call.args[count + 1];
            assert!(Path::new(iid).starts_with(work), "{iid}");
            assert!(Path::new(context).starts_with(work), "{context}");
            let stdin = match build.stage {
                BuildStage::Role => SeenStdin::Bytes(ROLE_DOCKERFILE.to_vec()),
                _ => SeenStdin::Null,
            };
            assert_eq!(call.stdin, stdin, "{} stdin", build.stage);
            assert!(
                (Duration::from_secs(600)..=Duration::from_secs(7200)).contains(&call.timeout),
                "{:?}",
                call.timeout
            );
        }
        assert!(
            stack.fixture.work_entries().is_empty(),
            "directory left behind"
        );

        let construct = plan.construct_build();
        let (first, second) = tokio::join!(
            stack.engine.build(construct, &tx, &cancel),
            stack.engine.build(construct, &tx, &cancel)
        );
        assert_eq!(first?, fixed_id()?);
        assert_eq!(second?, fixed_id()?);
        let calls = stack.calls(DockerSubcommand::BuildxBuild);
        let pair = &calls[3..];
        assert_ne!(pair[0].args.last(), pair[1].args.last(), "context paths");
        assert_ne!(
            path_arg(&pair[0], "--iidfile=")?,
            path_arg(&pair[1], "--iidfile=")?,
            "iidfile paths"
        );
        assert!(
            stack.fixture.work_entries().is_empty(),
            "directory left behind"
        );
        assert_local_only(&stack.runner)
    })
    .await
}

fn write_mode(root: &Path, name: &str, mode: u32) -> TestResult {
    let path = root.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, format!("body of {name}"))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[tokio::test]
async fn buildx_context_holds_exactly_the_planned_files_with_normalized_modes() -> TestResult {
    guarded(async {
        let stack = Stack::new()?;
        let role = stack.fixture.role.as_path();
        let modes = [
            ("Dockerfile", 0o644, 0o644),
            ("src/a.rs", 0o644, 0o644),
            ("bin/tool", 0o755, 0o755),
            ("notes.txt", 0o600, 0o644),
            ("x/run.sh", 0o775, 0o755),
            ("s/set", 0o4755, 0o755),
        ];
        for (name, mode, _) in modes {
            write_mode(role, name, mode)?;
        }
        write_mode(role, "secret.env", 0o644)?;
        write_mode(role, ".git/config", 0o644)?;
        let mut request = stack.fixture.request()?;
        request.context_files = modes
            .iter()
            .map(|(name, mode, _)| {
                Ok(ContextEntry::File {
                    path: RepoPath::parse(name)?,
                    bytes: format!("body of {name}").into_bytes(),
                    executable: mode & 0o111 != 0,
                })
            })
            .collect::<Result<_, Box<dyn Error>>>()?;
        let plan = ImagePlan::new(&request)?;
        let finals = plan.final_builds(&ImageId::parse(&tag_id("construct"))?);
        let (tx, _rx) = mpsc::channel(256);
        let cancel = CancellationToken::new();
        for build in [plan.construct_build(), &finals.role, &finals.derived] {
            stack.engine.build(build, &tx, &cancel).await?;
        }
        let captured = stack.runner.captured_contexts();
        assert_eq!(captured.len(), 3);

        let construct: Vec<_> = captured[0].files.iter().collect();
        assert_eq!(construct.len(), 1, "construct files");
        assert_eq!(construct[0].0, "Dockerfile");
        assert_eq!(
            *construct[0].1,
            CapturedFile::file(CONSTRUCT_DOCKERFILE, 0o644)
        );

        let listed: BTreeSet<&str> = modes.iter().map(|(name, _, _)| *name).collect();
        let found: BTreeSet<&str> = captured[1].files.keys().map(String::as_str).collect();
        assert_eq!(found, listed, "role context file set");
        for (name, _, expected) in modes {
            assert_eq!(
                captured[1].files[name],
                CapturedFile::file(&fs::read(role.join(name))?, expected),
                "{name}"
            );
        }

        let derived = &captured[2].files;
        let names: BTreeSet<&str> = derived.keys().map(String::as_str).collect();
        let expected: BTreeSet<&str> = [
            "Dockerfile",
            "jackin-start",
            "scan",
            "hooks/setup",
            "hooks/preflight",
        ]
        .into();
        assert_eq!(names, expected, "derived context file set");
        let wanted: [(&str, &[u8]); 5] = [
            ("Dockerfile", DERIVED_DOCKERFILE),
            ("jackin-start", JACKIN_START),
            ("scan", SCAN),
            ("hooks/setup", b"#!/bin/sh\necho setup\n"),
            ("hooks/preflight", b""),
        ];
        for (name, bytes) in wanted {
            assert_eq!(derived[name], CapturedFile::file(bytes, 0o644), "{name}");
        }
        Ok(())
    })
    .await
}

type Listing = Vec<(String, Vec<u8>)>;

fn outside_listing(dir: &Path) -> Result<Listing, Box<dyn Error>> {
    let mut rows = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        rows.push((
            entry.file_name().to_string_lossy().into_owned(),
            fs::read(entry.path())?,
        ));
    }
    rows.sort();
    Ok(rows)
}

fn as_build(base: &PlannedBuild, context: ContextSource) -> PlannedBuild {
    PlannedBuild {
        context,
        ..base.clone()
    }
}

fn materialized(name: &str) -> ContextSource {
    ContextSource::Materialized(vec![MaterializedFile {
        name: name.to_owned(),
        bytes: b"x".to_vec(),
        mode: FileMode(0o644),
    }])
}

#[tokio::test]
async fn buildx_refuses_unsafe_context_entries_before_docker_runs() -> TestResult {
    guarded(async {
        let stack = Stack::new()?;
        let role = stack.fixture.role.as_path();
        let outside = tempfile::tempdir()?;
        fs::write(outside.path().join("x"), b"outside secret")?;
        fs::write(role.join("real.txt"), b"real")?;
        fs::create_dir(role.join("d"))?;
        symlink("/etc/hosts", role.join("link"))?;
        symlink(outside.path(), role.join("dir"))?;
        let before = outside_listing(outside.path())?;
        let base_names: Vec<String> = fs::read_dir(stack.fixture.dir.path())?
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        let (_, finals) = plan_for(&stack)?;
        let link = ContextSource::Entries(vec![ContextEntry::Link {
            path: RepoPath::parse("link")?,
            target: "/etc/hosts".to_owned(),
        }]);
        let rows: Vec<(String, ContextSource, ContextProblem)> = vec![
            ("link".into(), link, ContextProblem::Escapes),
            (
                "../escape".into(),
                materialized("../escape"),
                ContextProblem::Escapes,
            ),
            ("/abs".into(), materialized("/abs"), ContextProblem::Escapes),
            (
                "a/../../b".into(),
                materialized("a/../../b"),
                ContextProblem::Escapes,
            ),
            (String::new(), materialized(""), ContextProblem::Escapes),
        ];
        let (tx, _rx) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        for (path, context, problem) in rows {
            let build = as_build(&finals.role, context);
            match stack.engine.build(&build, &tx, &cancel).await {
                Err(EngineError::ContextEntry {
                    path: found,
                    problem: kind,
                }) => {
                    assert_eq!(found, path, "refused path");
                    assert_eq!(kind, problem, "problem for {path:?}");
                }
                other => {
                    return Err(
                        format!("row {path:?}: expected ContextEntry, got {other:?}").into(),
                    );
                }
            }
            assert!(
                stack.calls(DockerSubcommand::BuildxBuild).is_empty(),
                "docker ran for {path:?}"
            );
            assert!(
                stack.fixture.work_entries().is_empty(),
                "work dir for {path:?}"
            );
        }
        assert_eq!(
            outside_listing(outside.path())?,
            before,
            "outside directory"
        );
        let after: Vec<String> = fs::read_dir(stack.fixture.dir.path())?
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            after.iter().map(String::as_str).collect::<BTreeSet<_>>(),
            base_names
                .iter()
                .map(String::as_str)
                .chain(["work"])
                .collect::<BTreeSet<_>>(),
            "files outside the work directory"
        );
        Ok(())
    })
    .await
}

fn texts() -> Vec<(OutputStream, String)> {
    (0..120)
        .map(|index| {
            let stream = if index % 2 == 0 {
                OutputStream::Stdout
            } else {
                OutputStream::Stderr
            };
            let text = match index {
                10 => "\x1b[31merror\x1b[0m: boom".to_owned(),
                20 => "\x1b]0;t\x07x".to_owned(),
                30 => "a\tb".to_owned(),
                40 => "p\rq".to_owned(),
                50 => String::new(),
                _ => format!("line {index}"),
            };
            (stream, text)
        })
        .collect()
}

async fn collect_events(mut rx: mpsc::Receiver<ImageEvent>) -> Vec<ImageEvent> {
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    events
}

fn check_line_events(events: &[ImageEvent], stage: BuildStage, expected: &[String]) -> TestResult {
    assert_eq!(events.len(), expected.len(), "{stage} event count");
    for (index, (event, text)) in events.iter().zip(expected).enumerate() {
        let ImageEvent::Line {
            stage: found,
            text: line,
        } = event
        else {
            return Err(format!("event {index} is not a Line: {event:?}").into());
        };
        assert_eq!(*found, stage, "event {index} stage");
        assert_eq!(line.as_str(), text, "event {index} text");
    }
    Ok(())
}

#[tokio::test]
async fn buildx_streams_build_output_as_ordered_sanitized_line_events() -> TestResult {
    guarded(async {
        let stack = Stack::new()?;
        let raw = texts();
        let expected: Vec<String> = raw
            .iter()
            .enumerate()
            .map(|(index, (_, text))| match index {
                10 => "error: boom".to_owned(),
                20 => "x".to_owned(),
                40 => "pq".to_owned(),
                _ => text.clone(),
            })
            .collect();
        for stage in [BuildStage::Role, BuildStage::Derived] {
            stack.model.script(
                stage,
                Script {
                    lines: raw.clone(),
                    ..Script::default()
                },
            );
        }
        let (_, finals) = plan_for(&stack)?;
        let cancel = CancellationToken::new();
        for build in [&finals.role, &finals.derived] {
            let (tx, rx) = mpsc::channel::<ImageEvent>(1);
            let run = async {
                let result = stack.engine.build(build, &tx, &cancel).await;
                drop(tx);
                result
            };
            let (result, events) = tokio::join!(run, collect_events(rx));
            result?;
            check_line_events(&events, build.stage, &expected)?;
        }
        Ok(())
    })
    .await
}

fn numbered(count: usize) -> Vec<(OutputStream, String)> {
    (1..=count)
        .map(|index| {
            let stream = if index % 2 == 0 {
                OutputStream::Stderr
            } else {
                OutputStream::Stdout
            };
            let text = if index % 7 == 0 {
                format!("\x1b[1mstep {index}\x1b[0m")
            } else {
                format!("step {index}")
            };
            (stream, text)
        })
        .collect()
}

fn expected_tail() -> String {
    (61..=100)
        .map(|index| format!("step {index}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn buildx_build_succeeds_when_the_progress_receiver_is_gone() -> TestResult {
    guarded(async {
        let stack = Stack::new()?;
        let (_, finals) = plan_for(&stack)?;
        let (tx, rx) = mpsc::channel::<ImageEvent>(1);
        drop(rx);
        let cancel = CancellationToken::new();
        let lines = Script {
            lines: numbered(100),
            ..Script::default()
        };
        stack.model.script(BuildStage::Role, lines);
        let id = stack.engine.build(&finals.role, &tx, &cancel).await?;
        assert_eq!(id, ImageId::parse(&tag_id(finals.role.tag.as_str()))?);

        let failing = Script {
            lines: numbered(100),
            outcome: Outcome::Exit(ProcessStatus::Exited(2)),
            ..Script::default()
        };
        stack.model.script(BuildStage::Role, failing);
        match stack.engine.build(&finals.role, &tx, &cancel).await {
            Err(EngineError::BuildFailed {
                stage,
                status,
                tail,
            }) => {
                assert_eq!(stage, BuildStage::Role);
                assert_eq!(status, ProcessStatus::Exited(2));
                assert_eq!(tail.as_str(), expected_tail());
            }
            other => return Err(format!("expected BuildFailed, got {other:?}").into()),
        }
        Ok(())
    })
    .await
}

async fn role_build(stack: &Stack, script: Script) -> Result<ImageId, EngineError> {
    let (_, finals) = plan_for(stack).map_err(|error| {
        EngineError::Fs(jackin_core::FsError::Io {
            path: stack.fixture.work.clone(),
            kind: std::io::ErrorKind::Other,
            message: error.to_string(),
        })
    })?;
    stack.model.script(BuildStage::Role, script);
    let (tx, _rx) = mpsc::channel::<ImageEvent>(256);
    stack
        .engine
        .build(&finals.role, &tx, &CancellationToken::new())
        .await
}

fn unreadable(
    result: Result<ImageId, EngineError>,
    tag: &ImageTag,
) -> Result<String, Box<dyn Error>> {
    match result {
        Err(EngineError::DigestUnreadable { tag: found, output }) => {
            assert_eq!(&found, tag, "tag of the unreadable ID");
            Ok(output.as_str().to_owned())
        }
        other => Err(format!("expected DigestUnreadable, got {other:?}").into()),
    }
}

#[tokio::test]
async fn buildx_failures_timeouts_and_cancel_are_typed_and_leave_no_directory() -> TestResult {
    guarded(async {
        let stack = Stack::new()?;
        let (_, finals) = plan_for(&stack)?;
        let tag = &finals.role.tag;
        let exit = |status| Script {
            lines: numbered(100),
            outcome: Outcome::Exit(status),
            ..Script::default()
        };
        match role_build(&stack, exit(ProcessStatus::Exited(1))).await {
            Err(EngineError::BuildFailed { status, tail, .. }) => {
                assert_eq!(status, ProcessStatus::Exited(1));
                assert_eq!(tail.as_str(), expected_tail(), "tail of the last 40 lines");
            }
            other => return Err(format!("exit 1: got {other:?}").into()),
        }
        assert!(stack.fixture.work_entries().is_empty(), "after exit 1");
        match role_build(&stack, exit(ProcessStatus::Signaled(9))).await {
            Err(EngineError::BuildFailed { status, .. }) => {
                assert_eq!(status, ProcessStatus::Signaled(9));
            }
            other => return Err(format!("signal 9: got {other:?}").into()),
        }
        assert!(stack.fixture.work_entries().is_empty(), "after a signal");
        let timeout = Script {
            outcome: Outcome::Timeout,
            ..Script::default()
        };
        match role_build(&stack, timeout).await {
            Err(EngineError::Process(ProcessError::Timeout { program, .. })) => {
                assert_eq!(program, Program::Docker(DockerSubcommand::BuildxBuild));
            }
            other => return Err(format!("timeout: got {other:?}").into()),
        }
        assert!(stack.fixture.work_entries().is_empty(), "after a timeout");

        let absent = Script {
            iid: IidFile::Absent,
            ..Script::default()
        };
        unreadable(role_build(&stack, absent).await, tag)?;
        assert!(
            stack.fixture.work_entries().is_empty(),
            "after a missing ID"
        );
        let garbage = Script {
            iid: IidFile::Text("not-an-id".to_owned()),
            ..Script::default()
        };
        let output = unreadable(role_build(&stack, garbage).await, tag)?;
        assert!(output.contains("not-an-id"), "{output:?}");
        let short = Script {
            iid: IidFile::Text("sha256:abc".to_owned()),
            ..Script::default()
        };
        unreadable(role_build(&stack, short).await, tag)?;
        assert!(stack.fixture.work_entries().is_empty(), "after a bad ID");

        stack.model.script(
            BuildStage::Role,
            Script {
                lines: vec![(OutputStream::Stdout, "first".to_owned())],
                outcome: Outcome::UntilCancelled,
                ..Script::default()
            },
        );
        let (tx, mut rx) = mpsc::channel::<ImageEvent>(8);
        let cancel = CancellationToken::new();
        let build = stack.engine.build(&finals.role, &tx, &cancel);
        let driver = async {
            let event = rx.recv().await;
            assert!(matches!(event, Some(ImageEvent::Line { .. })), "{event:?}");
            cancel.cancel();
        };
        let (result, ()) = tokio::join!(build, driver);
        assert!(
            matches!(result, Err(EngineError::Cancelled { group: None })),
            "{result:?}"
        );
        assert!(stack.fixture.work_entries().is_empty(), "after a cancel");
        Ok(())
    })
    .await
}

fn standalone(
    reply: ProcessReply,
    sub: DockerSubcommand,
) -> Result<(BuildxEngine, Arc<ScriptedProcessRunner>), Box<dyn Error>> {
    let runner = Arc::new(ScriptedProcessRunner::new());
    runner.expect_docker(sub, &[], reply);
    let shared: Arc<dyn ProcessRunner> = Arc::<ScriptedProcessRunner>::clone(&runner);
    let engine = BuildxEngine::new(
        shared,
        Arc::new(StdFileSystem::default()),
        HostPath::parse(Path::new("/nonexistent-work"))?,
    );
    Ok((engine, runner))
}

fn only_call(
    runner: &ScriptedProcessRunner,
    sub: DockerSubcommand,
) -> Result<SeenCall, Box<dyn Error>> {
    let calls = runner.seen();
    assert_eq!(calls.len(), 1, "one docker call");
    assert_eq!(calls[0].program, Program::Docker(sub));
    assert!(calls[0].timeout > Duration::ZERO && calls[0].timeout <= Duration::from_secs(300));
    Ok(calls[0].clone())
}

#[tokio::test]
async fn buildx_find_reads_the_image_id_and_never_mistakes_a_failure_for_absence() -> TestResult {
    guarded(async {
        let tag = ImageTag::parse("jackin.invalid/sentinel:0123456789abcdef")?;
        let id = format!("sha256:{}", "a".repeat(64));
        let sub = DockerSubcommand::ImageInspect;
        let rows: Vec<(&str, ProcessReply)> = vec![
            (
                "found",
                ProcessReply::exited(0, format!("{id}\n").as_bytes(), b""),
            ),
            (
                "absent",
                ProcessReply::exited(1, b"", b"Error response from daemon: No such image: x\n"),
            ),
            (
                "daemon down",
                ProcessReply::exited(
                    1,
                    b"",
                    b"Cannot connect to the Docker daemon at unix:///x\n",
                ),
            ),
            ("garbage", ProcessReply::exited(0, b"garbage\n", b"")),
            ("timeout", ProcessReply::timeout()),
            ("cancelled", ProcessReply::cancelled()),
            (
                "timeout with a fault",
                ProcessReply::timeout_with(still_alive()),
            ),
            (
                "cancelled with a fault",
                ProcessReply::cancelled_with(still_alive()),
            ),
            ("group fault", ProcessReply::group(still_alive())),
        ];
        for (label, reply) in rows {
            let (engine, runner) = standalone(reply, sub)?;
            let result = engine.find(&tag).await;
            let shown = result.as_ref().err().map(ToString::to_string);
            let call = only_call(&runner, sub)?;
            assert_eq!(
                call.args.last(),
                Some(&tag.to_string()),
                "{label}: tag position"
            );
            assert!(
                call.args.iter().any(|arg| arg.contains("{{.Id}}")),
                "{label}: format"
            );
            match (label, result) {
                ("found", Ok(Some(found))) => assert_eq!(found.to_string(), id),
                ("absent", Ok(None))
                | ("cancelled", Err(EngineError::Cancelled { group: None })) => {}
                ("daemon down", Err(EngineError::DigestUnreadable { output, .. })) => {
                    assert!(output.as_str().contains("Cannot connect"), "{output:?}");
                }
                ("garbage", Err(EngineError::DigestUnreadable { .. }))
                | (
                    "timeout",
                    Err(EngineError::Process(ProcessError::Timeout { group: None, .. })),
                ) => {}
                ("cancelled with a fault", Err(EngineError::Cancelled { group })) => {
                    assert_eq!(group, Some(still_alive()), "{label}");
                    assert_eq!(
                        shown.as_deref(),
                        Some("cancelled; processes of its group were still alive after 5s")
                    );
                }
                (
                    "timeout with a fault",
                    Err(EngineError::Process(ProcessError::Timeout { group, .. })),
                ) => assert_eq!(group, Some(still_alive()), "{label}"),
                ("group fault", Err(EngineError::Process(ProcessError::Group { fault, .. }))) => {
                    assert_eq!(fault, still_alive(), "{label}");
                }
                (_, other) => return Err(format!("{label}: got {other:?}").into()),
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn buildx_check_builder_reads_the_driver_line_of_the_inspect_output() -> TestResult {
    guarded(async {
        let builder = BuilderName::parse("orbstack")?;
        let sub = DockerSubcommand::BuildxInspect;
        let inspect = |driver: &str| {
            let text = format!("Name:          orbstack\nDriver:        {driver}\n\nNodes:\nName:             orbstack\nStatus:           running\n");
            ProcessReply::exited(0, text.as_bytes(), b"")
        };
        let trap = ProcessReply::exited(
            0,
            b"Name:          docker\nDriver:        docker-container\n",
            b"",
        );
        let rows: Vec<(&str, ProcessReply)> = vec![
            ("docker", inspect("docker")),
            ("trap", trap),
            ("remote", inspect("remote")),
            ("missing", ProcessReply::exited(1, b"", b"ERROR: no builder \"x\" found\n")),
            ("other failure", ProcessReply::exited(1, b"", b"ERROR: permission denied\n")),
            ("no driver line", ProcessReply::exited(0, b"Name: orbstack\n", b"")),
        ];
        for (label, reply) in rows {
            let (engine, runner) = standalone(reply, sub)?;
            let result = engine.check_builder(&builder).await;
            let call = only_call(&runner, sub)?;
            assert_eq!(call.args.last(), Some(&builder.to_string()), "{label}: builder position");
            match (label, result) {
                ("docker", Ok(())) => {}
                ("trap", Err(EngineError::BuilderDriver { driver, .. })) => {
                    assert_eq!(driver, "docker-container");
                }
                ("remote", Err(EngineError::BuilderDriver { driver, .. })) => {
                    assert_eq!(driver, "remote");
                }
                ("missing", Err(EngineError::BuilderMissing { builder: found })) => {
                    assert_eq!(found, builder);
                }
                ("other failure" | "no driver line", Err(EngineError::BuilderUnreadable { .. })) => {}
                (_, other) => return Err(format!("{label}: got {other:?}").into()),
            }
        }
        Ok(())
    })
    .await
}

fn seed(base: &Path, name: &str, file: &str) -> TestResult {
    let dir = base.join(name);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(file), b"x")?;
    Ok(())
}

fn dead_pid() -> Result<u32, Box<dyn Error>> {
    let mut child = Command::new("true").spawn()?;
    let pid = child.id();
    child.wait()?;
    let alive = Command::new("sh")
        .arg("-c")
        .arg(format!("kill -0 {pid}"))
        .stderr(Stdio::null())
        .status()?
        .success();
    assert!(!alive, "pid {pid} is still alive, the control failed");
    Ok(pid)
}

#[test]
fn a_new_engine_removes_per_call_directories_of_dead_processes_and_nothing_else() -> TestResult {
    let dir = tempfile::tempdir()?;
    let base = dir.path().canonicalize()?;
    let work = base.join("work");
    let outside = base.join("outside");
    fs::create_dir_all(&work)?;
    fs::create_dir_all(&outside)?;
    fs::write(outside.join("sentinel"), b"keep")?;
    let dead = dead_pid()?;
    let own = std::process::id();
    let parent = std::os::unix::process::parent_id();
    seed(&work, &format!("{dead}-1/context"), "x")?;
    symlink(&outside, work.join(format!("{dead}-2")))?;
    let survivors = [
        format!("{own}-9"),
        format!("{parent}-2"),
        "1-1".to_owned(),
        "locks".to_owned(),
        "12".to_owned(),
        "-5".to_owned(),
        "ab-1".to_owned(),
    ];
    for name in &survivors {
        seed(&work, name, "x")?;
    }
    fs::write(work.join("notes.txt"), b"keep")?;
    let engine = BuildxEngine::new(
        Arc::new(ScriptedProcessRunner::new()),
        Arc::new(StdFileSystem::default()),
        HostPath::parse(&work)?,
    );
    let mut left: Vec<String> = fs::read_dir(&work)?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    let mut expected: Vec<String> = survivors.to_vec();
    expected.push("notes.txt".to_owned());
    expected.sort();
    assert_eq!(left, expected, "entries of the work directory");
    assert_eq!(fs::read(outside.join("sentinel"))?, b"keep");
    drop(engine);
    let absent = BuildxEngine::new(
        Arc::new(ScriptedProcessRunner::new()),
        Arc::new(StdFileSystem::default()),
        HostPath::parse(&base.join("absent"))?,
    );
    drop(absent);
    Ok(())
}
