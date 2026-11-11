// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use jackin_core::{GroupFault, ProcessError, ProcessOutput, ProcessRunner, ProcessStatus, Program};
use jackin_session::TokioProcessRunner;
use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::support::{
    GROUP_ID, LOOP_DETACHED, Scratch, TestResult, WAIT_FOR, assert_no_leftover, guarded,
    wait_for_beat,
};

const GRACE: Duration = Duration::from_secs(5);

struct Held(Child);

impl Drop for Held {
    fn drop(&mut self) {
        drop(self.0.stdin.take());
        drop(self.0.kill());
        drop(self.0.wait());
    }
}

fn zombie_left() -> Option<GroupFault> {
    if cfg!(target_os = "linux") {
        None
    } else {
        Some(GroupFault::Alive { after: GRACE })
    }
}

async fn group_id(scratch: &Scratch) -> Result<i32, Box<dyn Error>> {
    for _ in 0..1000 {
        if let Ok(text) = std::fs::read_to_string(scratch.root().join("pgid")) {
            return Ok(text.trim().parse()?);
        }
        sleep(Duration::from_millis(10)).await;
    }
    Err("the script never wrote its group id".into())
}

async fn exited_unreaped(zombie: &Held) -> TestResult {
    let pid = Pid::from_raw(i32::try_from(zombie.0.id())?).ok_or("the zombie has no pid")?;
    for _ in 0..1000 {
        let options = WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG;
        if waitid(WaitId::Pid(pid), options)?.is_some() {
            return Ok(());
        }
        sleep(Duration::from_millis(10)).await;
    }
    Err("the zombie never exited".into())
}

fn still_unreaped(zombie: &mut Held) -> TestResult {
    match zombie.0.try_wait() {
        Ok(Some(status)) if status.success() => Ok(()),
        other => Err(format!("the runner reaped the zombie or it failed: {other:?}").into()),
    }
}

#[derive(Clone, Copy)]
enum Ending {
    Release,
    TimeOut,
    Cancel,
}

