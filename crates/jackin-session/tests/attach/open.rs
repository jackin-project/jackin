// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{AttemptId, Labels, Phase, SandboxName, SupervisorReading, SupervisorState};
use jackin_openshell::{Gateway, GatewayError, InputLease, PowerAction, TerminalEvent};
use jackin_session::{AttachError, AttachRequest};
use jackin_testkit::{Rig, TestResult, guard};

use super::support::{events, owner, set_state, size, target, until};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn attach_opens_the_main_terminal_reads_the_state_at_attach_and_detach_leaves_the_sandbox()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let before = handle.supervisor();

        let mut terminal = owner(&rig, 1).await?;
        assert_eq!(
            events(&mut terminal, 2).await?,
            vec![
                TerminalEvent::Attached,
                TerminalEvent::Lease(InputLease::Owner)
            ]
        );
        let running = SupervisorState::Running {
            attempt: AttemptId::FIRST,
        };
        assert_eq!(
            terminal.supervisor_at_attach(),
            &SupervisorReading::Reported(running)
        );
        terminal.input(b"hi".to_vec()).await?;
        until(|| handle.main_input() == b"hi").await?;
        terminal.detach().await?;
        until(|| handle.attachments() == 0).await?;
        assert_eq!(handle.phase(), Phase::Ready);
        assert_eq!(handle.supervisor(), before);

        handle.exit_codex(3);
        let after_exit = owner(&rig, 1).await?;
        assert_eq!(
            after_exit.supervisor_at_attach(),
            &SupervisorReading::Reported(SupervisorState::Exited {
                attempt: AttemptId::FIRST,
                code: 3
            })
        );
        set_state(&rig, &name, "waiting\n").await?;
        assert_eq!(
            owner(&rig, 1).await?.supervisor_at_attach(),
            &SupervisorReading::Reported(SupervisorState::Waiting)
        );
        set_state(&rig, &name, "garbage words\n").await?;
        let invalid = owner(&rig, 1).await?;
        assert!(matches!(
            invalid.supervisor_at_attach(),
            SupervisorReading::Invalid(_)
        ));
        std::fs::remove_file(handle.root().join(".jackin/run/state"))?;
        assert_eq!(
            owner(&rig, 1).await?.supervisor_at_attach(),
            &SupervisorReading::Unreadable
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn attach_refuses_an_absent_a_foreign_a_stopped_and_an_ended_sandbox_without_a_terminal()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let session = &rig.world.session;

        let selected = session
            .agent_metadata_for_sandbox(&name, &rig.world.cancel)
            .await?
            .identity;
        let absent = rig.sandbox_name(9)?;
        let mut absent_identity = selected.clone();
        absent_identity.sandbox = absent.clone();
        let missing = session
            .attach(
                AttachRequest::viewer(absent_identity, size()?),
                &CancellationToken::new(),
            )
            .await;
        assert!(matches!(missing, Err(AttachError::NotFound(found)) if found == absent));

        let foreign = SandboxName::parse("jackin-ghost-0000")?;
        rig.world
            .gateway
            .insert_unmanaged(&foreign, Labels::from_pairs([])?, Phase::Ready)?;
        let mut foreign_identity = selected;
        foreign_identity.sandbox = foreign.clone();
        let ghost = session
            .attach(
                AttachRequest::viewer(foreign_identity, size()?),
                &CancellationToken::new(),
            )
            .await;
        assert!(matches!(ghost, Err(AttachError::NotFound(found)) if found == foreign));

        let request = AttachRequest::owner(target(&rig, 1).await?, size()?);
        session
            .power(
                &session
                    .managed_identity(&name, &CancellationToken::new())
                    .await?,
                PowerAction::Stop,
                &CancellationToken::new(),
            )
            .await?;
        let stopped = session
            .attach(request.clone(), &CancellationToken::new())
            .await;
        assert!(matches!(
            stopped,
            Err(AttachError::Gateway(GatewayError::NotReady {
                phase: Phase::Stopped
            }))
        ));
        for ended in [
            Phase::Completed { exit_code: 0 },
            Phase::Error { exit_code: Some(7) },
            Phase::Error { exit_code: None },
        ] {
            rig.world.gateway.force_phase(&name, ended);
            let refused = session
                .attach(request.clone(), &CancellationToken::new())
                .await;
            match refused {
                Err(AttachError::MainProcessEnded { sandbox, phase }) => {
                    assert_eq!((sandbox, phase), (name.clone(), ended));
                }
                other => return Err(format!("{ended:?}: saw {other:?}").into()),
            }
            assert_eq!(handle.attachments(), 0);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn replaced_sandbox_refuses_stale_attach_without_acquiring_a_terminal() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let old = rig.handle(1).await?;
        let stale = AttachRequest::owner(target(&rig, 1).await?, size()?);
        rig.world.gateway.delete_sandbox(&old.name()).await?;
        rig.world.clock.advance(std::time::Duration::from_secs(60));
        rig.launch(1).await?;
        let replacement = rig.handle(1).await?;
        let result = rig.world.session.attach(stale, &rig.world.cancel).await;
        assert!(
            matches!(
                result,
                Err(AttachError::AgentMetadata(
                    jackin_session::AgentMetadataError::Replaced
                ))
            ),
            "{result:?}"
        );
        assert_eq!(replacement.attachments(), 0);
        let fresh = owner(&rig, 1).await?;
        assert_eq!(replacement.attachments(), 1);
        fresh.detach().await?;
        until(|| replacement.attachments() == 0).await?;
        Ok(())
    })
    .await
}
