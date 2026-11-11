// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use jackin_core::{
    HostPath, LaunchId, ProcessStatus, ProjectRoot, SandboxName, SandboxPath, Sha256,
};
use jackin_openshell::{Gateway, GatewayError, GatewayOp, PowerAction, TransferOp};
use jackin_testkit::{FakeGateway, Fault, Trigger};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, fake, git, guard, path, project, ready, sh, write};

const KEPT: &[u8] = b"keep-me";

fn host_git(dir: &Path, args: &[&OsStr]) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("/usr/bin/git")
        .args(args)
        .current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn index_a_non_utf8_name(dir: &Path) -> TestResult {
    let oid = host_git(
        dir,
        &[OsStr::new("rev-parse"), OsStr::new("HEAD:README.md")],
    )?;
    let entry = [
        b"100644,".as_slice(),
        oid.trim().as_bytes(),
        b",bad\xff.txt".as_slice(),
    ]
    .concat();
    host_git(
        dir,
        &[
            OsStr::new("update-index"),
            OsStr::new("--add"),
            OsStr::new("--cacheinfo"),
            OsStr::from_bytes(&entry),
        ],
    )?;
    Ok(())
}

fn names(dir: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

fn refusal(error: &GatewayError, op: TransferOp) -> Result<String, String> {
    match error {
        GatewayError::Transfer {
            op: got,
            status: ProcessStatus::Exited(1),
            tail,
        } if *got == op => Ok(tail.as_str().to_owned()),
        other => Err(format!("{other:?}")),
    }
}

#[tokio::test]
async fn an_upload_listing_a_non_utf8_name_fails_before_any_file_is_copied() -> TestResult {
    guard(async {
        let fake = fake()?;
        let cancel = CancellationToken::new();
        let dest = path("/sandbox/project")?;

        let (clean_dir, clean) = project().await?;
        tokio::fs::write(clean_dir.path().join("a.txt"), b"alpha").await?;
        let accepted = ready(&fake, sh("sleep 3600")?).await?;
        fake.upload_tree(&accepted.name(), &clean, &dest, &cancel)
            .await?;
        assert_eq!(
            accepted.file(&path("/sandbox/project/a.txt")?),
            Some(b"alpha".to_vec())
        );

        let (bad_dir, bad) = project().await?;
        tokio::fs::write(bad_dir.path().join("a.txt"), b"alpha").await?;
        index_a_non_utf8_name(bad_dir.path())?;
        let refused = ready(&fake, sh("sleep 3600")?).await?;
        let error = fake
            .upload_tree(&refused.name(), &bad, &dest, &cancel)
            .await
            .unwrap_err();
        let tail = refusal(&error, TransferOp::UploadFiltered)?;
        assert!(!tail.is_empty());
        assert_eq!(refused.files(), BTreeMap::new());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_upload_with_a_fifo_or_a_socket_in_a_listed_entry_fails_and_a_clean_nested_repository_arrives()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let cancel = CancellationToken::new();
        let dest = path("/sandbox/project")?;

        let (clean_dir, clean) = project().await?;
        git(clean_dir.path(), &["init", "-q", "nested"]).await?;
        tokio::fs::write(clean_dir.path().join("nested/f"), b"inner").await?;
        let accepted = ready(&fake, sh("sleep 3600")?).await?;
        fake.upload_tree(&accepted.name(), &clean, &dest, &cancel)
            .await?;
        assert_eq!(
            accepted.file(&path("/sandbox/project/nested/f")?),
            Some(b"inner".to_vec())
        );
        assert!(
            accepted
                .file(&path("/sandbox/project/nested/.git/HEAD")?)
                .is_some()
        );

        let (fifo_dir, fifo) = project().await?;
        tokio::fs::write(fifo_dir.path().join("a.txt"), b"alpha").await?;
        git(fifo_dir.path(), &["add", "a.txt"]).await?;
        git(fifo_dir.path(), &["commit", "-q", "-m", "a"]).await?;
        tokio::fs::remove_file(fifo_dir.path().join("a.txt")).await?;
        let made = tokio::process::Command::new("/usr/bin/mkfifo")
            .arg(fifo_dir.path().join("a.txt"))
            .status()
            .await?;
        assert!(made.success());
        let fifo_target = ready(&fake, sh("sleep 3600")?).await?;
        let error = fake
            .upload_tree(&fifo_target.name(), &fifo, &dest, &cancel)
            .await
            .unwrap_err();
        let tail = refusal(&error, TransferOp::UploadFiltered)?;
        assert!(tail.contains("unsupported file type"), "{tail}");
        assert_eq!(
            (fifo_target.files(), fifo_target.links()),
            (BTreeMap::new(), BTreeMap::new())
        );

        let (socket_dir, socket) = project().await?;
        tokio::fs::write(socket_dir.path().join("a.txt"), b"alpha").await?;
        git(socket_dir.path(), &["init", "-q", "nested"]).await?;
        tokio::fs::write(socket_dir.path().join("nested/f"), b"inner").await?;
        let listener = std::os::unix::net::UnixListener::bind(socket_dir.path().join("nested/s"))?;
        let socket_target = ready(&fake, sh("sleep 3600")?).await?;
        let error = fake
            .upload_tree(&socket_target.name(), &socket, &dest, &cancel)
            .await
            .unwrap_err();
        let tail = refusal(&error, TransferOp::UploadFiltered)?;
        assert!(tail.contains("unsupported file type"), "{tail}");
        assert_eq!(
            (socket_target.files(), socket_target.links()),
            (BTreeMap::new(), BTreeMap::new())
        );
        drop(listener);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_upload_listing_never_runs_a_hostile_project_fsmonitor() -> TestResult {
    guard(async {
        let fake = fake()?;
        let (dir, root) = project().await?;
        tokio::fs::write(dir.path().join("a.txt"), b"alpha").await?;
        let scratch = tempfile::tempdir()?;
        let marker = scratch.path().join("marker");
        let hook = scratch.path().join("hook.sh");
        tokio::fs::write(
            &hook,
            format!("#!/bin/sh\n/usr/bin/touch {}\n", marker.display()),
        )
        .await?;
        tokio::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).await?;
        let config = dir.path().join(".git/config");
        let mut text = tokio::fs::read_to_string(&config).await?;
        text.push_str(&format!("[core]\n\tfsmonitor = {}\n", hook.display()));
        tokio::fs::write(&config, text).await?;

        let listing = [
            OsStr::new("ls-files"),
            OsStr::new("-co"),
            OsStr::new("--exclude-standard"),
        ];
        host_git(dir.path(), &listing)?;
        assert!(
            marker.exists(),
            "the hostile fsmonitor did not fire in the control listing"
        );
        tokio::fs::remove_file(&marker).await?;

        let target = ready(&fake, sh("sleep 3600")?).await?;
        fake.upload_tree(
            &target.name(),
            &root,
            &path("/sandbox/project")?,
            &CancellationToken::new(),
        )
        .await?;
        assert!(
            !marker.exists(),
            "the project fsmonitor ran during the upload"
        );
        assert_eq!(
            target.file(&path("/sandbox/project/a.txt")?),
            Some(b"alpha".to_vec())
        );
        Ok(())
    })
    .await
}

struct Attempt<'a> {
    fake: &'a FakeGateway,
    root: &'a ProjectRoot,
    local: &'a HostPath,
    project_dest: &'a SandboxPath,
    file_dest: &'a SandboxPath,
}

impl Attempt<'_> {
    async fn refused(&self, name: &SandboxName) -> Result<[String; 2], String> {
        let cancel = CancellationToken::new();
        let tree = self
            .fake
            .upload_tree(name, self.root, self.project_dest, &cancel)
            .await
            .err()
            .ok_or("the tree upload succeeded")?;
        let file = self
            .fake
            .upload_file(name, self.local, self.file_dest, &cancel)
            .await
            .err()
            .ok_or("the file upload succeeded")?;
        Ok([
            refusal(&tree, TransferOp::UploadFiltered)?,
            refusal(&file, TransferOp::UploadFile)?,
        ])
    }
}

#[tokio::test]
async fn an_upload_to_a_sandbox_that_is_not_ready_says_so_an_absent_one_says_not_found_and_nothing_changes()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let (dir, root) = project().await?;
        tokio::fs::symlink("README.md", dir.path().join("link.txt")).await?;
        let project_dest = path("/sandbox/project")?;
        let file_dest = path("/sandbox/.jackin/bundle/upload.bundle")?;
        let local = HostPath::parse(&dir.path().join("payload.bin"))?;
        tokio::fs::write(local.as_path(), b"payload").await?;

        let stopped = ready(&fake, sh("sleep 3600")?).await?;
        let ended = ready(&fake, sh("sleep 3600")?).await?;
        fake.set_power(&stopped.name(), PowerAction::Stop).await?;
        ended.exit_main(3);
        let attempt = Attempt {
            fake: &fake,
            root: &root,
            local: &local,
            project_dest: &project_dest,
            file_dest: &file_dest,
        };
        let mut tails = Vec::new();
        for handle in [&stopped, &ended] {
            let (files, links) = (handle.files(), handle.links());
            tails.push(attempt.refused(&handle.name()).await?);
            assert_eq!((handle.files(), handle.links()), (files, links));
        }
        assert_eq!(tails[0], ["sandbox is not ready"; 2]);
        for tail in tails.iter().flatten() {
            assert!(tail.contains("ready") && !tail.contains("found"), "{tail}");
        }

        let absent = SandboxName::parse("jackin-absent-0001")?;
        for tail in attempt.refused(&absent).await? {
            assert!(tail.contains("found") && !tail.contains("ready"), "{tail}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_upload_reports_every_symlink_by_its_target_text_and_never_as_the_file_it_points_to()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let cancel = CancellationToken::new();
        let dest = path("/sandbox/project")?;

        let (dir, root) = project().await?;
        tokio::fs::write(dir.path().join("a.txt"), b"alpha").await?;
        tokio::fs::create_dir(dir.path().join("real")).await?;
        tokio::fs::write(dir.path().join("real/r.txt"), b"inner").await?;
        tokio::fs::symlink("a.txt", dir.path().join("link.txt")).await?;
        tokio::fs::symlink("real", dir.path().join("dirlink")).await?;
        tokio::fs::symlink("nowhere", dir.path().join("dangling")).await?;
        let linked = ready(&fake, sh("sleep 3600")?).await?;
        fake.upload_tree(&linked.name(), &root, &dest, &cancel)
            .await?;
        let expected: BTreeMap<_, _> = [
            ("/sandbox/project/link.txt", "a.txt"),
            ("/sandbox/project/dirlink", "real"),
            ("/sandbox/project/dangling", "nowhere"),
        ]
        .into_iter()
        .map(|(link, target)| Ok((path(link)?, target.to_owned())))
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;
        assert_eq!(linked.links(), expected);
        let uploaded: Vec<String> = linked
            .files()
            .keys()
            .map(|file| file.as_str().to_owned())
            .collect();
        assert_eq!(
            uploaded,
            [
                "/sandbox/project/README.md",
                "/sandbox/project/a.txt",
                "/sandbox/project/real/r.txt"
            ]
        );

        let (plain_dir, plain) = project().await?;
        tokio::fs::write(plain_dir.path().join("a.txt"), b"alpha").await?;
        let unlinked = ready(&fake, sh("sleep 3600")?).await?;
        fake.upload_tree(&unlinked.name(), &plain, &dest, &cancel)
            .await?;
        assert_eq!(unlinked.links(), BTreeMap::new());
        Ok(())
    })
    .await
}

