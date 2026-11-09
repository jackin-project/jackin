// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use jackin_core::{
    AttemptId, FileMode, SandboxName, SandboxPath, SecretBytes, SupervisorState, sandbox_layout,
};
use jackin_image::BuildStage;
use jackin_openshell::{Gateway, SandboxFile};
use jackin_session::{AttachTarget, LaunchFailure, LaunchLimits, LaunchOutcome};
use jackin_testkit::{Outcome, RepoFixture, Rig, RigBuilder, SandboxHandle, guard_within};

pub(crate) use jackin_testkit::{ACCESS, PLACEHOLDER, REFRESH, TestResult, launch_id};

const GUARD: Duration = Duration::from_secs(170);

pub(crate) async fn guard(work: impl Future<Output = TestResult>) -> TestResult {
    guard_within(GUARD, work).await
}

pub(crate) fn path(text: &str) -> Result<SandboxPath, Box<dyn Error>> {
    Ok(SandboxPath::parse(text)?)
}

pub(crate) fn limits(ready_secs: u64, supervisor_secs: u64) -> LaunchLimits {
    LaunchLimits {
        ready: Duration::from_secs(ready_secs),
        supervisor: Duration::from_secs(supervisor_secs),
        attempt: Duration::from_secs(20),
        hook: Duration::from_secs(600),
        upload: Duration::from_secs(600),
        create_attempts: std::num::NonZeroU8::MIN.saturating_add(2),
        bundle_bytes: jackin_session::DEFAULT_BUNDLE_BYTES,
        export_entry_bytes: jackin_session::DEFAULT_EXPORT_ENTRY_BYTES,
        export_objects: jackin_session::DEFAULT_EXPORT_OBJECTS,
        export_total_bytes: jackin_session::DEFAULT_EXPORT_TOTAL_BYTES,
    }
}

async fn dirty_repo() -> Result<RepoFixture, Box<dyn Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("src/main.rs", b"fn main() {}\n").await?;
    repo.file("README.md", b"readme\n").await?;
    repo.ignore("target/\n").await?;
    repo.commit().await?;
    repo.file("notes.txt", b"notes\n").await?;
    repo.file("target/out.bin", b"binary\n").await?;
    Ok(repo)
}

pub(crate) async fn builder() -> Result<RigBuilder, Box<dyn Error>> {
    Ok(Rig::builder()
        .limits(limits(30, 20))
        .repo(dirty_repo().await?))
}

pub(crate) async fn launch_rig() -> Result<Rig, Box<dyn Error>> {
    builder().await?.build().await
}

pub(crate) fn ready(outcome: Outcome) -> Result<AttachTarget, Box<dyn Error>> {
    match outcome.result {
        Ok(LaunchOutcome::Ready(target)) => Ok(target),
        other => Err(format!("expected a ready launch, saw {other:?}").into()),
    }
}

pub(crate) fn failure(outcome: Outcome) -> Result<LaunchFailure, Box<dyn Error>> {
    match outcome.result {
        Err(failure) => Ok(failure),
        other => Err(format!("expected a failed launch, saw {other:?}").into()),
    }
}

pub(crate) fn running(number: u64) -> Result<SupervisorState, Box<dyn Error>> {
    Ok(SupervisorState::Running {
        attempt: AttemptId::try_from(number)?,
    })
}

pub(crate) fn has_file(handle: &SandboxHandle, target: &SandboxPath) -> bool {
    handle.file(target).is_some()
}

pub(crate) fn under_project(handle: &SandboxHandle) -> bool {
    handle
        .files()
        .keys()
        .any(|file| file.as_str().starts_with("/sandbox/project"))
}

pub(crate) fn converged(rig: &Rig, handle: &SandboxHandle, id: u8) -> TestResult {
    assert_eq!(handle.supervisor(), Some(running(1)?));
    assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));
    assert!(has_file(
        handle,
        &sandbox_layout::uploaded_marker(&launch_id(id))
    ));
    assert!(has_file(
        handle,
        &sandbox_layout::setup_done_marker(&launch_id(id))
    ));
    let stages = rig.stages();
    for stage in [BuildStage::Construct, BuildStage::Derived] {
        assert_eq!(
            stages.iter().filter(|s| **s == stage).count(),
            1,
            "{stages:?}"
        );
    }
    Ok(())
}

pub(crate) async fn write(
    rig: &Rig,
    sandbox: &SandboxName,
    target: &SandboxPath,
    content: &str,
) -> TestResult {
    rig.world
        .gateway
        .write_file(
            sandbox,
            &SandboxFile {
                path: target.clone(),
                mode: FileMode(0o600),
                content: SecretBytes::new(content.as_bytes().to_vec()),
            },
        )
        .await?;
    Ok(())
}

pub(crate) fn unreachable() -> jackin_openshell::GatewayError {
    jackin_openshell::GatewayError::Protocol {
        message: "injected".to_owned(),
    }
}
