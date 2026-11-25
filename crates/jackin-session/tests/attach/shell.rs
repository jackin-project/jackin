// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{EnvName, Labels, Phase, SandboxName, SandboxPath, sandbox_layout};
use std::time::Duration;

use jackin_openshell::{GatewayError, GatewayOp, PowerAction, TerminalEvent};
use jackin_session::{AttachError, EnvFileProblem, LaunchStep, ShellRequest};
use jackin_testkit::{Fault, Rig, TestResult, Trigger, guard, guard_within, launch_id};
use tokio_util::sync::CancellationToken;

use super::support::size;

const WITHIN: Duration = Duration::from_secs(20);
const HOSTILE: &str = "a'b \"c\" $x";

fn request(name: &SandboxName) -> Result<ShellRequest, Box<dyn std::error::Error>> {
    Ok(ShellRequest {
        sandbox: name.clone(),
        size: size()?,
    })
}

#[tokio::test]
async fn a_shell_sees_the_role_env_untouched_and_starts_in_the_project_without_touching_codex()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let env = BTreeMap::from([(EnvName::parse("GREETING")?, HOSTILE.to_owned())]);
        rig.launch_with_env(1, env).await?;
        let handle = rig.handle(1).await?;
        let before = handle.supervisor();

        let mut shell = rig
            .world
            .session
            .open_shell(request(&handle.name())?, &CancellationToken::new())
            .await?;
        shell
            .input(b"printf '%s' \"$GREETING\" > greeting.txt; touch marker; exit\n".to_vec())
            .await?;
        while let Some(event) = shell.next_event().await {
            if matches!(event, TerminalEvent::Exited(_)) {
                break;
            }
        }
        let greeting = SandboxPath::parse("/sandbox/project/greeting.txt")?;
        assert_eq!(handle.file(&greeting), Some(HOSTILE.as_bytes().to_vec()));
        let marker = SandboxPath::parse("/sandbox/project/marker")?;
        assert_eq!(handle.file(&marker), Some(Vec::new()));
        assert_eq!(handle.attachments(), 0);
        assert_eq!(handle.supervisor(), before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_shell_is_refused_without_a_usable_env_file_a_launch_label_or_a_ready_sandbox()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.gateway().inject(Fault::FailAt {
            trigger: Trigger::Write(sandbox_layout::env_file(&launch_id(1))),
            error: GatewayError::Protocol {
                message: "injected".to_owned(),
            },
        });
        let failed = rig.launch_with(1, CancellationToken::new()).await?;
        assert!(
            matches!(failed, Err(ref failure) if failure.step == LaunchStep::Credentials),
            "{failed:?}"
        );
        let name = rig.sandbox_name(1)?;
        let session = &rig.world.session;
        let missing = session
            .open_shell(request(&name)?, &CancellationToken::new())
            .await;
        assert!(
            matches!(
                missing,
                Err(AttachError::EnvFile {
                    problem: EnvFileProblem::Missing,
                    ..
                })
            ),
            "{missing:?}"
        );

        let env_path = sandbox_layout::env_file(&launch_id(1));
        rig.world
            .gateway
            .install_file(&env_path, b"not an env file\n".to_vec());
        let invalid = session
            .open_shell(request(&name)?, &CancellationToken::new())
            .await;
        assert!(
            matches!(
                invalid,
                Err(AttachError::EnvFile {
                    problem: EnvFileProblem::Invalid(_),
                    ..
                })
            ),
            "{invalid:?}"
        );

        let unlabelled = SandboxName::parse("jackin-nolnch-0001")?;
        let managed = Labels::from_pairs([("jackin.dev/managed".to_owned(), "true".to_owned())])?;
        rig.world
            .gateway
            .insert_unmanaged(&unlabelled, managed, Phase::Ready)?;
        let bare = session
            .open_shell(request(&unlabelled)?, &CancellationToken::new())
            .await;
        assert!(
            matches!(
                bare,
                Err(AttachError::EnvFile {
                    problem: EnvFileProblem::NoLaunchLabel,
                    ..
                })
            ),
            "{bare:?}"
        );

        rig.world.gateway.force_phase(&name, Phase::Ready);
        session
            .power(&name, PowerAction::Stop, &CancellationToken::new())
            .await?;
        let stopped = session
            .open_shell(request(&name)?, &CancellationToken::new())
            .await;
        assert!(
            matches!(
                stopped,
                Err(AttachError::Gateway(GatewayError::NotReady {
                    phase: Phase::Stopped
                }))
            ),
            "{stopped:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_shell_on_a_sandbox_whose_main_process_ended_is_main_process_ended() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        for ended in [
            Phase::Completed { exit_code: 0 },
            Phase::Error { exit_code: Some(7) },
            Phase::Error { exit_code: None },
        ] {
            rig.world.gateway.force_phase(&name, ended);
            let refused = rig
                .world
                .session
                .open_shell(request(&name)?, &CancellationToken::new())
                .await;
            match refused {
                Err(AttachError::MainProcessEnded { sandbox, phase }) => {
                    assert_eq!((sandbox, phase), (name.clone(), ended));
                }
                other => return Err(format!("{ended:?}: saw {other:?}").into()),
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_before_the_shell_terminal_is_requested_opens_no_terminal() -> TestResult {
    guard_within(WITHIN, async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let name = rig.sandbox_name(1)?;
        let session = &rig.world.session;

        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let before_lookup = session.open_shell(request(&name)?, &cancelled).await;
        assert!(
            matches!(
                before_lookup,
                Err(AttachError::Gateway(GatewayError::Cancelled { .. }))
            ),
            "{before_lookup:?}"
        );

        let hold = Trigger::Op(GatewayOp::ReadFiles);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let opening = session.open_shell(request(&name)?, &token);
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            token.cancel();
        };
        let (during_env_read, ()) = tokio::join!(opening, driver);
        assert!(
            matches!(
                during_env_read,
                Err(AttachError::Gateway(GatewayError::Cancelled { .. }))
            ),
            "{during_env_read:?}"
        );
        assert_eq!(rig.gateway().calls(GatewayOp::Terminal), 0);
        rig.gateway().release(&hold);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_shell_on_a_sandbox_jackin_does_not_manage_says_so() -> TestResult {
    guard_within(WITHIN, async {
        let rig = Rig::new().await?;
        let ghost = SandboxName::parse("jackin-ghost-0000")?;
        rig.gateway()
            .insert_unmanaged(&ghost, Labels::from_pairs([])?, Phase::Ready)?;
        let session = &rig.world.session;
        let refused = session
            .open_shell(request(&ghost)?, &CancellationToken::new())
            .await;
        assert!(
            matches!(&refused, Err(AttachError::NotManaged(name)) if *name == ghost),
            "{refused:?}"
        );
        let missing = SandboxName::parse("jackin-missing-0001")?;
        let absent = session
            .open_shell(request(&missing)?, &CancellationToken::new())
            .await;
        assert!(
            matches!(&absent, Err(AttachError::NotFound(name)) if *name == missing),
            "{absent:?}"
        );
        assert_eq!(rig.gateway().calls(GatewayOp::Terminal), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_while_the_shell_terminal_is_being_opened_ends_the_call() -> TestResult {
    guard_within(WITHIN, async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let name = rig.sandbox_name(1)?;
        let hold = Trigger::Op(GatewayOp::Terminal);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let opening = rig.world.session.open_shell(request(&name)?, &token);
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            token.cancel();
        };
        let (opened, ()) = tokio::join!(opening, driver);
        assert!(
            matches!(
                opened,
                Err(AttachError::Gateway(GatewayError::Cancelled { .. }))
            ),
            "{opened:?}"
        );
        rig.gateway().release(&hold);
        assert_eq!(rig.handle(1).await?.attachments(), 0);
        Ok(())
    })
    .await
}
