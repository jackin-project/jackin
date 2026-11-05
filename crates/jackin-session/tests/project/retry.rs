// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::Path;

use jackin_core::{FileLock, FileMode, HostPath, SandboxPath, SecretBytes, sandbox_layout};
use jackin_openshell::{Gateway, GatewayOp, SandboxFile};
use jackin_session::{
    LaunchEvent, LaunchFeed, LaunchStep, LineDelivery, SkipReason, StepResult, launch_events,
};
use jackin_testkit::{Fault, RepoFixture, Trigger};
use tokio_util::sync::CancellationToken;

use super::oracle::arrived;
use super::support::{
    Outcome, Rig, TestResult, failure, global_config, guard, launch_id, mismatch, path, ready,
};

fn injected() -> jackin_openshell::GatewayError {
    jackin_openshell::GatewayError::Protocol {
        message: "injected".to_owned(),
    }
}

async fn base() -> Result<RepoFixture, Box<dyn std::error::Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    repo.file("keep.txt", b"keep\n").await?;
    repo.commit().await?;
    repo.git(&["branch", "master"]).await?;
    repo.file("u.txt", b"untracked\n").await?;
    Ok(repo)
}

fn marker() -> SandboxPath {
    sandbox_layout::uploaded_marker(&launch_id(1))
}

