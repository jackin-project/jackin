// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::{AttemptId, FileMode, SecretBytes, StartRequest, SupervisorState};
use jackin_openshell::{Gateway, PowerAction, SandboxFile};
use jackin_testkit::{FakeGateway, SupervisorModel};

use super::support::{TestResult, fake, guard, observe, path, read, ready, sh, supervised, write};

fn attempt(number: u64) -> Result<AttemptId, Box<dyn Error>> {
    Ok(AttemptId::try_from(number)?)
}

async fn state(
    fake: &FakeGateway,
    name: &jackin_core::SandboxName,
) -> Result<Option<SupervisorState>, Box<dyn Error>> {
    Ok(read(fake, name, "/sandbox/.jackin/run/state")
        .await?
        .map(|bytes| SupervisorState::parse(&String::from_utf8_lossy(&bytes)))
        .transpose()?)
}

async fn request(fake: &FakeGateway, name: &jackin_core::SandboxName, text: &str) -> TestResult {
    let file = SandboxFile {
        path: path("/sandbox/.jackin/run/start")?,
        mode: FileMode(0o600),
        content: SecretBytes::new(text.as_bytes().to_vec()),
    };
    Ok(fake.write_file(name, &file).await?)
}

async fn attempt_request(
    fake: &FakeGateway,
    name: &jackin_core::SandboxName,
    number: u64,
) -> TestResult {
    request(fake, name, &StartRequest(attempt(number)?).render()).await
}

fn running(number: u64) -> Result<SupervisorState, Box<dyn Error>> {
    Ok(SupervisorState::Running {
        attempt: attempt(number)?,
    })
}

fn exited(number: u64, code: i32) -> Result<SupervisorState, Box<dyn Error>> {
    Ok(SupervisorState::Exited {
        attempt: attempt(number)?,
        code,
    })
}

#[tokio::test]
async fn cooperative_supervisor_walks_waiting_running_exited_through_the_gateway_files()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, supervised()?).await?;
        let name = handle.name();
        assert_eq!(state(&fake, &name).await?, Some(SupervisorState::Waiting));
        attempt_request(&fake, &name, 1).await?;
        assert_eq!(state(&fake, &name).await?, Some(running(1)?));
        handle.exit_codex(3);
        assert_eq!(state(&fake, &name).await?, Some(exited(1, 3)?));
        attempt_request(&fake, &name, 2).await?;
        assert_eq!(state(&fake, &name).await?, Some(running(2)?));
        handle.exit_codex(0);
        assert_eq!(state(&fake, &name).await?, Some(exited(2, 0)?));
        assert_eq!(handle.supervisor(), Some(exited(2, 0)?));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn stale_repeated_zero_and_malformed_requests_start_nothing() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, supervised()?).await?;
        let name = handle.name();
        attempt_request(&fake, &name, 1).await?;
        handle.exit_codex(3);
        for text in ["1", "0", "abc"] {
            request(&fake, &name, text).await?;
            assert_eq!(state(&fake, &name).await?, Some(exited(1, 3)?), "{text}");
        }
        attempt_request(&fake, &name, 2).await?;
        handle.exit_codex(0);
        attempt_request(&fake, &name, 1).await?;
        assert_eq!(state(&fake, &name).await?, Some(exited(2, 0)?));
        attempt_request(&fake, &name, 3).await?;
        assert_eq!(state(&fake, &name).await?, Some(running(3)?));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_request_written_while_running_takes_effect_at_exit() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, supervised()?).await?;
        let name = handle.name();
        attempt_request(&fake, &name, 1).await?;
        attempt_request(&fake, &name, 2).await?;
        assert_eq!(state(&fake, &name).await?, Some(running(1)?));
        handle.exit_codex(5);
        assert_eq!(state(&fake, &name).await?, Some(running(2)?));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn exits_on_start_never_writes_and_plain_mains_have_no_supervisor() -> TestResult {
    guard(async {
        let exits = fake()?;
        exits.set_supervisor(SupervisorModel::ExitsOnStart { code: 3 });
        let handle = ready(&exits, supervised()?).await?;
        assert_eq!(
            state(&exits, &handle.name()).await?,
            Some(SupervisorState::Waiting)
        );
        attempt_request(&exits, &handle.name(), 1).await?;
        assert_eq!(state(&exits, &handle.name()).await?, Some(exited(1, 3)?));

        let silent = fake()?;
        silent.set_supervisor(SupervisorModel::NeverWritesState);
        let handle = ready(&silent, supervised()?).await?;
        assert_eq!(state(&silent, &handle.name()).await?, None);
        attempt_request(&silent, &handle.name(), 1).await?;
        assert_eq!(state(&silent, &handle.name()).await?, None);
        assert_eq!(handle.supervisor(), None);

        let plain = fake()?;
        let handle = ready(&plain, sh("sleep 3600")?).await?;
        assert_eq!(state(&plain, &handle.name()).await?, None);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn stop_and_start_reset_the_supervisor_to_waiting_and_keep_project_files() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, supervised()?).await?;
        let name = handle.name();
        attempt_request(&fake, &name, 1).await?;
        handle.exit_codex(0);
        attempt_request(&fake, &name, 2).await?;
        handle.exit_codex(0);
        write(&fake, &name, "/sandbox/project/f", "work").await?;
        assert_eq!(state(&fake, &name).await?, Some(exited(2, 0)?));
        fake.set_power(&name, PowerAction::Stop).await?;
        fake.set_power(&name, PowerAction::Start).await?;
        assert_eq!(
            observe(&fake, &name).await?.phase,
            jackin_core::Phase::Ready
        );
        assert_eq!(state(&fake, &name).await?, Some(SupervisorState::Waiting));
        assert_eq!(
            read(&fake, &name, "/sandbox/.jackin/run/start").await?,
            None
        );
        assert_eq!(
            read(&fake, &name, "/sandbox/project/f").await?,
            Some(b"work".to_vec())
        );
        attempt_request(&fake, &name, 1).await?;
        assert_eq!(state(&fake, &name).await?, Some(running(1)?));
        Ok(())
    })
    .await
}