struct Downloads<'a> {
    fake: &'a FakeGateway,
    ready: &'a SandboxName,
    stopped: &'a SandboxName,
    absent: &'a SandboxName,
    remote: SandboxPath,
    missing: SandboxPath,
    local: HostPath,
}

#[derive(Clone, Copy, Debug)]
enum Case {
    Corrupt,
    Missing,
    Stopped,
    Absent,
}

impl Downloads<'_> {
    async fn run(&self, case: Case) -> Result<Sha256, GatewayError> {
        let (name, remote) = match case {
            Case::Corrupt => {
                self.fake.inject(Fault::CorruptDownload);
                (self.ready, self.remote.clone())
            }
            Case::Missing => (self.ready, self.missing.clone()),
            Case::Stopped => (self.stopped, self.remote.clone()),
            Case::Absent => (self.absent, self.remote.clone()),
        };
        self.fake
            .download_file(name, &remote, &self.local, &CancellationToken::new())
            .await
    }
}

fn matches_case(case: Case, error: &GatewayError) -> bool {
    match case {
        Case::Corrupt => matches!(
            error,
            GatewayError::Transfer {
                op: TransferOp::Digest,
                ..
            }
        ),
        Case::Missing => matches!(
            error,
            GatewayError::Transfer {
                op: TransferOp::Size,
                status: ProcessStatus::Exited(1),
                ..
            }
        ),
        Case::Stopped => matches!(error, GatewayError::NotReady { .. }),
        Case::Absent => matches!(error, GatewayError::NotFound { .. }),
    }
}

