// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_core::{
    HostPath, ProcessError, ProcessStatus, Program, ProjectRoot, SandboxPath, Sha256, StdFileSystem,
};
use jackin_openshell::{Gateway, GatewayError, GatewayOp, TransferOp};
use openshell_sdk::raw::proto::SandboxPhase;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use super::exec_rpc::{Step, scripted};
use super::fakes::{Recorded, ScriptedRunner, exited};
use super::harness::{Env, config, env, guard, resolved};
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

const HOOKS_PATH: &str = "/dev/null";

fn git_in(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("/usr/bin/git")
        .args(args)
        .current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

async fn open_project(dir: &Path) -> ProjectRoot {
    let path = HostPath::parse(dir).unwrap();
    ProjectRoot::open(&StdFileSystem::default(), &path)
        .await
        .unwrap()
}

async fn project() -> (TempDir, ProjectRoot) {
    let dir = TempDir::new().unwrap();
    git_in(dir.path(), &["init", "--quiet"]);
    let root = open_project(dir.path()).await;
    (dir, root)
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

fn bundle_file(bytes: &[u8]) -> (TempDir, HostPath) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("upload.bundle");
    std::fs::write(&path, bytes).unwrap();
    let path = HostPath::parse(&path).unwrap();
    (dir, path)
}

fn sums_reply(
    hex: &Arc<Mutex<String>>,
    exit: &Arc<AtomicUsize>,
    seen: &Arc<Mutex<Vec<Vec<String>>>>,
) -> super::server::ExecBackend {
    let (hex, exit, seen) = (Arc::clone(hex), Arc::clone(exit), Arc::clone(seen));
    scripted(move |start, _| {
        seen.lock().unwrap().push(start.command.clone());
        if exit.load(Ordering::SeqCst) == 0 {
            vec![
                Step::Out(format!("{}  f\n", hex.lock().unwrap()).into_bytes()),
                Step::Exit(0),
            ]
        } else {
            vec![Step::Err(b"no such file".to_vec()), Step::Exit(1)]
        }
    })
}

#[tokio::test]
async fn upload_tree_and_upload_file_run_one_scoped_call_each_and_compare_the_file_digest() {
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
    let hex = Arc::new(Mutex::new(Sha256::of(&bytes).as_str().to_owned()));
    let exit = Arc::new(AtomicUsize::new(0));
    let seen = Arc::new(Mutex::new(Vec::new()));
    env.server.state().exec = Some(sums_reply(&hex, &exit, &seen));
    let (dir, root) = project().await;
    let (_file_dir, file) = bundle_file(&bytes);
    let runner = ScriptedRunner::always(exited(0, b"", b""));
    let gateway = gateway_with(&env, runner.clone()).await;
    let name = sandbox_name(TRANSFER_SANDBOX);
    let cancel = CancellationToken::new();
    let project_dest = SandboxPath::parse("/sandbox/project").unwrap();
    let bundle_dest = SandboxPath::parse("/sandbox/.jackin/bundle/upload-ab12.bundle").unwrap();

    guard(gateway.upload_tree(&name, &root, &project_dest, &cancel))
        .await
        .unwrap();
    assert!(seen.lock().unwrap().is_empty());
    guard(gateway.upload_file(&name, &file, &bundle_dest, &cancel))
        .await
        .unwrap();

    let calls = runner.calls();
    assert_eq!(calls.len(), 2);
    let endpoint = env.server.endpoint();
    assert_scoped(
        &calls[0],
        &endpoint,
        &upload_tail(&[".", "/sandbox/project"]),
    );
    assert_eq!(calls[0].cwd.as_ref().unwrap().as_path(), dir.path());
    let file_name = file.as_path().display().to_string();
    let mut expected = upload_tail(&[]);
    expected.extend([
        file_name,
        "/sandbox/.jackin/bundle/upload-ab12.bundle".to_owned(),
        "--no-git-ignore".to_owned(),
    ]);
    assert_scoped(&calls[1], &endpoint, &expected);
    assert!(calls[1].cwd.is_none());
    let hostile: BTreeSet<(String, String)> = [
        ("GIT_CONFIG_COUNT", "2"),
        ("GIT_CONFIG_KEY_0", "core.fsmonitor"),
        ("GIT_CONFIG_VALUE_0", "false"),
        ("GIT_CONFIG_KEY_1", "core.hooksPath"),
        ("GIT_CONFIG_VALUE_1", HOOKS_PATH),
    ]
    .map(|(name, value)| (name.to_owned(), value.to_owned()))
    .into();
    for call in &calls {
        assert_eq!(call.program, Program::Openshell);
        assert!(call.stdin_null && call.has_sink);
        assert_eq!(call.timeout, Duration::from_secs(123));
        call.assert_git_env_isolated();
        assert_eq!(call.env.iter().cloned().collect::<BTreeSet<_>>(), hostile);
        assert_eq!(call.env.len(), hostile.len());
        assert_eq!(
            call.env_remove_prefixes
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["GIT_", "OPENSHELL_"])
        );
    }
    assert_eq!(
        seen.lock().unwrap().clone(),
        vec![words(&[SUMS, "/sandbox/.jackin/bundle/upload-ab12.bundle"])]
    );

    "0".repeat(64).clone_into(&mut hex.lock().unwrap());
    let error = guard(gateway.upload_file(&name, &file, &bundle_dest, &cancel))
        .await
        .unwrap_err();
    match error {
        GatewayError::Transfer {
            op: TransferOp::Digest,
            status,
            tail,
        } => {
            assert_eq!(status, ProcessStatus::Exited(0));
            for digest in [Sha256::of(&bytes).to_string(), "0".repeat(64)] {
                assert!(tail.as_str().contains(&digest), "{tail:?}");
            }
        }
        other => panic!("{other:?}"),
    }

    exit.store(1, Ordering::SeqCst);
    let error = guard(gateway.upload_file(&name, &file, &bundle_dest, &cancel))
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            GatewayError::Transfer {
                op: TransferOp::Digest,
                status: ProcessStatus::Exited(1),
                ..
            }
        ),
        "{error:?}"
    );
}

