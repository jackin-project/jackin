// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{SandboxPath, sandbox_layout};
use jackin_openshell::GatewayOp;
use jackin_testkit::{Fault, RepoFixture, Trigger};

use super::oracle::arrived;
use super::support::{
    Rig, TestResult, failure, global_config, guard, launch_id, mismatch, path, ready,
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
        assert!(handle.file(&path("/sandbox/project/u.txt")?).is_some());
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
            let file = jackin_openshell::SandboxFile {
                path: path(target)?,
                mode: jackin_core::FileMode(0o600),
                content: jackin_core::SecretBytes::new(content.as_bytes().to_vec()),
            };
            jackin_openshell::Gateway::write_file(gateway.as_ref(), &handle.name(), &file).await?;
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

#[tokio::test]
async fn a_failure_at_every_sandbox_program_run_retries_to_the_state_of_a_clean_run() -> TestResult
{
    guard(async {
        let clean = Rig::new(base().await?).await?;
        ready(clean.launch(1).await?)?;
        let handle = clean.handle(1).await?;
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
        assert!(total >= 12, "{total}");
        arrived(clean.repo.root().as_path(), &handle.root(), &["?? u.txt"])?;

        for cut in 0..total {
            let rig = Rig::new(base().await?).await?;
            rig.world.gateway.inject(Fault::FailAfter {
                op: GatewayOp::Program,
                calls: u32::try_from(cut)?,
                error: injected(),
            });
            failure(rig.launch(1).await?)?;
            ready(rig.launch(1).await?)?;
            let handle = rig.handle(1).await?;
            assert!(handle.file(&marker()).is_some(), "cut {cut}");
            arrived(rig.repo.root().as_path(), &handle.root(), &["?? u.txt"])?;
        }
        Ok(())
    })
    .await
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
        assert!(handle.root().join("project/.git").is_dir());
        std::fs::remove_file(rig.world.home().as_path().join(".gitconfig"))?;
        ready(rig.launch(1).await?)?;
        assert!(rig.handle(1).await?.file(&marker()).is_some());
        Ok(())
    })
    .await
}