#[tokio::test]
async fn a_failed_download_never_touches_an_existing_file_leaves_no_temp_and_a_good_one_replaces_it()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let ready_handle = ready(&fake, sh("sleep 3600")?).await?;
        let stopped_handle = ready(&fake, sh("sleep 3600")?).await?;
        let ready_name = ready_handle.name();
        let stopped_name = stopped_handle.name();
        write(&fake, &ready_name, "/sandbox/export.bin", "bundle-bytes").await?;
        fake.set_power(&stopped_name, PowerAction::Stop).await?;
        let absent = SandboxName::parse("jackin-absent-0001")?;
        let dir = tempfile::tempdir()?;
        let local = HostPath::parse(&dir.path().join("export.bin"))?;
        let downloads = Downloads {
            fake: &fake,
            ready: &ready_name,
            stopped: &stopped_name,
            absent: &absent,
            remote: path("/sandbox/export.bin")?,
            missing: path(&format!("/sandbox/{}", LaunchId::generate()?))?,
            local: local.clone(),
        };

        for case in [Case::Corrupt, Case::Missing, Case::Stopped, Case::Absent] {
            tokio::fs::write(local.as_path(), KEPT).await?;
            let error = downloads.run(case).await.unwrap_err();
            assert!(matches_case(case, &error), "{case:?}: {error:?}");
            assert_eq!(
                tokio::fs::read(local.as_path()).await.ok().as_deref(),
                Some(KEPT),
                "{case:?}"
            );
            assert_eq!(names(dir.path())?, ["export.bin"], "{case:?}");

            tokio::fs::remove_file(local.as_path()).await?;
            let error = downloads.run(case).await.unwrap_err();
            assert!(matches_case(case, &error), "{case:?}: {error:?}");
            assert_eq!(names(dir.path())?, Vec::<String>::new(), "{case:?}");
        }

        tokio::fs::write(local.as_path(), KEPT).await?;
        let digest = fake
            .download_file(
                &ready_name,
                &path("/sandbox/export.bin")?,
                &local,
                &CancellationToken::new(),
            )
            .await?;
        assert_eq!(tokio::fs::read(local.as_path()).await?, b"bundle-bytes");
        assert_eq!(digest, Sha256::of(b"bundle-bytes"));
        assert_eq!(names(dir.path())?, ["export.bin"]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_armed_at_the_download_ends_the_call_as_cancelled_and_keeps_an_existing_file()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        write(&fake, &name, "/sandbox/export.bin", "bundle-bytes").await?;
        let dir = tempfile::tempdir()?;
        let local = HostPath::parse(&dir.path().join("export.bin"))?;
        let remote = path("/sandbox/export.bin")?;

        for seeded in [true, false] {
            if seeded {
                tokio::fs::write(local.as_path(), KEPT).await?;
            }
            let cancel = CancellationToken::new();
            fake.inject(Fault::CancelAt {
                trigger: Trigger::Op(GatewayOp::Download),
                token: cancel.clone(),
            });
            let cancelled = fake.download_file(&name, &remote, &local, &cancel).await;
            assert!(
                matches!(cancelled, Err(GatewayError::Cancelled { .. })),
                "{cancelled:?}"
            );
            let expected: &[&str] = if seeded { &["export.bin"] } else { &[] };
            assert_eq!(names(dir.path())?, expected);
            if seeded {
                assert_eq!(
                    tokio::fs::read(local.as_path()).await.ok().as_deref(),
                    Some(KEPT)
                );
                tokio::fs::remove_file(local.as_path()).await?;
            }
        }

        fake.download_file(&name, &remote, &local, &CancellationToken::new())
            .await?;
        assert_eq!(tokio::fs::read(local.as_path()).await?, b"bundle-bytes");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn concurrent_uploads_of_one_project_to_one_sandbox_all_succeed_and_leave_the_project()
-> TestResult {
    guard(async {
        let fake = Arc::new(fake()?);
        let (dir, root) = project().await?;
        for index in 0..40 {
            let file = dir.path().join(format!("file-{index}.txt"));
            tokio::fs::write(&file, format!("{index}\n")).await?;
            tokio::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o444)).await?;
        }
        let dest = path("/sandbox/project")?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();

        let mut uploads = JoinSet::new();
        for _ in 0..8 {
            let fake = Arc::clone(&fake);
            let name = name.clone();
            let root = root.clone();
            let dest = dest.clone();
            uploads.spawn(async move {
                fake.upload_tree(&name, &root, &dest, &CancellationToken::new())
                    .await
            });
        }
        while let Some(joined) = uploads.join_next().await {
            joined??;
        }
        assert_eq!(
            handle.file(&path("/sandbox/project/file-39.txt")?),
            Some(b"39\n".to_vec())
        );
        assert_eq!(
            handle.file(&path("/sandbox/project/README.md")?),
            Some(b"fixture\n".to_vec())
        );
        Ok(())
    })
    .await
}