fn model_listing(cwd: &Path, home: &Path, pairs: &[(String, String)]) -> Vec<String> {
    let output = std::process::Command::new("/usr/bin/git")
        .args(["ls-files", "-co", "--exclude-standard", "-z"])
        .current_dir(cwd)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .envs(pairs.iter().cloned())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
        .wait_with_output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut names: Vec<String> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8(name.to_vec()).unwrap())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn the_upload_env_makes_the_cli_git_ignore_a_hostile_project_config_and_keep_the_user_excludes()
 {
    let env = env().await;
    add_sandbox(
        &mut env.server.state(),
        TRANSFER_WORKSPACE,
        TRANSFER_SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
    let home = TempDir::new().unwrap();
    std::fs::write(
        home.path().join(".gitconfig"),
        format!(
            "[core]\n\texcludesFile = {}\n",
            home.path().join("ignore").display()
        ),
    )
    .unwrap();
    std::fs::write(home.path().join("ignore"), "global-ignored.txt\n").unwrap();
    let work = TempDir::new().unwrap();
    for name in ["a.txt", "u.txt", "global-ignored.txt"] {
        std::fs::write(work.path().join(name), name).unwrap();
    }
    git_in(work.path(), &["init", "--quiet"]);
    git_in(work.path(), &["add", "a.txt"]);
    let marker = home.path().join("marker");
    let hook = home.path().join("hook.sh");
    std::fs::write(&hook, format!("#!/bin/sh\ntouch {}\n", marker.display())).unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = work.path().join(".git/config");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str(&format!("[core]\n\tfsmonitor = {}\n", hook.display()));
    std::fs::write(&config, text).unwrap();
    let root = open_project(work.path()).await;

    let control = model_listing(work.path(), home.path(), &[]);
    assert!(
        marker.exists(),
        "the hostile fsmonitor did not fire in the control run: {control:?}"
    );
    std::fs::remove_file(&marker).unwrap();

    let listed = Arc::new(Mutex::new(Vec::new()));
    let (seen, cwd, home_dir) = (
        Arc::clone(&listed),
        work.path().to_path_buf(),
        home.path().to_path_buf(),
    );
    let runner = ScriptedRunner::new(move |call: &Recorded| {
        if call.args.iter().any(|arg| arg == "upload") {
            *seen.lock().unwrap() = model_listing(&cwd, &home_dir, &call.env);
        }
        Ok(exited(0, b"", b""))
    });
    let gateway = gateway_with(&env, runner).await;
    let dest = SandboxPath::parse("/sandbox/project").unwrap();

    guard(gateway.upload_tree(
        &sandbox_name(TRANSFER_SANDBOX),
        &root,
        &dest,
        &CancellationToken::new(),
    ))
    .await
    .unwrap();
    assert!(!marker.exists(), "the project fsmonitor ran");
    assert_eq!(*listed.lock().unwrap(), ["a.txt", "u.txt"]);
}

#[tokio::test]
async fn failed_tree_and_file_uploads_name_their_own_step_and_never_ask_for_a_digest() {
    let env = env().await;
    let hex = Arc::new(Mutex::new(Sha256::of(b"x").as_str().to_owned()));
    let exit = Arc::new(AtomicUsize::new(0));
    let seen = Arc::new(Mutex::new(Vec::new()));
    env.server.state().exec = Some(sums_reply(&hex, &exit, &seen));
    let (_dir, root) = project().await;
    let (_file_dir, file) = bundle_file(b"x");
    let name = sandbox_name(TRANSFER_SANDBOX);
    let cancel = CancellationToken::new();
    let dest = SandboxPath::parse("/sandbox/project").unwrap();

    let runner = ScriptedRunner::always(exited(1, b"", b"\x1b[31mfatal\x1b[0m"));
    let gateway = gateway_with(&env, runner.clone()).await;
    let tree = guard(gateway.upload_tree(&name, &root, &dest, &cancel))
        .await
        .unwrap_err();
    match tree {
        GatewayError::Transfer { op, status, tail } => {
            assert_eq!(
                (op, status),
                (TransferOp::UploadFiltered, ProcessStatus::Exited(1))
            );
            assert!(!tail.as_str().contains('\u{1b}') && tail.as_str().contains("fatal"));
        }
        other => panic!("{other:?}"),
    }
    let sent = guard(gateway.upload_file(&name, &file, &dest, &cancel))
        .await
        .unwrap_err();
    assert!(
        matches!(
            sent,
            GatewayError::Transfer {
                op: TransferOp::UploadFile,
                status: ProcessStatus::Exited(1),
                ..
            }
        ),
        "{sent:?}"
    );
    assert!(seen.lock().unwrap().is_empty());

    let after = Duration::from_secs(9);
    for (reply, check) in [
        (
            ProcessError::Timeout {
                program: Program::Openshell,
                after,
                group: None,
            },
            0_u8,
        ),
        (
            ProcessError::Cancelled {
                program: Program::Openshell,
                group: None,
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
        let error = guard(gateway.upload_file(&name, &file, &dest, &cancel))
            .await
            .unwrap_err();
        match (check, error) {
            (
                0,
                GatewayError::Timeout {
                    op: GatewayOp::UploadFile,
                    after: got,
                },
            ) => assert_eq!(got, after),
            (
                1,
                GatewayError::Cancelled {
                    op: GatewayOp::UploadFile,
                },
            )
            | (2, GatewayError::Process(ProcessError::Spawn { .. })) => {}
            (_, other) => panic!("{other:?}"),
        }
    }
    let runner = ScriptedRunner::new(|_| {
        Err(ProcessError::Cancelled {
            program: Program::Openshell,
            group: None,
        })
    });
    let gateway = gateway_with(&env, runner).await;
    let error = guard(gateway.upload_tree(&name, &root, &dest, &cancel))
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            GatewayError::Cancelled {
                op: GatewayOp::UploadTree
            }
        ),
        "{error:?}"
    );
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
    let first_temp = std::path::PathBuf::from(call.args.last().unwrap());
    assert_ne!(first_temp, destination);
    assert_eq!(first_temp.parent(), destination.parent());
    let mut expected = words(&[
        "sandbox",
        "download",
        TRANSFER_SANDBOX,
        "/sandbox/.jackin/export.bundle",
    ]);
    expected.push(first_temp.display().to_string());
    assert_scoped(&call, &env.server.endpoint(), &expected);
    assert!(call.stdin_null && call.has_sink && call.cwd.is_none());
    assert_eq!(call.timeout, Duration::from_secs(123));
    assert!(call.env.is_empty(), "{:?}", call.env);
    assert_eq!(std::fs::read(&destination).unwrap(), bytes);

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
    let second_temp = runner.calls()[1].args.last().cloned().unwrap();
    assert_ne!(second_temp, first_temp.display().to_string());

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
    assert_eq!(entries(dir.path()), ["export.bundle"]);
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

struct Download {
    size: String,
    stat_exit: i32,
    sums_exit: i32,
    sums: String,
}

#[derive(Clone, Copy)]
enum CliReply {
    Exit(i32),
    Timeout,
    Cancelled,
    Oversize,
}

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
        cli: CliReply::Exit(0),
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
            cli: CliReply::Exit(1),
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
            cli: CliReply::Timeout,
            ..base("the CLI times out after a partial file", |error| {
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
            cli: CliReply::Cancelled,
            ..base("the download is cancelled after a partial file", |error| {
                matches!(
                    error,
                    GatewayError::Cancelled {
                        op: GatewayOp::Download
                    }
                )
            })
        },
        Scenario {
            cli: CliReply::Oversize,
            sums_exit: 1,
            ..base(
                "the file grows past the limit during the download",
                |error| matches!(error, GatewayError::DownloadTooLarge { size, limit } if *size == FOUR_GIB + 1 && *limit == FOUR_GIB),
            )
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
async fn a_failed_download_never_touches_the_destination_leaves_no_temp_and_the_size_limit_is_inclusive()
 {
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
    let cli_reply = Arc::new(Mutex::new(CliReply::Exit(0)));
    let (reply, payload) = (Arc::clone(&cli_reply), bytes.clone());
    let runner = ScriptedRunner::new(move |call: &Recorded| {
        let target = call.args.last().unwrap();
        let reply = *reply.lock().unwrap();
        if matches!(reply, CliReply::Oversize) {
            let file = std::fs::File::create(target).unwrap();
            file.set_len(FOUR_GIB + 1).unwrap();
        } else {
            std::fs::write(target, &payload).unwrap();
        }
        match reply {
            CliReply::Exit(code) => Ok(exited(code, b"", b"cli said no")),
            CliReply::Oversize => Ok(exited(0, b"", b"")),
            CliReply::Timeout => Err(ProcessError::Timeout {
                program: Program::Openshell,
                after: Duration::from_secs(5),
                group: None,
            }),
            CliReply::Cancelled => Err(ProcessError::Cancelled {
                program: Program::Openshell,
                group: None,
            }),
        }
    });
    let gateway = gateway_with(&env, runner.clone()).await;

    std::fs::write(&destination, b"stale").unwrap();
    guard(gateway.download_file(&name, &remote, &local, &cancel))
        .await
        .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), bytes);
    assert_eq!(entries(dir.path()), ["out.bundle"]);
    assert_eq!(runner.calls().len(), 1);
    std::fs::remove_file(&destination).unwrap();
    guard(gateway.download_file(&name, &remote, &local, &cancel))
        .await
        .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), bytes);
    std::fs::remove_file(&destination).unwrap();

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
        for seeded in [true, false] {
            if seeded {
                std::fs::write(&destination, b"stale").unwrap();
            }
            let before = runner.calls().len();
            let error = guard(gateway.download_file(&name, &remote, &local, &cancel))
                .await
                .unwrap_err();
            let label = format!("{} (seeded: {seeded})", scenario.label);
            assert!((scenario.expect)(&error), "{label}: {error:?}");
            let expected: &[&str] = if seeded { &["out.bundle"] } else { &[] };
            assert_eq!(entries(dir.path()), expected, "{label}");
            if seeded {
                assert_eq!(std::fs::read(&destination).unwrap(), b"stale", "{label}");
                std::fs::remove_file(&destination).unwrap();
            }
            assert_eq!(runner.calls().len() - before, scenario.cli_calls, "{label}");
        }
    }
}
