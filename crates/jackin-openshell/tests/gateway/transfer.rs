// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_core::{
    HostPath, ProcessError, ProcessStatus, Program, ProjectRoot, SandboxPath, Sha256, StdFileSystem,
};
use jackin_openshell::{Gateway, GatewayError, GatewayOp, ProjectUpload, TransferOp};
use openshell_sdk::raw::proto::SandboxPhase;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use super::exec_rpc::{Step, host_shell, scripted};
use super::fakes::{Recorded, ScriptedRunner, exited};
use super::harness::{Env, config, env, guard, resolved};
use super::server::Fault;
use super::support::{add_sandbox, sandbox_name};

const TRANSFER_WORKSPACE: &str = "jackin-ws";
const TRANSFER_SANDBOX: &str = "jackin-ws-ab12";
const STAT: &str = "/usr/bin/stat";
const SUMS: &str = "/usr/bin/sha256sum";
const FOUR_GIB: u64 = 4 * 1024 * 1024 * 1024;

async fn gateway_with(env: &Env, runner: ScriptedRunner) -> jackin_openshell::GrpcGateway {
    let resolved = resolved(env).await;
    guard(jackin_openshell::GrpcGateway::connect(config(
        env,
        resolved,
        TRANSFER_WORKSPACE,
        runner,
    )))
    .await
    .unwrap()
}

async fn project() -> (TempDir, ProjectRoot) {
    let dir = TempDir::new().unwrap();
    let status = tokio::process::Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(dir.path())
        .status()
        .await
        .unwrap();
    assert!(status.success());
    let path = HostPath::parse(dir.path()).unwrap();
    let root = ProjectRoot::open(&StdFileSystem::default(), &path)
        .await
        .unwrap();
    (dir, root)
}