async fn run_with_a_zombie(
    ending: Ending,
) -> Result<(Result<ProcessOutput, ProcessError>, Scratch), Box<dyn Error>> {
    let scratch = Scratch::new()?;
    let body = format!("{WAIT_FOR}{LOOP_DETACHED}{GROUP_ID}wait_for \"$d/go\"\nexit 0\n");
    let mut spec = scratch.spec(&body, &[])?;
    if matches!(ending, Ending::TimeOut) {
        spec.timeout = Duration::from_secs(3);
    }
    let cancel = CancellationToken::new();
    let driver = async {
        let group = group_id(&scratch).await?;
        let zombie = Held(
            Command::new("sh")
                .args(["-c", "exit 0"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .process_group(group)
                .spawn()?,
        );
        exited_unreaped(&zombie).await?;
        wait_for_beat(scratch.root()).await?;
        match ending {
            Ending::Release => std::fs::write(scratch.root().join("go"), b"")?,
            Ending::TimeOut => {}
            Ending::Cancel => cancel.cancel(),
        }
        Ok::<_, Box<dyn Error>>(zombie)
    };
    let (result, zombie) = tokio::join!(TokioProcessRunner.run(&spec, None, &cancel), driver);
    let mut zombie = zombie?;
    still_unreaped(&mut zombie)?;
    assert_no_leftover(scratch.root()).await?;
    Ok((result, scratch))
}

#[tokio::test]
async fn a_clean_exit_that_leaves_only_a_zombie_in_the_group_is_ok_where_a_zombie_is_gone()
-> TestResult {
    guarded(async {
        let (result, _scratch) = run_with_a_zombie(Ending::Release).await?;
        match (result, zombie_left()) {
            (Ok(output), None) => assert_eq!(output.status, ProcessStatus::Exited(0)),
            (Err(ProcessError::Group { program, fault }), Some(alive)) => {
                assert_eq!(program, Program::Git);
                assert_eq!(fault, alive);
            }
            (other, expected) => {
                return Err(format!("expected {expected:?} as the fault, got {other:?}").into());
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_timeout_that_leaves_only_a_zombie_in_the_group_keeps_the_timeout() -> TestResult {
    guarded(async {
        let (result, _scratch) = run_with_a_zombie(Ending::TimeOut).await?;
        match result {
            Err(ProcessError::Timeout {
                program,
                after,
                group,
            }) => {
                assert_eq!(program, Program::Git);
                assert_eq!(after, Duration::from_secs(3));
                assert_eq!(group, zombie_left());
            }
            other => return Err(format!("expected Timeout, got {other:?}").into()),
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_that_leaves_only_a_zombie_in_the_group_keeps_the_cancel() -> TestResult {
    guarded(async {
        let (result, _scratch) = run_with_a_zombie(Ending::Cancel).await?;
        match result {
            Err(ProcessError::Cancelled { program, group }) => {
                assert_eq!(program, Program::Git);
                assert_eq!(group, zombie_left());
            }
            other => return Err(format!("expected Cancelled, got {other:?}").into()),
        }
        Ok(())
    })
    .await
}

#[cfg(target_os = "linux")]
mod hostile_neighbours {
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    use super::*;

    fn bystander(link: &Path) -> Result<Held, Box<dyn Error>> {
        Ok(Held(
            Command::new("sh")
                .args(["-c", "exec \"$0\" -c 'read _'"])
                .arg(link)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .process_group(0)
                .spawn()?,
        ))
    }

    async fn named_like(comm: &[u8], neighbour: &Held) -> TestResult {
        let mut stat = Vec::new();
        for _ in 0..500 {
            stat = std::fs::read(format!("/proc/{}/stat", neighbour.0.id()))?;
            if stat.windows(comm.len()).any(|window| window == comm) {
                return Ok(());
            }
            sleep(Duration::from_millis(10)).await;
        }
        Err(format!("the neighbour is not named {comm:?}: {stat:?}").into())
    }

    #[tokio::test]
    async fn a_run_ends_ok_and_spares_neighbours_whose_names_try_to_confuse_the_scan() -> TestResult
    {
        guarded(async {
            let scratch = Scratch::new()?;
            let body = format!("{WAIT_FOR}{LOOP_DETACHED}{GROUP_ID}wait_for \"$d/go\"\nexit 0\n");
            let spec = scratch.spec(&body, &[])?;
            let cancel = CancellationToken::new();
            let driver = async {
                let group = group_id(&scratch).await?;
                let raw = std::ffi::OsStr::from_bytes(b"\xffx\xfe");
                let odd = scratch.root().join(raw);
                std::os::unix::fs::symlink("/bin/sh", &odd)?;
                let non_utf8 = bystander(&odd)?;
                named_like(b"(\xffx\xfe)", &non_utf8).await?;
                let fake = format!("x) S 1 {group} ");
                let forged = scratch.root().join(&fake);
                std::os::unix::fs::symlink("/bin/sh", &forged)?;
                let impostor = bystander(&forged)?;
                named_like(fake.as_bytes(), &impostor).await?;
                wait_for_beat(scratch.root()).await?;
                std::fs::write(scratch.root().join("go"), b"")?;
                Ok::<_, Box<dyn Error>>((non_utf8, impostor))
            };
            let (result, neighbours) =
                tokio::join!(TokioProcessRunner.run(&spec, None, &cancel), driver);
            let (mut non_utf8, mut impostor) = neighbours?;
            let output = result?;
            assert_eq!(output.status, ProcessStatus::Exited(0));
            assert_no_leftover(scratch.root()).await?;
            for (label, neighbour) in [
                ("non-UTF-8 name", &mut non_utf8),
                ("forged name", &mut impostor),
            ] {
                assert!(
                    neighbour.0.try_wait()?.is_none(),
                    "the {label} neighbour was killed"
                );
            }
            Ok(())
        })
        .await
    }
}
