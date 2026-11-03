// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Poll;
use std::time::Duration;

use jackin_core::Clock;
use jackin_openshell::{Gateway, GatewayError};
use jackin_testkit::{FakeClock, FakeGateway, Fault, ProgramScript, SandboxHandle};
use tokio::task::yield_now;
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, fake, guard, path, program_run, ready, sh};

const HOLD: &str = "alias.hold=!f() { while [ ! -e \"$1\" ]; do sleep 0.02; done; }; f";

struct Sleeper {
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
    finished: bool,
}

impl Sleeper {
    fn new(clock: &Arc<FakeClock>) -> Sleeper {
        let clock = Arc::clone(clock);
        Sleeper {
            future: Box::pin(async move {
                clock.sleep(Duration::from_secs(10)).await;
            }),
            finished: false,
        }
    }

    async fn done(&mut self) -> bool {
        if !self.finished {
            self.finished =
                poll_fn(|cx| Poll::Ready(self.future.as_mut().poll(cx).is_ready())).await;
        }
        self.finished
    }

    async fn stays_pending_for(&mut self, rounds: usize) -> bool {
        for _ in 0..rounds {
            if self.done().await {
                return false;
            }
            yield_now().await;
        }
        true
    }

    async fn finishes_within(&mut self, rounds: usize) -> bool {
        for _ in 0..rounds {
            if self.done().await {
                return true;
            }
            yield_now().await;
        }
        false
    }
}

struct ReleaseFlag(std::path::PathBuf);

impl Drop for ReleaseFlag {
    fn drop(&mut self) {
        drop(std::fs::write(&self.0, b""));
    }
}

struct ReleaseFifo {
    path: std::path::PathBuf,
    armed: bool,
}

impl ReleaseFifo {
    fn new(path: &std::path::Path) -> ReleaseFifo {
        ReleaseFifo {
            path: path.to_path_buf(),
            armed: true,
        }
    }

    async fn read(&mut self) -> TestResult {
        self.armed = false;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || std::fs::read(path)).await??;
        Ok(())
    }
}

impl Drop for ReleaseFifo {
    fn drop(&mut self) {
        if self.armed {
            let path = self.path.clone();
            drop(std::thread::spawn(move || drop(std::fs::read(path))));
        }
    }
}

async fn prepared(fake: &FakeGateway) -> Result<SandboxHandle, Box<dyn Error>> {
    let handle = ready(fake, sh("sleep 3600")?).await?;
    tokio::fs::create_dir_all(handle.root().join("project")).await?;
    handle.git(&["init", "-q"]).await?;
    Ok(handle)
}

fn linked() -> Result<(FakeGateway, Arc<FakeClock>), Box<dyn Error>> {
    let clock = Arc::new(FakeClock::new());
    let fake = fake()?;
    fake.link_clock(Arc::clone(&clock));
    Ok((fake, clock))
}

async fn blocked_git(
    handle: &SandboxHandle,
    release: &std::path::Path,
) -> Result<String, Box<dyn Error>> {
    let flag = release.to_string_lossy().into_owned();
    handle.git(&["-c", HOLD, "hold", &flag]).await
}

async fn held_until_the_git_call_returns(
    handle: SandboxHandle,
    clock: Arc<FakeClock>,
) -> TestResult {
    let release = handle.root().join("release");
    let _flag = ReleaseFlag(release.clone());
    let start = clock.now();
    let finished = AtomicBool::new(false);
    let call = async {
        let result = blocked_git(&handle, &release).await;
        finished.store(true, Ordering::SeqCst);
        result
    };
    let driver = async {
        let mut sleeping = Sleeper::new(&clock);
        assert!(
            sleeping.stays_pending_for(300).await,
            "the clock moved while a git call was in flight"
        );
        assert!(!finished.load(Ordering::SeqCst), "the git call ended early");
        assert_eq!(clock.now(), start, "time moved during the git call");
        tokio::fs::write(&release, b"").await?;
        assert!(
            sleeping.finishes_within(100_000).await,
            "the clock stayed held after the git call returned"
        );
        Ok::<_, Box<dyn Error>>(())
    };
    let (result, driven) = tokio::join!(call, driver);
    driven?;
    result?;
    Ok(())
}

#[tokio::test]
async fn a_git_call_of_a_handle_from_sandbox_holds_the_clock_until_it_returns() -> TestResult {
    guard(async {
        let (fake, clock) = linked()?;
        let handle = prepared(&fake).await?;
        held_until_the_git_call_returns(handle, clock).await
    })
    .await
}

#[tokio::test]
async fn a_git_call_of_a_handle_from_sandboxes_holds_the_clock_until_it_returns() -> TestResult {
    guard(async {
        let (fake, clock) = linked()?;
        prepared(&fake).await?;
        let handle = fake.sandboxes().into_iter().next().ok_or("no sandbox")?;
        held_until_the_git_call_returns(handle, clock).await
    })
    .await
}