fn ready_sandbox(env: &Env) -> TempDir {
    let root = TempDir::new().unwrap();
    env.server.state().exec = Some(host_shell(root.path().to_path_buf()));
    add_sandbox(
        &mut env.server.state(),
        TRANSFER_WORKSPACE,
        TRANSFER_SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
    root
}

fn words(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

fn upload_tail(tail: &[&'static str]) -> Vec<String> {
    let mut args = vec!["sandbox", "upload", TRANSFER_SANDBOX];
    args.extend_from_slice(tail);
    words(&args)
}

fn assert_scoped(call: &Recorded, endpoint: &str, tail: &[String]) {
    let position = call
        .args
        .iter()
        .position(|arg| arg == "sandbox")
        .expect("a sandbox subcommand");
    let scope: BTreeSet<(&str, &str)> = call.args[..position]
        .chunks(2)
        .map(|pair| (pair[0].as_str(), pair[1].as_str()))
        .collect();
    assert_eq!(call.args[..position].len(), 6, "{:?}", call.args);
    assert_eq!(
        scope,
        BTreeSet::from([
            ("-g", "jackin-local"),
            ("--gateway-endpoint", endpoint),
            ("--workspace", TRANSFER_WORKSPACE),
        ])
    );
    assert_eq!(&call.args[position..], tail);
    call.assert_openshell_env_isolated();
}

#[tokio::test]
async fn upload_runs_two_openshell_calls_naming_gateway_workspace_and_sandbox() {
    let env = env().await;
    let _sandbox_root = ready_sandbox(&env);
    let (dir, root) = project().await;
    let runner = ScriptedRunner::always(exited(0, b"", b""));
    let gateway = gateway_with(&env, runner.clone()).await;
    let upload = ProjectUpload {
        source: root.clone(),
        dest: SandboxPath::parse("/sandbox/project").unwrap(),
    };
    guard(gateway.upload_project(
        &sandbox_name(TRANSFER_SANDBOX),
        &upload,
        &CancellationToken::new(),
    ))
    .await
    .unwrap();
    let calls = runner.calls();
    assert_eq!(calls.len(), 2);
    for call in &calls {
        assert_eq!(call.program, Program::Openshell);
        assert_eq!(call.cwd.as_ref(), Some(root.path()));
        assert_eq!(call.cwd.as_ref().unwrap().as_path(), dir.path());
        assert!(call.stdin_null && call.has_sink);
        assert_eq!(call.timeout, Duration::from_secs(123));
        call.assert_git_env_isolated();
    }
    let endpoint = env.server.endpoint();
    assert_scoped(
        &calls[0],
        &endpoint,
        &upload_tail(&[".", "/sandbox/project"]),
    );
    assert_scoped(
        &calls[1],
        &endpoint,
        &upload_tail(&[".git", "/sandbox/project", "--no-git-ignore"]),
    );
}

#[tokio::test]
async fn upload_failures_name_the_failed_step_and_stop() {
    let env = env().await;
    let (_dir, root) = project().await;
    let upload = ProjectUpload {
        source: root,
        dest: SandboxPath::parse("/sandbox/project").unwrap(),
    };
    let name = sandbox_name(TRANSFER_SANDBOX);
    let cancel = CancellationToken::new();

    let runner = ScriptedRunner::always(exited(1, b"", b"\x1b[31mfatal\x1b[0m"));
    let gateway = gateway_with(&env, runner.clone()).await;
    let error = guard(gateway.upload_project(&name, &upload, &cancel))
        .await
        .unwrap_err();
    match error {
        GatewayError::Transfer { op, status, tail } => {
            assert_eq!(
                (op, status),
                (TransferOp::UploadFiltered, ProcessStatus::Exited(1))
            );
            assert!(!tail.as_str().contains('\u{1b}') && tail.as_str().contains("fatal"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(runner.calls().len(), 1);

    let counter = Arc::new(AtomicUsize::new(0));
    let runner = ScriptedRunner::new(move |_| {
        Ok(exited(
            if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                0
            } else {
                2
            },
            b"",
            b"boom",
        ))
    });
    let gateway = gateway_with(&env, runner).await;
    let error = guard(gateway.upload_project(&name, &upload, &cancel))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        GatewayError::Transfer {
            op: TransferOp::UploadGit,
            status: ProcessStatus::Exited(2),
            ..
        }
    ));

    let after = Duration::from_secs(9);
    for (reply, check) in [
        (
            ProcessError::Timeout {
                program: Program::Openshell,
                after,
            },
            0_u8,
        ),
        (
            ProcessError::Cancelled {
                program: Program::Openshell,
            },
            1,
        ),
        (
            ProcessError::Spawn {
                program: Program::Openshell,
                message: "missing".to_owned(),
            },
            2,
        ),
    ] {
        let reply = Arc::new(Mutex::new(Some(reply)));
        let runner = ScriptedRunner::new(move |_| Err(reply.lock().unwrap().take().unwrap()));
        let gateway = gateway_with(&env, runner).await;
        let error = guard(gateway.upload_project(&name, &upload, &cancel))
            .await
            .unwrap_err();
        match (check, error) {
            (
                0,
                GatewayError::Timeout {
                    op: GatewayOp::Upload,
                    after: got,
                },
            ) => assert_eq!(got, after),
            (
                1,
                GatewayError::Cancelled {
                    op: GatewayOp::Upload,
                },
            )
            | (2, GatewayError::Process(ProcessError::Spawn { .. })) => {}
            (_, other) => panic!("{other:?}"),
        }
    }
}

#[tokio::test]
async fn download_names_the_targets_and_verifies_the_digest() {
    let env = env().await;
    add_sandbox(
        &mut env.server.state(),
        TRANSFER_WORKSPACE,
        TRANSFER_SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
    let bytes = b"bundle bytes".to_vec();
    let digest = Sha256::of(&bytes);
    let remote_hex = Arc::new(Mutex::new(digest.as_str().to_owned()));
    let remote_exit = Arc::new(AtomicUsize::new(0));
    let (hex, exit) = (Arc::clone(&remote_hex), Arc::clone(&remote_exit));
    let size = bytes.len();
    env.server.state().exec = Some(scripted(move |start, _| {
        if start.command[0] == STAT {
            vec![Step::Out(format!("{size}\n").into_bytes()), Step::Exit(0)]
        } else if exit.load(Ordering::SeqCst) == 0 {
            vec![
                Step::Out(
                    format!("{}  /sandbox/.jackin/export.bundle\n", hex.lock().unwrap())
                        .into_bytes(),
                ),
                Step::Exit(0),
            ]
        } else {
            vec![Step::Err(b"no such file".to_vec()), Step::Exit(1)]
        }
    }));
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("export.bundle");
    let local = HostPath::parse(&destination).unwrap();
    let write_bytes = bytes.clone();
    let cli_exit = Arc::new(AtomicUsize::new(0));
    let cli = Arc::clone(&cli_exit);
    let runner = ScriptedRunner::new(move |call: &Recorded| {
        if cli.load(Ordering::SeqCst) != 0 {
            return Ok(exited(1, b"", b"download failed"));
        }
        std::fs::write(call.args.last().unwrap(), &write_bytes).unwrap();
        Ok(exited(0, b"", b""))
    });
    let gateway = gateway_with(&env, runner.clone()).await;
    let remote = SandboxPath::parse("/sandbox/.jackin/export.bundle").unwrap();
    let name = sandbox_name(TRANSFER_SANDBOX);
    let cancel = CancellationToken::new();

    guard(gateway.download_file(&name, &remote, &local, &cancel))
        .await
        .unwrap();
    let call = runner.calls().remove(0);
    let mut expected = words(&[
        "sandbox",
        "download",
        TRANSFER_SANDBOX,
        "/sandbox/.jackin/export.bundle",
    ]);
    expected.push(destination.display().to_string());
    assert_scoped(&call, &env.server.endpoint(), &expected);
    assert!(call.stdin_null && call.has_sink && call.cwd.is_none());
    assert_eq!(call.timeout, Duration::from_secs(123));

    "0".repeat(64).clone_into(&mut remote_hex.lock().unwrap());
    let error = guard(gateway.download_file(&name, &remote, &local, &cancel))
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            GatewayError::Transfer {
                op: TransferOp::Digest,
                ..
            }
        ),
        "{error:?}"
    );

    digest.as_str().clone_into(&mut remote_hex.lock().unwrap());
    remote_exit.store(1, Ordering::SeqCst);
    let error = guard(gateway.download_file(&name, &remote, &local, &cancel))
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            GatewayError::Transfer {
                op: TransferOp::Digest,
                ..
            }
        ),
        "{error:?}"
    );

    remote_exit.store(0, Ordering::SeqCst);
    cli_exit.store(1, Ordering::SeqCst);
    let error = guard(gateway.download_file(&name, &remote, &local, &cancel))
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            GatewayError::Transfer {
                op: TransferOp::Download,
                ..
            }
        ),
        "{error:?}"
    );
}