#[tokio::test]
async fn a_retry_after_a_host_deletion_ends_ready_without_the_deleted_file() -> TestResult {
    guard(async {
        let rig = Rig::new(base().await?).await?;
        rig.world.gateway.inject(Fault::FailAt {
            trigger: Trigger::Program(sandbox_layout::GIT.clone()),
            error: injected(),
        });
        failure(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        assert!(handle.file(&path("/sandbox/project/u.txt")?).is_none());
        assert!(handle.file(&marker()).is_none());

        rig.repo.remove("u.txt").await?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        assert!(handle.file(&path("/sandbox/project/u.txt")?).is_none());
        assert!(handle.file(&marker()).is_some());
        arrived(rig.repo.root().as_path(), &handle.root(), &[])?;

        let again = Rig::new(base().await?).await?;
        again.world.gateway.inject(Fault::FailAt {
            trigger: Trigger::Program(sandbox_layout::GIT.clone()),
            error: injected(),
        });
        failure(again.launch(1).await?)?;
        ready(again.launch(1).await?)?;
        let handle = again.handle(1).await?;
        arrived(again.repo.root().as_path(), &handle.root(), &["?? u.txt"])
    })
    .await
}

fn outside(files: &BTreeMap<SandboxPath, Vec<u8>>) -> BTreeMap<SandboxPath, Vec<u8>> {
    files
        .iter()
        .filter(|(path, _)| {
            !path.as_str().starts_with("/sandbox/project")
                && !path.as_str().contains("uploaded-")
                && !path.as_str().contains("/run/")
        })
        .map(|(path, bytes)| (path.clone(), bytes.clone()))
        .collect()
}

#[tokio::test]
async fn the_project_directory_is_emptied_but_nothing_outside_it() -> TestResult {
    guard(async {
        let rig = Rig::new(base().await?).await?;
        rig.world.gateway.inject(Fault::FailAt {
            trigger: Trigger::Op(GatewayOp::UploadTree),
            error: injected(),
        });
        failure(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        let gateway = &rig.world.gateway;
        for (target, content) in [
            ("/sandbox/project/stale/deep/file.txt", "stale"),
            ("/sandbox/project/.dotfile", "dot"),
            ("/sandbox/project/.git/HEAD", "ref: refs/heads/leftover\n"),
            ("/sandbox/outside.txt", "outside"),
            ("/sandbox/.jackin/extra", "extra"),
            ("/sandbox/.codex/note", "note"),
        ] {
            let file = SandboxFile {
                path: path(target)?,
                mode: FileMode(0o600),
                content: SecretBytes::new(content.as_bytes().to_vec()),
            };
            Gateway::write_file(gateway.as_ref(), &handle.name(), &file).await?;
        }
        let before = outside(&handle.files());
        assert!(before.len() >= 5, "{:?}", before.keys());

        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        for stale in ["stale/deep/file.txt", ".dotfile"] {
            assert!(
                !handle.root().join("project").join(stale).exists(),
                "{stale}"
            );
        }
        assert_eq!(outside(&handle.files()), before);
        arrived(rig.repo.root().as_path(), &handle.root(), &["?? u.txt"])
    })
    .await
}

const PROGRAM_RUNS: usize = 13;

#[tokio::test]
async fn the_cuts_cover_every_sandbox_program_run_of_a_clean_launch() -> TestResult {
    guard(async {
        let rig = Rig::new(base().await?).await?;
        rig.world.gateway.inject(Fault::FailAfter {
            op: GatewayOp::Program,
            calls: u32::try_from(PROGRAM_RUNS)?,
            error: injected(),
        });
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        let (mkdir, find, rm) = (
            path("/usr/bin/mkdir")?,
            path("/usr/bin/find")?,
            path("/usr/bin/rm")?,
        );
        let programs = [
            &sandbox_layout::PRINTENV,
            &sandbox_layout::GIT,
            &mkdir,
            &find,
            &rm,
        ];
        let total: usize = programs
            .iter()
            .map(|program| handle.runs(program).len())
            .sum();
        assert_eq!(total, PROGRAM_RUNS);
        arrived(rig.repo.root().as_path(), &handle.root(), &["?? u.txt"])
    })
    .await
}

async fn retries_after_a_cut_to_the_state_of_a_clean_run(cut: u32) -> TestResult {
    guard(async {
        let rig = Rig::new(base().await?).await?;
        rig.world.gateway.inject(Fault::FailAfter {
            op: GatewayOp::Program,
            calls: cut,
            error: injected(),
        });
        failure(rig.launch(1).await?)?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        assert!(handle.file(&marker()).is_some(), "cut {cut}");
        arrived(rig.repo.root().as_path(), &handle.root(), &["?? u.txt"])
    })
    .await
}

macro_rules! cut_tests {
    ($($name:ident: $cut:literal,)+) => {
        $(
            #[tokio::test]
            async fn $name() -> TestResult {
                retries_after_a_cut_to_the_state_of_a_clean_run($cut).await
            }
        )+
    };
}

cut_tests! {
    a_failure_after_0_sandbox_program_runs_retries_to_the_state_of_a_clean_run: 0,
    a_failure_after_1_sandbox_program_runs_retries_to_the_state_of_a_clean_run: 1,
    a_failure_after_2_sandbox_program_runs_retries_to_the_state_of_a_clean_run: 2,
    a_failure_after_3_sandbox_program_runs_retries_to_the_state_of_a_clean_run: 3,
    a_failure_after_4_sandbox_program_runs_retries_to_the_state_of_a_clean_run: 4,
    a_failure_after_5_sandbox_program_runs_retries_to_the_state_of_a_clean_run: 5,
    a_failure_after_6_sandbox_program_runs_retries_to_the_state_of_a_clean_run: 6,
    a_failure_after_7_sandbox_program_runs_retries_to_the_state_of_a_clean_run: 7,
    a_failure_after_8_sandbox_program_runs_retries_to_the_state_of_a_clean_run: 8,
    a_failure_after_9_sandbox_program_runs_retries_to_the_state_of_a_clean_run: 9,
    a_failure_after_10_sandbox_program_runs_retries_to_the_state_of_a_clean_run: 10,
    a_failure_after_11_sandbox_program_runs_retries_to_the_state_of_a_clean_run: 11,
    a_failure_after_12_sandbox_program_runs_retries_to_the_state_of_a_clean_run: 12,
}

#[tokio::test]
async fn no_marker_is_written_at_a_corrupt_upload_a_failing_step_or_a_mismatch() -> TestResult {
    guard(async {
        let rig = Rig::new(base().await?).await?;
        rig.world.gateway.inject(Fault::CorruptUpload);
        failure(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        assert!(handle.file(&marker()).is_none());
        ready(rig.launch(1).await?)?;
        assert!(rig.handle(1).await?.file(&marker()).is_some());

        let rig = Rig::new(base().await?).await?;
        rig.world.gateway.inject(Fault::FailAt {
            trigger: Trigger::Program(sandbox_layout::GIT.clone()),
            error: injected(),
        });
        failure(rig.launch(1).await?)?;
        assert!(rig.handle(1).await?.file(&marker()).is_none());
        ready(rig.launch(1).await?)?;
        assert!(rig.handle(1).await?.file(&marker()).is_some());

        let repo = RepoFixture::new().await?;
        repo.file(".gitattributes", b"*.dat filter=demo\n").await?;
        repo.file("data.dat", b"lower case\n").await?;
        repo.commit().await?;
        let rig = Rig::new(repo).await?;
        global_config(&rig, "[filter \"demo\"]\n\tclean = tr a-z A-Z\n")?;
        mismatch(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        assert!(handle.file(&marker()).is_none());
        assert!(!handle.root().join("project/.git").exists());
        std::fs::remove_file(rig.world.home().as_path().join(".gitconfig"))?;
        ready(rig.launch(1).await?)?;
        assert!(rig.handle(1).await?.file(&marker()).is_some());
        Ok(())
    })
    .await
}

async fn mark_as_a_peer(rig: &Rig) -> TestResult {
    let name = rig.handle(1).await?.name();
    for (target, content) in [
        ("/sandbox/project/peer.txt", "from the peer"),
        (marker().as_str(), ""),
    ] {
        let file = SandboxFile {
            path: path(target)?,
            mode: FileMode(0o600),
            content: SecretBytes::new(content.as_bytes().to_vec()),
        };
        rig.world.gateway.write_file(&name, &file).await?;
    }
    Ok(())
}

async fn as_a_peer(
    rig: &Rig,
    feed: &mut LaunchFeed,
    held: &mut Option<FileLock>,
) -> Result<Option<StepResult>, Box<dyn std::error::Error>> {
    let mut upload = None;
    while let Some(event) = feed.recv().await {
        match event {
            LaunchEvent::StepStarted(LaunchStep::Upload) => {
                mark_as_a_peer(rig).await?;
                *held = None;
            }
            LaunchEvent::StepFinished {
                step: LaunchStep::Upload,
                result,
            } => upload = Some(result),
            _ => {}
        }
    }
    Ok(upload)
}

#[tokio::test]
async fn a_launch_that_waits_for_the_upload_lock_skips_the_upload_when_a_peer_marked_it_meanwhile()
-> TestResult {
    guard(async {
        let rig = Rig::new(base().await?).await?;
        rig.world.fs.create_dir_all(rig.world.cache_dir()).await?;
        let lock = HostPath::resolve(
            Path::new(&format!("upload-{}.lock", launch_id(1))),
            rig.world.cache_dir(),
        )?;
        let mut held = Some(rig.world.fs.lock(&lock).await?);
        let (events, mut feed) = launch_events(LineDelivery::Reliable);
        let launched = rig
            .world
            .session
            .launch(rig.plan(1)?, events, CancellationToken::new());
        let peer = as_a_peer(&rig, &mut feed, &mut held);
        let (outcome, upload) = tokio::join!(launched, peer);
        ready(Outcome { result: outcome })?;
        assert_eq!(
            upload?,
            Some(StepResult::Skipped(SkipReason::AlreadyUploaded))
        );
        assert!(held.is_none());
        let handle = rig.handle(1).await?;
        assert_eq!(
            handle.file(&path("/sandbox/project/peer.txt")?),
            Some(b"from the peer".to_vec())
        );
        assert!(!handle.root().join("project/.git").exists());
        Ok(())
    })
    .await
}
