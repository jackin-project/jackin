// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin::ExitClass;
use jackin_core::{AttemptId, HookKind, Phase, SupervisorState, sandbox_layout};
use jackin_testkit::{Rig, SandboxHandle, TestResult, guard};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::support::{
    Failure, Harness, Ran, RawProbe, ScriptedTerminal, Step, failed, term_size, until,
};

const DETACH: &[u8] = b"\x10\x11";
const CTRL_D: &[u8] = b"\x04";

async fn running() -> Result<(Rig, Harness, SandboxHandle), Failure> {
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    let handle = rig.handle(1).await?;
    let harness = Harness::new(&rig.world);
    harness.write_config(&rig.config.render())?;
    Ok((rig, harness, handle))
}

fn script(steps: Vec<Step>) -> Result<(ScriptedTerminal, RawProbe), Failure> {
    ScriptedTerminal::new(steps)
}

#[tokio::test]
async fn attach_pumps_input_output_and_resizes_and_a_detach_leaves_codex_running() -> TestResult {
    guard(async {
        let (_rig, harness, handle) = running().await?;
        handle.print_main(b"codex-says");
        let size = term_size(120, 40)?;
        let (terminal, raw) = script(vec![
            Step::Bytes(b"typed".to_vec()),
            Step::Resize(size),
            Step::Bytes(DETACH.to_vec()),
        ])?;
        let name = handle.name();
        let ran = harness
            .command(&["attach", name.as_str()])
            .terminal(terminal)
            .run()
            .await?;
        ran.ok()?;
        assert_eq!(handle.main_input(), b"typed");
        assert!(ran.stdout.contains("codex-says"), "{}", ran.stdout);
        assert_eq!(handle.pty_size(), Some(size));
        assert_eq!(handle.phase(), Phase::Ready);
        assert_eq!(
            handle.supervisor(),
            Some(SupervisorState::Running {
                attempt: AttemptId::FIRST
            })
        );
        assert!(!raw.raw());

        let (terminal, raw) = script(vec![
            Step::Bytes(b"more".to_vec()),
            Step::Bytes(CTRL_D.to_vec()),
            Step::Bytes(b"after".to_vec()),
            Step::Close,
        ])?;
        harness
            .command(&["attach", name.as_str()])
            .terminal(terminal)
            .run()
            .await?
            .ok()?;
        until(|| handle.attachments() == 0).await?;
        assert_eq!(handle.main_input(), b"typedmore");
        assert_eq!(handle.phase(), Phase::Ready);
        assert!(!raw.raw());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_exited_codex_shows_its_code_filters_replies_and_restart_runs_preflight_and_attempt_two()
-> TestResult {
    guard(async {
        let (_rig, harness, handle) = running().await?;
        let name = handle.name();
        handle.exit_codex(37);
        let start = handle.file(&sandbox_layout::RUN_START);
        let (terminal, _) = script(vec![
            Step::Bytes(b"hi\x1b[12;40Ryo".to_vec()),
            Step::Bytes(DETACH.to_vec()),
        ])?;
        let shown = harness
            .command(&["attach", name.as_str()])
            .terminal(terminal)
            .run()
            .await?;
        shown.ok()?;
        assert!(shown.stderr.contains("37"), "{}", shown.stderr);
        assert_eq!(handle.main_input(), b"hiyo");
        assert_eq!(handle.file(&sandbox_layout::RUN_START), start);

        let preflight = sandbox_layout::hook_path(HookKind::Preflight);
        let runs = handle.runs(&preflight).len();
        let (terminal, _) = script(vec![Step::Bytes(DETACH.to_vec())])?;
        let args = ["attach", name.as_str(), "--restart"];
        harness
            .command(&args)
            .terminal(terminal)
            .run()
            .await?
            .ok()?;
        let second = AttemptId::FIRST.next().ok_or("no second attempt")?;
        assert_eq!(
            handle.supervisor(),
            Some(SupervisorState::Running { attempt: second })
        );
        assert!(handle.runs(&preflight).len() > runs);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_restart_on_a_running_codex_is_refused_and_a_missing_terminal_touches_nothing()
-> TestResult {
    guard(async {
        let (_rig, harness, handle) = running().await?;
        let name = handle.name();
        let start = handle.file(&sandbox_layout::RUN_START);
        let (terminal, _) = script(vec![Step::Bytes(DETACH.to_vec())])?;
        let refused = harness
            .command(&["attach", name.as_str(), "--restart"])
            .terminal(terminal)
            .run()
            .await?;
        failed(&refused, ExitClass::Refused, &[name.as_str()])?;
        assert_eq!(handle.file(&sandbox_layout::RUN_START), start);

        for args in [
            vec!["attach", name.as_str()],
            vec!["attach", name.as_str(), "--restart"],
        ] {
            failed(
                &harness.run(&args).await?,
                ExitClass::Failure,
                &["terminal"],
            )?;
        }
        assert_eq!(handle.file(&sandbox_layout::RUN_START), start);
        assert_eq!(handle.attachments(), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn attaching_an_error_phase_sandbox_names_its_exit_code_and_an_absent_one_is_refused()
-> TestResult {
    guard(async {
        let (_rig, harness, handle) = running().await?;
        let name = handle.name();
        handle.exit_main(5);
        for (target, code) in [(name.as_str(), Some("5")), ("jackin-absent-0009", None)] {
            let (terminal, raw) = script(vec![Step::Bytes(DETACH.to_vec())])?;
            let ran = harness
                .command(&["attach", target])
                .terminal(terminal)
                .run()
                .await?;
            let mut wanted = vec![target];
            wanted.extend(code);
            failed(&ran, ExitClass::Failure, &wanted)?;
            assert!(!raw.raw());
        }
        let ended = harness.run(&["inspect", name.as_str(), "--json"]).await?;
        assert!(ended.stdout.contains(r#""exit_code":5}"#));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_terminal_is_restored_after_a_session_error_and_a_cancel_detaches_with_code_130()
-> TestResult {
    guard(async {
        let (_rig, harness, handle) = running().await?;
        let name = handle.name();
        let (terminal, raw) = script(vec![])?;
        let command = harness
            .command(&["attach", name.as_str()])
            .terminal(terminal);
        let (ran, ()) = tokio::join!(command.run(), async {
            drop(until(|| handle.attachments() == 1).await);
            handle.exit_main(1);
        });
        assert_eq!(ran?.exit, ExitClass::Failure);
        assert!(!raw.raw());

        let (_rig, harness, handle) = running().await?;
        let name = handle.name();
        let (terminal, raw) = script(vec![])?;
        let cancel = CancellationToken::new();
        let command = harness
            .command(&["attach", name.as_str()])
            .terminal(terminal)
            .cancel(cancel.clone());
        let (ran, ()) = tokio::join!(command.run(), async {
            drop(until(|| handle.attachments() == 1).await);
            cancel.cancel();
        });
        let ran = ran?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(!raw.raw());
        assert!(ran.stderr.contains(name.as_str()));
        until(|| handle.attachments() == 0).await?;
        assert_eq!(handle.phase(), Phase::Ready);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn osc52_is_blocked_in_both_directions_while_other_bytes_pass() -> TestResult {
    guard(async {
        let (_rig, harness, handle) = running().await?;
        let name = handle.name();
        handle.print_main(b"x\x1b]52;c;c2VjcmV0\x07y\x1b]52;c;?\x1b\\z\x1b[1mbold");
        let (terminal, _) = script(vec![
            Step::Bytes(b"a\x1b]52;c;c2VjcmV0\x1b\\b\x1b[12;40Rc".to_vec()),
            Step::Bytes(DETACH.to_vec()),
        ])?;
        let ran = harness
            .command(&["attach", name.as_str()])
            .terminal(terminal)
            .run()
            .await?;
        ran.ok()?;
        assert!(ran.stdout.contains("xyz\x1b[1mbold"), "{:?}", ran.stdout);
        assert!(!ran.stdout.contains("]52"), "{:?}", ran.stdout);
        assert_eq!(handle.main_input(), b"ab\x1b[12;40Rc");
        Ok(())
    })
    .await
}

async fn after_ticks(
    rig: &Rig,
    harness: &Harness,
    handle: &SandboxHandle,
    ticks: usize,
    act: impl FnOnce(),
) -> Result<(Ran, RawProbe), Failure> {
    let gate = Arc::new(Notify::new());
    let (terminal, probe) = script(vec![
        Step::Gate(Arc::clone(&gate)),
        Step::Bytes(DETACH.to_vec()),
    ])?;
    let name = handle.name();
    let cancel = CancellationToken::new();
    let command = harness
        .command(&["attach", name.as_str()])
        .terminal(terminal)
        .cancel(cancel.clone());
    let clock = &rig.world.clock;
    let (ran, driven) = tokio::join!(command.run(), async {
        let driven = async {
            until(|| probe.raw()).await?;
            let before = clock.slept().len();
            act();
            until(|| clock.slept().len() >= before + ticks).await?;
            if handle.attachments() != 1 {
                return Err(Failure::from("the pump detached by itself"));
            }
            gate.notify_one();
            Ok(())
        }
        .await;
        if driven.is_err() {
            cancel.cancel();
        }
        driven
    });
    driven?;
    Ok((ran?, probe))
}

fn hints(ran: &Ran) -> usize {
    ran.stderr.matches("Codex exited with code").count()
}

#[tokio::test]
async fn a_codex_exit_while_attached_prints_one_hint_and_the_pump_keeps_running() -> TestResult {
    guard(async {
        let (rig, harness, handle) = running().await?;
        let name = handle.name();
        let (ran, raw) = after_ticks(&rig, &harness, &handle, 3, || handle.exit_codex(0)).await?;
        ran.ok()?;
        assert!(!raw.raw());
        assert_eq!(hints(&ran), 1, "{:?}", ran.stderr);
        let hint = format!(
            "\r\nCodex exited with code 0; Ctrl+D detaches, then jackin attach {name} --restart starts it again\r\n"
        );
        assert!(ran.stderr.contains(&hint), "{:?}", ran.stderr);
        assert!(!ran.stderr.contains('\u{1b}'), "{:?}", ran.stderr);
        assert!(matches!(
            handle.supervisor(),
            Some(SupervisorState::Exited { code: 0, .. })
        ));
        assert_eq!(handle.phase(), Phase::Ready);
        until(|| handle.attachments() == 0).await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_exit_before_the_attach_is_hinted_once_even_after_polls() -> TestResult {
    guard(async {
        let (rig, harness, handle) = running().await?;
        handle.exit_codex(37);
        let (ran, raw) = after_ticks(&rig, &harness, &handle, 3, || {}).await?;
        ran.ok()?;
        assert!(!raw.raw());
        assert_eq!(hints(&ran), 1, "{:?}", ran.stderr);
        assert!(ran.stderr.contains("code 37"), "{:?}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_running_codex_never_shows_the_exit_hint() -> TestResult {
    guard(async {
        let (rig, harness, handle) = running().await?;
        let (ran, raw) = after_ticks(&rig, &harness, &handle, 3, || {}).await?;
        ran.ok()?;
        assert!(!raw.raw());
        assert_eq!(hints(&ran), 0, "{:?}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_main_stream_that_ends_shows_its_exit_code_and_only_zero_is_a_success() -> TestResult {
    guard(async {
        for (code, class) in [(0, ExitClass::Ok), (7, ExitClass::Failure)] {
            let (_rig, harness, handle) = running().await?;
            let name = handle.name();
            let (terminal, raw) = script(vec![])?;
            let command = harness
                .command(&["attach", name.as_str()])
                .terminal(terminal);
            let (ran, ()) = tokio::join!(command.run(), async {
                drop(until(|| handle.attachments() == 1).await);
                handle.exit_main(code);
            });
            let ran = ran?;
            assert_eq!(ran.exit, class, "{}", ran.stderr);
            assert!(
                ran.stderr.contains(&format!("exited with code {code}")),
                "{}",
                ran.stderr
            );
            assert!(!raw.raw());
        }
        Ok(())
    })
    .await
}