fn tree(root: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(
        base: &std::path::Path,
        dir: &std::path::Path,
        out: &mut std::collections::BTreeMap<String, Vec<u8>>,
    ) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(base).unwrap().display().to_string();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                out.insert(relative, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}

const HOST_CONFIG: &str = "
[remote \"origin\"]
\turl = https://SECRET-A:x@example.com/o/r.git
\tfetch = +refs/heads/*:refs/remotes/origin/*
[http]
\textraHeader = Authorization: Basic SECRET-B
[credential]
\thelper = store --file=SECRET-C
[user]
\temail = me@example.com
";

fn copy_git_into_sandbox(sandbox_root: std::path::PathBuf) -> ScriptedRunner {
    ScriptedRunner::new(move |call: &Recorded| {
        if call.args.iter().any(|arg| arg == "--no-git-ignore") {
            let dest = call
                .args
                .iter()
                .position(|arg| arg == ".git")
                .map(|index| call.args[index + 1].clone())
                .unwrap();
            let target = sandbox_root.join(dest.trim_start_matches('/'));
            std::fs::create_dir_all(&target).unwrap();
            let status = std::process::Command::new("cp")
                .arg("-R")
                .arg(call.cwd.as_ref().unwrap().as_path().join(".git"))
                .arg(&target)
                .status()
                .unwrap();
            assert!(status.success());
        }
        Ok(exited(0, b"", b""))
    })
}

#[tokio::test]
async fn upload_replaces_the_sandbox_git_config_with_a_sanitized_copy_at_the_destination() {
    for dest in ["/sandbox/project", "/sandbox/other"] {
        let env = env().await;
        let sandbox_root = ready_sandbox(&env);
        let (dir, root) = project().await;
        let config = dir.path().join(".git/config");
        let mut text = std::fs::read_to_string(&config).unwrap();
        text.push_str(HOST_CONFIG);
        std::fs::write(&config, text).unwrap();
        let runner = copy_git_into_sandbox(sandbox_root.path().to_path_buf());
        let gateway = gateway_with(&env, runner.clone()).await;
        let upload = ProjectUpload {
            source: root,
            dest: SandboxPath::parse(dest).unwrap(),
        };
        guard(gateway.upload_project(
            &sandbox_name(TRANSFER_SANDBOX),
            &upload,
            &CancellationToken::new(),
        ))
        .await
        .unwrap();

        let landed = sandbox_root
            .path()
            .join(dest.trim_start_matches('/'))
            .join(".git");
        let written = std::fs::read_to_string(landed.join("config")).unwrap();
        assert!(!written.contains("SECRET"), "{written}");
        for kept in [
            "me@example.com",
            "https://example.com/o/r.git",
            "+refs/heads/*:refs/remotes/origin/*",
        ] {
            assert!(written.contains(kept), "{kept} is missing from {written}");
        }
        assert!(!written.contains("extraheader") && !written.contains("credential"));
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(landed.join("config"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o644);
        }
        let mut host_tree = tree(&dir.path().join(".git"));
        let mut sandbox_tree = tree(&landed);
        assert!(host_tree.remove("config").is_some() && sandbox_tree.remove("config").is_some());
        assert_eq!(host_tree, sandbox_tree);

        assert_eq!(runner.calls().len(), 2);
        let git = runner.git_calls();
        assert_eq!(git.len(), 1);
        assert_eq!(git[0].program, Program::Git);
        assert_eq!(
            git[0].args,
            words(&[
                "config",
                "--no-includes",
                "-f",
                &config.display().to_string(),
                "--list",
                "-z",
            ])
        );
        assert!(git[0].stdin_null && git[0].cwd.is_none());
        assert_eq!(git[0].env_remove_prefixes, words(&["GIT_"]));
        assert_eq!(
            git[0].env,
            vec![
                ("GIT_CONFIG_NOSYSTEM".to_owned(), "1".to_owned()),
                ("GIT_CONFIG_GLOBAL".to_owned(), "/dev/null".to_owned()),
            ]
        );
    }
}

#[tokio::test]
async fn upload_fails_when_the_sanitized_write_fails_and_when_git_is_not_a_directory() {
    let env = env().await;
    let sandbox_root = ready_sandbox(&env);
    let (_dir, root) = project().await;
    let runner = copy_git_into_sandbox(sandbox_root.path().to_path_buf());
    let gateway = gateway_with(&env, runner.clone()).await;
    let name = sandbox_name(TRANSFER_SANDBOX);
    let upload = ProjectUpload {
        source: root,
        dest: SandboxPath::parse("/sandbox/project").unwrap(),
    };
    env.server.state().faults.insert(
        "exec_sandbox_interactive",
        Fault::Status(tonic::Code::Unavailable, "gateway lost".to_owned()),
    );
    let error = guard(gateway.upload_project(&name, &upload, &CancellationToken::new()))
        .await
        .unwrap_err();
    assert!(
        matches!(error, GatewayError::Unreachable { .. }),
        "{error:?}"
    );
    assert_eq!(runner.calls().len(), 2);
    env.server.state().faults.clear();

    let linked = TempDir::new().unwrap();
    std::fs::write(
        linked.path().join(".git"),
        "gitdir: /elsewhere/.git/worktrees/w\n",
    )
    .unwrap();
    let linked_root = ProjectRoot::open(
        &StdFileSystem::default(),
        &HostPath::parse(linked.path()).unwrap(),
    )
    .await
    .unwrap();
    let runner = ScriptedRunner::always(exited(0, b"", b""));
    let gateway = gateway_with(&env, runner.clone()).await;
    let error = guard(gateway.upload_project(
        &name,
        &ProjectUpload {
            source: linked_root,
            dest: SandboxPath::parse("/sandbox/project").unwrap(),
        },
        &CancellationToken::new(),
    ))
    .await
    .unwrap_err();
    assert!(
        matches!(error, GatewayError::UnsupportedGitDirectory { ref path } if path.as_path() == linked.path().join(".git")),
        "{error:?}"
    );
    assert!(runner.calls().is_empty() && runner.git_calls().is_empty());
}

struct Download {
    size: String,
    stat_exit: i32,
    sums_exit: i32,
    sums: String,
}

type CliReply = Result<i32, ()>;

struct Scenario {
    label: &'static str,
    size: String,
    stat_exit: i32,
    cli: CliReply,
    sums_exit: i32,
    sums_match: bool,
    cli_calls: usize,
    expect: fn(&GatewayError) -> bool,
}

fn scenarios() -> Vec<Scenario> {
    let base = |label, expect: fn(&GatewayError) -> bool| Scenario {
        label,
        size: "12".to_owned(),
        stat_exit: 0,
        cli: Ok(0),
        sums_exit: 0,
        sums_match: true,
        cli_calls: 1,
        expect,
    };
    vec![
        Scenario {
            size: (FOUR_GIB + 1).to_string(),
            cli_calls: 0,
            ..base(
                "one byte over the limit",
                |error| matches!(error, GatewayError::DownloadTooLarge { size, limit } if *size == FOUR_GIB + 1 && *limit == FOUR_GIB),
            )
        },
        Scenario {
            stat_exit: 1,
            cli_calls: 0,
            ..base("stat fails", |error| {
                matches!(
                    error,
                    GatewayError::Transfer {
                        op: TransferOp::Size,
                        status: ProcessStatus::Exited(1),
                        ..
                    }
                )
            })
        },
        Scenario {
            size: "twelve".to_owned(),
            cli_calls: 0,
            ..base("a size that is not a number", |error| {
                matches!(
                    error,
                    GatewayError::Transfer {
                        op: TransferOp::Size,
                        ..
                    }
                )
            })
        },
        Scenario {
            cli: Ok(1),
            ..base("the CLI fails after a partial file", |error| {
                matches!(
                    error,
                    GatewayError::Transfer {
                        op: TransferOp::Download,
                        status: ProcessStatus::Exited(1),
                        ..
                    }
                )
            })
        },
        Scenario {
            cli: Err(()),
            ..base("the CLI times out", |error| {
                matches!(
                    error,
                    GatewayError::Timeout {
                        op: GatewayOp::Download,
                        ..
                    }
                )
            })
        },
        Scenario {
            sums_exit: 1,
            ..base("the remote digest fails", |error| {
                matches!(
                    error,
                    GatewayError::Transfer {
                        op: TransferOp::Digest,
                        status: ProcessStatus::Exited(1),
                        ..
                    }
                )
            })
        },
        Scenario {
            sums_match: false,
            ..base("the digest does not match", |error| {
                matches!(
                    error,
                    GatewayError::Transfer {
                        op: TransferOp::Digest,
                        status: ProcessStatus::Exited(0),
                        ..
                    }
                )
            })
        },
    ]
}

fn download_backend(plan: &Arc<Mutex<Download>>) -> super::server::ExecBackend {
    let plan = Arc::clone(plan);
    scripted(move |start, _| {
        let plan = plan.lock().unwrap();
        if start.command[0] == STAT {
            if plan.stat_exit == 0 {
                vec![
                    Step::Out(format!("{}\n", plan.size).into_bytes()),
                    Step::Exit(0),
                ]
            } else {
                vec![
                    Step::Err(b"stat: no such file".to_vec()),
                    Step::Exit(plan.stat_exit),
                ]
            }
        } else {
            assert_eq!(start.command[0], SUMS);
            if plan.sums_exit == 0 {
                vec![
                    Step::Out(format!("{}  f\n", plan.sums).into_bytes()),
                    Step::Exit(0),
                ]
            } else {
                vec![
                    Step::Err(b"sha256sum: failed".to_vec()),
                    Step::Exit(plan.sums_exit),
                ]
            }
        }
    })
}

#[tokio::test]
async fn a_failed_download_leaves_no_local_file_and_the_size_limit_is_inclusive() {
    let env = env().await;
    add_sandbox(
        &mut env.server.state(),
        TRANSFER_WORKSPACE,
        TRANSFER_SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
    let bytes = b"partial or whole".to_vec();
    let good = Sha256::of(&bytes).as_str().to_owned();
    let plan = Arc::new(Mutex::new(Download {
        size: FOUR_GIB.to_string(),
        stat_exit: 0,
        sums_exit: 0,
        sums: good.clone(),
    }));
    env.server.state().exec = Some(download_backend(&plan));
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("out.bundle");
    let local = HostPath::parse(&destination).unwrap();
    let remote = SandboxPath::parse("/sandbox/.jackin/export.bundle").unwrap();
    let name = sandbox_name(TRANSFER_SANDBOX);
    let cancel = CancellationToken::new();
    let cli_reply = Arc::new(Mutex::new(Ok(0)));
    let (reply, payload) = (Arc::clone(&cli_reply), bytes.clone());
    let runner = ScriptedRunner::new(move |call: &Recorded| match *reply.lock().unwrap() {
        Ok(code) => {
            std::fs::write(call.args.last().unwrap(), &payload).unwrap();
            Ok(exited(code, b"", b"cli said no"))
        }
        Err(()) => Err(ProcessError::Timeout {
            program: Program::Openshell,
            after: Duration::from_secs(5),
        }),
    });
    let gateway = gateway_with(&env, runner.clone()).await;

    std::fs::write(&destination, b"stale").unwrap();
    guard(gateway.download_file(&name, &remote, &local, &cancel))
        .await
        .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), bytes);
    assert_eq!(runner.calls().len(), 1);

    for scenario in scenarios() {
        {
            let mut plan = plan.lock().unwrap();
            plan.size.clone_from(&scenario.size);
            plan.stat_exit = scenario.stat_exit;
            plan.sums_exit = scenario.sums_exit;
            plan.sums = if scenario.sums_match {
                good.clone()
            } else {
                "0".repeat(64)
            };
        }
        *cli_reply.lock().unwrap() = scenario.cli;
        std::fs::write(&destination, b"stale").unwrap();
        let before = runner.calls().len();
        let error = guard(gateway.download_file(&name, &remote, &local, &cancel))
            .await
            .unwrap_err();
        assert!((scenario.expect)(&error), "{}: {error:?}", scenario.label);
        assert!(
            !destination.exists(),
            "{}: the local file is left",
            scenario.label
        );
        assert_eq!(
            runner.calls().len() - before,
            scenario.cli_calls,
            "{}",
            scenario.label
        );
    }
}