#[tokio::test]
async fn a_commit_blocked_on_a_file_write_holds_the_clock_until_it_returns() -> TestResult {
    guard(async {
        let (fake, clock) = linked()?;
        let handle = prepared(&fake).await?;
        let project = handle.root().join("project");
        let fifo = project.join("blocker");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()?
                .success()
        );
        tokio::fs::write(project.join(".git/info/exclude"), b"blocker\n").await?;
        tokio::fs::write(project.join("staged.txt"), b"staged").await?;
        handle.git(&["add", "staged.txt"]).await?;
        let mut release = ReleaseFifo::new(&fifo);
        let start = clock.now();
        let finished = AtomicBool::new(false);
        let call = async {
            let result = handle
                .commit_in_project("blocker", b"content", "blocked")
                .await;
            finished.store(true, Ordering::SeqCst);
            result
        };
        let driver = async {
            let mut sleeping = Sleeper::new(&clock);
            assert!(
                sleeping.stays_pending_for(300).await,
                "the clock moved while the commit was in flight"
            );
            assert!(!finished.load(Ordering::SeqCst), "the commit ended early");
            assert_eq!(clock.now(), start, "time moved during the commit");
            release.read().await?;
            assert!(
                sleeping.finishes_within(100_000).await,
                "the clock stayed held after the commit returned"
            );
            Ok::<_, Box<dyn Error>>(())
        };
        let (result, driven) = tokio::join!(call, driver);
        driven?;
        result?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_handle_of_a_gateway_without_a_linked_clock_does_not_hold_it() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = prepared(&fake).await?;
        let clock = Arc::new(FakeClock::new());
        let release = handle.root().join("release");
        let _flag = ReleaseFlag(release.clone());
        let finished = AtomicBool::new(false);
        let call = async {
            let result = blocked_git(&handle, &release).await;
            finished.store(true, Ordering::SeqCst);
            result
        };
        let driver = async {
            let mut sleeping = Sleeper::new(&clock);
            assert!(
                sleeping.finishes_within(300).await,
                "an unlinked handle held the clock"
            );
            assert!(!finished.load(Ordering::SeqCst), "the git call ended early");
            tokio::fs::write(&release, b"").await?;
            Ok::<_, Box<dyn Error>>(())
        };
        let (result, driven) = tokio::join!(call, driver);
        driven?;
        result?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_stalled_program_run_holds_the_clock_until_the_cancel_and_an_unlinked_gateway_does_not()
-> TestResult {
    guard(async {
        for linked_clock in [true, false] {
            let clock = Arc::new(FakeClock::new());
            let fake = fake()?;
            if linked_clock {
                fake.link_clock(Arc::clone(&clock));
            }
            let handle = ready(&fake, sh("sleep 3600")?).await?;
            let hook = path("/usr/local/lib/jackin/hooks/setup")?;
            fake.script_program(
                &hook,
                ProgramScript {
                    exit_code: 0,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                },
            );
            fake.inject(Fault::StallProgram {
                program: hook.clone(),
            });
            let mut stalled = program_run(&handle.name(), hook.as_str(), &[])?;
            stalled.timeout = Duration::from_secs(3600);
            let cancel = CancellationToken::new();
            let call = fake.run_program(&stalled, &cancel);
            let driver = async {
                let mut sleeping = Sleeper::new(&clock);
                let held = sleeping.stays_pending_for(300).await;
                cancel.cancel();
                let released = sleeping.finishes_within(100_000).await;
                (held, released)
            };
            let (result, (held, released)) = tokio::join!(call, driver);
            assert!(
                matches!(result, Err(GatewayError::Cancelled { .. })),
                "{result:?}"
            );
            assert!(released, "the clock stayed held after the cancel");
            assert_eq!(
                held, linked_clock,
                "linked: {linked_clock}; the sleeper was held: {held}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn sleepers_counts_the_tasks_blocked_in_sleep_now_and_not_the_ones_that_left() -> TestResult {
    guard(async {
        let clock = Arc::new(FakeClock::new());
        assert_eq!(clock.sleepers(), 0);

        let mut first = Sleeper::new(&clock);
        assert!(!first.done().await);
        assert_eq!(clock.sleepers(), 1);
        let mut second = Sleeper::new(&clock);
        assert!(!second.done().await);
        assert_eq!(clock.sleepers(), 2);

        drop(first);
        assert_eq!(clock.sleepers(), 1, "a dropped sleep still counts");
        drop(second);
        assert_eq!(clock.sleepers(), 0, "a dropped sleep still counts");
        assert!(clock.slept().is_empty(), "a dropped sleep is not a sleep");

        let mut finishing = Sleeper::new(&clock);
        assert!(!finishing.done().await);
        assert_eq!(clock.sleepers(), 1);
        clock.advance(Duration::from_secs(10));
        assert!(finishing.finishes_within(1_000).await);
        assert_eq!(clock.sleepers(), 0, "a finished sleep still counts");
        assert_eq!(clock.slept(), [Duration::from_secs(10)]);
        Ok(())
    })
    .await
}
