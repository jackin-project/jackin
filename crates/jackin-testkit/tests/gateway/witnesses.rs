// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{EnvName, Phase};
use jackin_openshell::contract::NUDGE_CASES;
use jackin_openshell::{
    EndReason, Gateway, GatewayError, InputMode, RepaintPolicy, TermSize, TerminalEvent,
    TerminalKind, TerminalRequest,
};
use jackin_testkit::{FakeGateway, SandboxHandle};
use std::time::Duration;

use super::support::{
    TestResult, attach, collect_until, drain, eventually, fake, guard, output_text, path, ready,
    sh, size, supervised, write,
};

const BANNER: &str = "echo banner; sleep 3600";

async fn banner(fake: &FakeGateway) -> Result<SandboxHandle, Box<dyn std::error::Error>> {
    ready(fake, sh(BANNER)?).await
}

fn pair(size: TermSize) -> (u16, u16) {
    (size.cols.get(), size.rows.get())
}

fn history(handle: &SandboxHandle) -> Vec<(u16, u16)> {
    handle.resizes().into_iter().map(pair).collect()
}

fn is_output(event: &TerminalEvent) -> bool {
    matches!(event, TerminalEvent::Output(_))
}

#[tokio::test]
async fn the_fake_repaint_nudge_fires_only_on_advance_for_every_shared_case() -> TestResult {
    guard(async {
        for case in NUDGE_CASES {
            let fake = fake()?;
            let handle = banner(&fake).await?;
            let policy = if case.nudge {
                RepaintPolicy::NudgeAfter(Duration::from_millis(600))
            } else {
                RepaintPolicy::Never
            };
            let session = fake
                .open_terminal(&TerminalRequest {
                    sandbox: handle.name(),
                    kind: TerminalKind::MainProcess {
                        input: InputMode::Owner,
                        repaint: policy,
                    },
                    size: size(case.open.0, case.open.1)?,
                })
                .await?;
            for &(cols, rows) in case.resize_before {
                session.resize(size(cols, rows)?).await?;
            }
            eventually(
                case.name,
                || history(&handle).len(),
                case.resize_before.len(),
            )
            .await?;
            let before = case.resize_before.to_vec();
            for _ in 0..50 {
                tokio::task::yield_now().await;
            }
            assert_eq!(history(&handle), before, "{}: nothing may fire", case.name);
            handle.advance_repaint();
            assert_eq!(history(&handle), case.windows, "{}", case.name);
            handle.advance_repaint();
            assert_eq!(
                history(&handle),
                case.windows,
                "{}: the nudge fired twice",
                case.name
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_nudge_uses_the_size_each_attachment_has_now() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = banner(&fake).await?;
        let request = |input| TerminalRequest {
            sandbox: handle.name(),
            kind: TerminalKind::MainProcess {
                input,
                repaint: RepaintPolicy::NudgeAfter(Duration::from_millis(600)),
            },
            size: size(100, 30).unwrap(),
        };
        let first = fake.open_terminal(&request(InputMode::Owner)).await?;
        let second = fake.open_terminal(&request(InputMode::Viewer)).await?;
        first.resize(size(70, 12)?).await?;
        eventually("the first resize", || history(&handle).len(), 1).await?;
        handle.advance_repaint();
        assert_eq!(
            history(&handle),
            [(70, 12), (70, 11), (70, 12), (100, 29), (100, 30)]
        );
        drop((first, second));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_resize_history_lists_each_resize_the_transport_received_but_not_the_attach_size()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = banner(&fake).await?;
        let owner = attach(&fake, &handle.name(), InputMode::Owner).await?;
        assert!(handle.resizes().is_empty());
        for wanted in [(100, 30), (80, 24), (80, 24), (120, 40)] {
            owner.resize(size(wanted.0, wanted.1)?).await?;
        }
        eventually("four resizes", || history(&handle).len(), 4).await?;
        assert_eq!(history(&handle), [(100, 30), (80, 24), (80, 24), (120, 40)]);
        assert_eq!(handle.pty_size().map(pair), Some((120, 40)));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_raw_log_keeps_the_bytes_the_fake_supervisor_swallows_and_skips_viewers() -> TestResult
{
    guard(async {
        let fake = fake()?;
        let handle = banner(&fake).await?;
        let name = handle.name();

        let mut owner = attach(&fake, &name, InputMode::Owner).await?;
        collect_until("the owner replay", &mut owner, is_output).await?;
        owner.input(vec![b'a', 0x10, 0x11]).await?;
        let ended = drain("the detach pair", &mut owner).await?;
        assert_eq!(
            ended.last(),
            Some(&TerminalEvent::Ended(EndReason::Detached))
        );
        assert_eq!(handle.main_input(), b"a");
        assert_eq!(handle.raw_input(), [b'a', 0x10, 0x11]);

        let mut again = attach(&fake, &name, InputMode::Owner).await?;
        collect_until("the second replay", &mut again, is_output).await?;
        again.input(vec![0x04]).await?;
        drain("Ctrl-D to detach", &mut again).await?;
        assert_eq!(handle.main_input(), b"a");
        assert_eq!(handle.raw_input(), [b'a', 0x10, 0x11, 0x04]);

        let viewer = attach(&fake, &name, InputMode::Viewer).await?;
        viewer.input(b"typed".to_vec()).await?;
        let mut owner = attach(&fake, &name, InputMode::Owner).await?;
        collect_until("the third replay", &mut owner, is_output).await?;
        let mut reader = attach(&fake, &name, InputMode::Owner).await?;
        reader.input(b"z".to_vec()).await?;
        collect_until("the reader lease", &mut reader, |event| {
            matches!(event, TerminalEvent::Lease(_))
        })
        .await?;
        eventually(
            "the reader bytes",
            || handle.raw_input().ends_with(b"z"),
            true,
        )
        .await?;
        assert!(!String::from_utf8_lossy(&handle.raw_input()).contains("typed"));
        assert_eq!(handle.main_input(), b"a");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_replay_window_can_start_inside_an_escape_sequence() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = banner(&fake).await?;
        handle.print_main(b"\x1b[31mred\x1b[0m");

        let mut whole = attach(&fake, &handle.name(), InputMode::Owner).await?;
        let events = collect_until("the whole replay", &mut whole, is_output).await?;
        assert_eq!(output_text(&events), "banner\r\n\x1b[31mred\x1b[0m");
        whole.detach().await?;
        drain("the whole replay to end", &mut whole).await?;

        handle.set_replay_window(Some(9));
        let mut cut = attach(&fake, &handle.name(), InputMode::Owner).await?;
        let events = collect_until("the cut replay", &mut cut, is_output).await?;
        assert_eq!(output_text(&events), "1mred\x1b[0m");
        cut.detach().await?;
        drain("the cut replay to end", &mut cut).await?;

        handle.set_replay_window(None);
        let mut full = attach(&fake, &handle.name(), InputMode::Owner).await?;
        let events = collect_until("the full replay", &mut full, is_output).await?;
        assert_eq!(output_text(&events), "banner\r\n\x1b[31mred\x1b[0m");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_twenty_first_attachment_is_refused_and_a_detach_frees_a_slot() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = banner(&fake).await?;
        let name = handle.name();
        let mut held = Vec::new();
        for _ in 0..20 {
            held.push(attach(&fake, &name, InputMode::Viewer).await?);
        }
        assert_eq!(handle.attachments(), 20);
        let refused = attach(&fake, &name, InputMode::Viewer).await;
        assert!(
            matches!(&refused, Err(error) if error.downcast_ref::<GatewayError>().is_some_and(|e| matches!(e, GatewayError::ResourceExhausted { .. }))),
            "{refused:?}"
        );
        assert_eq!(handle.attachments(), 20);

        let mut leaving = held.remove(0);
        leaving.detach().await?;
        drain("the detach", &mut leaving).await?;
        eventually("a free slot", || handle.attachments(), 19).await?;
        attach(&fake, &name, InputMode::Viewer).await?;

        fake.set_attachment_cap(2);
        let Err(error) = attach(&fake, &name, InputMode::Viewer).await else {
            return Err("a lowered cap did not refuse".into());
        };
        assert!(
            matches!(
                error.downcast_ref::<GatewayError>(),
                Some(GatewayError::ResourceExhausted { .. })
            ),
            "{error:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_stream_exit_status_follows_the_phase_code_unless_the_knob_sets_another() -> TestResult
{
    guard(async {
        let fake = fake()?;
        let plain = ready(&fake, supervised()?).await?;
        let mut session = attach(&fake, &plain.name(), InputMode::Owner).await?;
        collect_until("the lease", &mut session, |event| {
            matches!(event, TerminalEvent::Lease(_))
        })
        .await?;
        plain.exit_main(1);
        let events = drain("the plain exit", &mut session).await?;
        assert!(
            events.contains(&TerminalEvent::Exited(Some(1))),
            "{events:?}"
        );
        assert_eq!(plain.phase(), Phase::Error { exit_code: Some(1) });

        let skewed = ready(&fake, supervised()?).await?;
        skewed.set_stream_exit_status(Some(255));
        let mut session = attach(&fake, &skewed.name(), InputMode::Owner).await?;
        collect_until("the lease", &mut session, |event| {
            matches!(event, TerminalEvent::Lease(_))
        })
        .await?;
        skewed.exit_main(1);
        let events = drain("the skewed exit", &mut session).await?;
        assert!(
            events.contains(&TerminalEvent::Exited(Some(255))),
            "{events:?}"
        );
        assert!(!events.contains(&TerminalEvent::Exited(Some(1))));
        assert_eq!(skewed.phase(), Phase::Error { exit_code: Some(1) });
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_shell_witness_records_where_it_ran_what_it_was_given_and_when_it_closed() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = banner(&fake).await?;
        let name = handle.name();
        write(&fake, &name, "/sandbox/marker", "m").await?;
        assert!(handle.shells().is_empty());
        let env = BTreeMap::from([(EnvName::parse("JACKIN_PANE")?, "shell-1".to_owned())]);
        let mut shell = fake
            .open_terminal(&TerminalRequest {
                sandbox: name.clone(),
                kind: TerminalKind::Shell {
                    workdir: path("/sandbox")?,
                    env: env.clone(),
                },
                size: size(100, 30)?,
            })
            .await?;
        shell.resize(size(90, 20)?).await?;
        shell.input(b"echo hi\n".to_vec()).await?;
        eventually(
            "the shell input",
            || handle.shells().first().map(|shell| shell.input.clone()),
            Some(b"echo hi\n".to_vec()),
        )
        .await?;
        let witness = handle.shells().remove(0);
        assert_eq!(witness.workdir, path("/sandbox")?);
        assert_eq!(witness.env, env);
        assert_eq!(
            witness.sizes.iter().copied().map(pair).collect::<Vec<_>>(),
            [(100, 30), (90, 20)]
        );
        assert!(!witness.closed);

        shell.input(b"exit 0\n".to_vec()).await?;
        drain("the shell to exit", &mut shell).await?;
        eventually(
            "the shell to close",
            || handle.shells().first().map(|shell| shell.closed),
            Some(true),
        )
        .await?;
        assert_eq!(handle.shells()[0].input, b"echo hi\nexit 0\n".to_vec());
        assert_eq!(handle.attachments(), 0);
        Ok(())
    })
    .await
}
