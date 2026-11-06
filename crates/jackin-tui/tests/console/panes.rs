// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::num::NonZeroU16;
use std::sync::Arc;
use std::time::Duration;

use jackin_console::domain::instance::InstanceStatus;
use jackin_console::domain::vocabulary::{PaneWords, fill_in};
use jackin_console::panes::{PaneKind, PaneRefusal, PaneRequest};
use jackin_console::screens::Go;
use jackin_core::{AttemptId, SupervisorState};
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_openshell::{InputMode, TermSize};
use jackin_session::AttachRequest;
use jackin_testkit::{ConsoleRig, Fault, Rig, SandboxHandle, TestResult, Trigger, guard};

use super::support::Lab;

const GRID_COLS: u16 = 119;
const GRID_ROWS: u16 = 34;
const CPR_QUERY: &[u8] = b"\x1b[3;5H\x1b[6n";
const CPR_REPLY: &[u8] = b"\x1b[3;5R";
const DECRQM_QUERY: &[u8] = b"\x1b[?2004$p";
const DECRQM_REPLY: &[u8] = b"\x1b[?2004;2$y";
const KITTY_QUERY: &[u8] = b"\x1b[?u";
const KITTY_REPLY: &[u8] = b"\x1b[?0u";

fn size(cols: u16, rows: u16) -> Result<TermSize, Box<dyn Error>> {
    Ok(TermSize {
        cols: NonZeroU16::new(cols).ok_or("zero columns")?,
        rows: NonZeroU16::new(rows).ok_or("zero rows")?,
    })
}

struct Capsule {
    lab: Lab,
    id: String,
    handle: SandboxHandle,
    marks: usize,
    seen: usize,
}

async fn until(what: &str, mut done: impl FnMut() -> bool) -> TestResult {
    for _ in 0..1000 {
        if done() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(format!("never held: {what}").into())
}

impl Capsule {
    async fn open() -> Result<Capsule, Box<dyn Error>> {
        Capsule::attach(Rig::new().await?, |_| {}).await
    }

    async fn attach(
        rig: Rig,
        before: impl FnOnce(&SandboxHandle),
    ) -> Result<Capsule, Box<dyn Error>> {
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        before(&handle);
        let mut lab = Lab::with(rig).await?;
        let id = lab
            .world()
            .instances
            .first()
            .ok_or("no instance")?
            .id
            .clone();
        let mut capsule = Capsule {
            lab,
            id,
            handle,
            marks: 0,
            seen: 0,
        };
        capsule.enter();
        capsule.attached().await?;
        Ok(capsule)
    }

    fn enter(&mut self) {
        let instance = self.id.clone();
        self.lab.console.console().app().go(Go::Attach {
            instance,
            pane: None,
        });
        self.lab.console.tick(1);
    }

    async fn attached(&mut self) -> TestResult {
        let words = PaneWords::openshell();
        let waiting = fill_in(&words.attaching_title, &[("agent", "Codex")]);
        for _ in 0..80 {
            self.lab.console.pump(1).await;
            if self.handle.attachments() >= 1 && !self.lab.console.screen().contains(&waiting) {
                return Ok(());
            }
        }
        Err(format!("still attaching:\n{}", self.lab.console.screen()).into())
    }

    async fn sync(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        self.marks += 1;
        let mark = format!("SYNCMARK{}", self.marks);
        self.handle.print_main(mark.as_bytes());
        self.lab.console.wait_for_text(&mark).await?;
        let before = self.handle.raw_input().len();
        self.lab.console.type_text("~");
        let handle = self.handle.clone();
        until("the sync marker reached the sandbox input", || {
            handle.raw_input().len() > before && handle.raw_input().ends_with(b"~")
        })
        .await?;
        let mut raw = self.handle.raw_input().split_off(self.seen);
        raw.pop();
        self.seen = self.handle.raw_input().len();
        Ok(raw)
    }

    async fn feed(&mut self, bytes: &[u8]) -> TestResult {
        self.marks += 1;
        let mark = format!("FEDMARK{}", self.marks);
        self.handle.print_main(bytes);
        self.handle.print_main(mark.as_bytes());
        self.lab.console.wait_for_text(&mark).await?;
        self.lab.console.pump(2).await;
        Ok(())
    }

    async fn pump_until(&mut self, done: impl Fn(&SandboxHandle) -> bool) -> TestResult {
        for _ in 0..400 {
            if done(&self.handle) {
                return Ok(());
            }
            self.lab.console.pump(1).await;
        }
        Err(format!("the condition never held:\n{}", self.lab.console.screen()).into())
    }

    fn console(&mut self) -> &mut ConsoleRig {
        &mut self.lab.console
    }
}

#[tokio::test]
async fn the_pane_attaches_once_at_its_first_size_and_follows_the_layout() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        assert_eq!(capsule.handle.attachments(), 1);
        assert_eq!(capsule.handle.pty_size(), Some(size(GRID_COLS, GRID_ROWS)?));
        assert_eq!(capsule.handle.resizes(), Vec::new());
        capsule.console().resize(100, 30);
        capsule
            .pump_until(|handle| !handle.resizes().is_empty())
            .await?;
        assert_eq!(capsule.handle.resizes(), vec![size(99, 24)?]);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_burst_of_layout_changes_sends_the_last_size_once() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.console().resize(110, 36);
        capsule.console().resize(100, 30);
        capsule.console().resize(90, 28);
        capsule
            .pump_until(|handle| !handle.resizes().is_empty())
            .await?;
        capsule.console().pump(5).await;
        assert_eq!(capsule.handle.resizes(), vec![size(89, 22)?]);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn redraw_resizes_one_row_less_and_back() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.console().keys("ctrl+b r")?;
        capsule
            .pump_until(|handle| handle.resizes().len() == 2)
            .await?;
        assert_eq!(
            capsule.handle.resizes(),
            vec![size(GRID_COLS, GRID_ROWS - 1)?, size(GRID_COLS, GRID_ROWS)?]
        );
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn codex_output_reaches_the_screen_and_typing_reaches_codex() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.handle.print_main(b"hello from codex\r\n");
        capsule.console().wait_for_text("hello from codex").await?;
        capsule.console().type_text("ls");
        capsule.console().keys("enter")?;
        let raw = capsule.sync().await?;
        assert_eq!(raw, b"ls\r");
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn ctrl_d_never_reaches_codex_and_ctrl_b_ctrl_b_sends_the_prefix() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.console().keys("a ctrl+d ctrl+b ctrl+b b")?;
        capsule
            .pump_until(|handle| handle.raw_input().len() >= 3 || handle.raw_input().contains(&4))
            .await?;
        assert_eq!(capsule.handle.raw_input(), b"a\x02b");
        let raw = capsule.sync().await?;
        assert_eq!(raw, b"a\x02b");
        assert_eq!(capsule.handle.attachments(), 1);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn alt_ctrl_d_never_reaches_codex_either() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.console().keys("a alt+ctrl+d b")?;
        let raw = capsule.sync().await?;
        assert_eq!(raw, b"ab");
        assert_eq!(capsule.handle.attachments(), 1);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_query_reaches_a_running_codex_once_with_its_answer() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        for (query, reply) in [
            (CPR_QUERY, CPR_REPLY),
            (DECRQM_QUERY, DECRQM_REPLY),
            (KITTY_QUERY, KITTY_REPLY),
        ] {
            capsule.handle.print_main(query);
            assert_eq!(capsule.sync().await?, reply);
        }
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn no_query_is_answered_unless_codex_runs() -> TestResult {
    guard(Box::pin(async {
        for state in ["waiting", "garbage", "missing"] {
            let mut capsule = Capsule::open().await?;
            let handle = capsule.handle.clone();
            match state {
                "missing" => std::fs::remove_file(handle.root().join(".jackin/run/state"))?,
                text => {
                    std::fs::write(handle.root().join(".jackin/run/state"), format!("{text}\n"))?;
                }
            }
            capsule.handle.print_main(CPR_QUERY);
            capsule.handle.print_main(DECRQM_QUERY);
            capsule.handle.print_main(KITTY_QUERY);
            assert_eq!(capsule.sync().await?, b"", "{state}");
        }
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_query_in_the_replay_is_not_answered_but_the_next_one_is() -> TestResult {
    guard(Box::pin(async {
        let rig = Rig::new().await?;
        let mut capsule = Capsule::attach(rig, |handle| {
            handle.print_main(b"\x1b[3;5H\x1b[6nold screen\r\n");
        })
        .await?;
        capsule.console().wait_for_text("old screen").await?;
        capsule.handle.print_main(b"\x1b[7;9H\x1b[6n");
        assert_eq!(capsule.sync().await?, b"\x1b[7;9R");
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_cut_escape_sequence_at_the_start_of_the_replay_never_shows() -> TestResult {
    guard(Box::pin(async {
        let rig = Rig::new().await?;
        let mut capsule = Capsule::attach(rig, |handle| {
            handle.print_main(b"\x1b[31mred\x1b[0m done\r\n");
            handle.set_replay_window(Some(b"1mred\x1b[0m done\r\n".len()));
        })
        .await?;
        capsule.handle.advance_repaint();
        capsule
            .handle
            .print_main(b"\x1b[2J\x1b[Hrepainted screen\r\n");
        let screen = capsule.console().wait_for_text("repainted screen").await?;
        assert!(!screen.contains("1mred"), "{screen}");
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn every_attach_runs_the_redraw_resize_once() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.handle.advance_repaint();
        assert_eq!(
            capsule.handle.resizes(),
            vec![size(GRID_COLS, GRID_ROWS - 1)?, size(GRID_COLS, GRID_ROWS)?]
        );
        capsule.console().keys("ctrl+b d")?;
        capsule
            .pump_until(|handle| handle.attachments() == 0)
            .await?;
        capsule.enter();
        capsule.console().keys("enter")?;
        capsule.attached().await?;
        capsule.handle.advance_repaint();
        assert_eq!(capsule.handle.resizes().len(), 4);
        assert_eq!(capsule.handle.pty_size(), Some(size(GRID_COLS, GRID_ROWS)?));
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_shell_pane_gets_replies_and_closing_it_ends_the_shell() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        let instance = capsule.id.clone();
        capsule.console().console().app().go(Go::NewSession {
            instance,
            agent: None,
            account: None,
        });
        capsule
            .pump_until(|handle| handle.shells().len() == 1)
            .await?;
        capsule.console().pump(12).await;
        capsule.console().type_text("printf '\\033[c'");
        capsule.console().keys("ctrl+j")?;
        capsule
            .pump_until(|handle| {
                let shells = handle.shells();
                String::from_utf8_lossy(&shells[0].input).contains("\u{1b}[?")
            })
            .await?;
        assert!(!capsule.handle.shells()[0].closed);
        capsule.console().keys("ctrl+b x")?;
        capsule.console().keys("right enter")?;
        capsule
            .pump_until(|handle| handle.shells()[0].closed)
            .await?;
        assert_eq!(capsule.handle.attachments(), 1);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn leaving_the_capsule_detaches_codex_and_keeps_the_shell() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        let instance = capsule.id.clone();
        capsule.console().console().app().go(Go::NewSession {
            instance,
            agent: None,
            account: None,
        });
        capsule
            .pump_until(|handle| handle.shells().len() == 1)
            .await?;
        capsule.console().keys("ctrl+b n")?;
        capsule.console().pump(10).await;
        assert_eq!(capsule.handle.attachments(), 1);
        capsule.console().keys("ctrl+b d")?;
        capsule
            .pump_until(|handle| handle.attachments() == 0)
            .await?;
        capsule.console().pump(5).await;
        assert!(!capsule.handle.shells()[0].closed);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_stalled_supervisor_read_does_not_hold_back_a_detach() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        let gateway = Arc::clone(&capsule.lab.rig.world.gateway);
        let held = Trigger::Op(GatewayOp::ReadFiles);
        gateway.inject(Fault::HoldAt {
            trigger: held.clone(),
        });
        assert!(gateway.until_held(&held).await);
        capsule.console().keys("ctrl+b d")?;
        let detached = capsule.pump_until(|handle| handle.attachments() == 0).await;
        gateway.release(&held);
        detached
    }))
    .await
}

#[tokio::test]
async fn enter_reattaches_a_detached_codex_and_the_replay_shows_once() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.handle.print_main(b"one screen\r\n");
        capsule.console().wait_for_text("one screen").await?;
        capsule.console().keys("ctrl+b d")?;
        capsule
            .pump_until(|handle| handle.attachments() == 0)
            .await?;
        capsule.enter();
        let words = PaneWords::openshell();
        let note = fill_in(&words.detached, &[("agent", "Codex")]);
        capsule.console().wait_for_text(&note).await?;
        capsule.console().keys("enter")?;
        capsule.attached().await?;
        let screen = capsule.console().screen();
        assert_eq!(screen.matches("one screen").count(), 1, "{screen}");
        assert_eq!(capsule.handle.attachments(), 1);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_closed_stream_after_the_attach_detaches_and_enter_reattaches() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.handle.close_main_streams();
        let words = PaneWords::openshell();
        capsule
            .console()
            .wait_for_text(&words.connection_lost)
            .await?;
        capsule.console().keys("enter")?;
        capsule.attached().await?;
        assert_eq!(capsule.handle.attachments(), 1);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_refused_attach_is_failed_with_its_reason_and_enter_retries() -> TestResult {
    guard(Box::pin(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.world.gateway.set_attachment_cap(0);
        let mut lab = Lab::with(rig).await?;
        let id = lab.world().instances[0].id.clone();
        lab.console.console().app().go(Go::Attach {
            instance: id,
            pane: None,
        });
        let words = PaneWords::openshell();
        let lead = fill_in(&words.failed, &[("agent", "Codex"), ("reason", "")]);
        lab.console.wait_for_text(&lead).await?;
        assert!(
            lab.console.screen().contains("connection limit"),
            "{}",
            lab.console.screen()
        );
        let step: String = words.next_doctor.chars().take(8).collect();
        assert!(
            !lab.console.screen().contains(&step),
            "{}",
            lab.console.screen()
        );
        lab.rig.world.gateway.set_attachment_cap(20);
        lab.console.keys("enter")?;
        let handle = lab.rig.handle(1).await?;
        until("the main process has one attachment", || {
            handle.attachments() == 1
        })
        .await?;
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn an_exited_codex_shows_its_code_and_enter_restarts_it() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.handle.exit_codex(3);
        let words = PaneWords::openshell();
        let note = fill_in(&words.exited, &[("agent", "Codex"), ("code", "3")]);
        capsule.console().wait_for_text(&note).await?;
        assert_eq!(capsule.handle.attachments(), 0);
        capsule.console().keys("enter")?;
        capsule.attached().await?;
        assert_eq!(
            capsule.handle.supervisor(),
            Some(SupervisorState::Running {
                attempt: SupervisorState::Exited {
                    attempt: AttemptId::FIRST,
                    code: 3
                }
                .next_attempt()
                .ok_or("no next attempt")?
            })
        );
        assert_eq!(capsule.handle.attachments(), 1);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_bell_in_a_background_tab_marks_the_pane_and_focus_events_follow_the_tab() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.handle.print_main(b"\x1b[?1004h");
        capsule.sync().await?;
        let instance = capsule.id.clone();
        capsule.console().console().app().go(Go::NewSession {
            instance,
            agent: None,
            account: None,
        });
        capsule
            .pump_until(|handle| handle.shells().len() == 1)
            .await?;
        capsule
            .pump_until(|handle| handle.raw_input().ends_with(b"\x1b[O"))
            .await?;
        assert!(!capsule.console().screen().contains("Codex ●"));
        capsule.handle.print_main(b"\x07");
        for _ in 0..400 {
            if capsule.console().screen().contains("Codex ●") {
                break;
            }
            capsule.console().pump(1).await;
        }
        let marked = capsule.console().screen();
        assert!(marked.contains("Codex ●"), "{marked}");
        capsule.console().keys("ctrl+b p")?;
        capsule
            .pump_until(|handle| handle.raw_input().ends_with(b"\x1b[I"))
            .await?;
        assert!(!capsule.console().screen().contains("Codex ●"));
        Ok(())
    }))
    .await
}

fn osc52(text: &[u8]) -> Vec<u8> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = b"\x1b]52;c;".to_vec();
    for chunk in text.chunks(3) {
        let n =
            chunk.iter().fold(0_u32, |acc, b| acc << 8 | u32::from(*b)) << (8 * (3 - chunk.len()));
        for i in 0..=chunk.len() {
            out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize]);
        }
        out.extend(std::iter::repeat_n(b'=', 3 - chunk.len()));
    }
    out.push(0x07);
    out
}

#[tokio::test]
async fn a_codex_clipboard_write_after_a_key_is_copied_and_says_so() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.console().keys("a")?;
        capsule.feed(&osc52(b"Hello")).await?;
        assert_eq!(capsule.console().clipboard().writes(), vec!["Hello"]);
        let words = PaneWords::openshell();
        let what = fill_in(
            &words.copied_from,
            &[("source", "Codex"), ("chars", "5 characters")],
        );
        assert!(
            capsule.console().status_line().contains(&what),
            "{}",
            capsule.console().status_line()
        );
        capsule.feed(&osc52(b"Again")).await?;
        assert_eq!(
            capsule.console().clipboard().writes(),
            vec!["Hello", "Again"]
        );
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_clipboard_write_is_refused_without_a_gesture_or_after_the_window() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.feed(&osc52(b"Early")).await?;
        assert_eq!(capsule.console().clipboard().writes(), Vec::<String>::new());
        let words = PaneWords::openshell();
        let blocked = fill_in(&words.clip_write_blocked, &[("source", "Codex")]);
        assert!(capsule.console().status_line().contains(&blocked));
        capsule.console().keys("a")?;
        capsule.console().elapse(6);
        capsule.console().pump(1).await;
        capsule.feed(&osc52(b"Late")).await?;
        assert_eq!(capsule.console().clipboard().writes(), Vec::<String>::new());
        capsule.console().keys("a")?;
        capsule.console().elapse(2);
        capsule.console().pump(1).await;
        capsule.feed(&osc52(b"Inside")).await?;
        assert_eq!(capsule.console().clipboard().writes(), vec!["Inside"]);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_refresh_keeps_the_open_panes() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        let instance = capsule.id.clone();
        capsule.console().console().app().go(Go::NewSession {
            instance,
            agent: None,
            account: None,
        });
        capsule
            .pump_until(|handle| handle.shells().len() == 1)
            .await?;
        capsule.console().pump(12).await;
        let before = capsule.console().screen();
        assert!(
            before.contains("1 Codex") && before.contains("2 Shell"),
            "{before}"
        );
        capsule.lab.world().refresh_now();
        capsule.console().settle().await?;
        let after = capsule.console().screen();
        assert!(
            after.contains("1 Codex") && after.contains("2 Shell"),
            "{after}"
        );
        assert_eq!(capsule.handle.attachments(), 1);
        assert_eq!(capsule.handle.shells().len(), 1);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_clipboard_write_in_the_replay_is_not_copied_after_a_key() -> TestResult {
    guard(Box::pin(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        handle.print_main(&osc52(b"Replayed"));
        let mut lab = Lab::with(rig).await?;
        let id = lab.world().instances[0].id.clone();
        let mut capsule = Capsule {
            lab,
            id,
            handle,
            marks: 0,
            seen: 0,
        };
        capsule.enter();
        capsule.console().keys("a")?;
        capsule.attached().await?;
        capsule.console().pump(20).await;
        assert_eq!(capsule.console().clipboard().writes(), Vec::<String>::new());
        capsule.console().keys("a")?;
        capsule.feed(&osc52(b"Live")).await?;
        assert_eq!(capsule.console().clipboard().writes(), vec!["Live"]);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_clipboard_write_from_a_pane_that_lost_the_focus_is_refused() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.console().keys("a")?;
        let instance = capsule.id.clone();
        capsule.console().console().app().go(Go::NewSession {
            instance,
            agent: None,
            account: None,
        });
        capsule
            .pump_until(|handle| handle.shells().len() == 1)
            .await?;
        capsule.console().pump(12).await;
        capsule.console().keys("a")?;
        capsule.handle.print_main(&osc52(b"Hidden"));
        capsule.console().pump(20).await;
        assert_eq!(capsule.console().clipboard().writes(), Vec::<String>::new());
        capsule.console().keys("ctrl+b p")?;
        capsule.console().keys("a")?;
        capsule.feed(&osc52(b"Shown")).await?;
        assert_eq!(capsule.console().clipboard().writes(), vec!["Shown"]);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_clipboard_read_is_never_answered_and_the_cap_is_on_decoded_bytes() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.console().keys("a")?;
        capsule.feed(b"\x1b]52;c;?\x07").await?;
        assert_eq!(capsule.sync().await?, b"a");
        assert_eq!(capsule.console().clipboard().writes(), Vec::<String>::new());
        capsule.console().keys("a")?;
        capsule.feed(&osc52(&vec![b'x'; 100_000])).await?;
        assert_eq!(capsule.console().clipboard().writes().len(), 1);
        capsule.console().keys("a")?;
        capsule.feed(&osc52(&vec![b'y'; 100_001])).await?;
        assert_eq!(capsule.console().clipboard().writes().len(), 1);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_shell_pane_never_writes_the_clipboard() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        let instance = capsule.id.clone();
        capsule.console().console().app().go(Go::NewSession {
            instance,
            agent: None,
            account: None,
        });
        capsule
            .pump_until(|handle| handle.shells().len() == 1)
            .await?;
        capsule.console().pump(12).await;
        capsule
            .console()
            .type_text("printf '\\033]52;c;SGVsbG8=\\007'");
        capsule.console().keys("ctrl+j")?;
        let words = PaneWords::openshell();
        let blocked = fill_in(&words.clip_write_blocked, &[("source", &words.shell_label)]);
        for _ in 0..400 {
            if capsule.console().status_line().contains(&blocked) {
                break;
            }
            capsule.console().pump(1).await;
        }
        assert!(
            capsule.console().status_line().contains(&blocked),
            "{}",
            capsule.console().status_line()
        );
        assert_eq!(capsule.console().clipboard().writes(), Vec::<String>::new());
        Ok(())
    }))
    .await
}

const FLOOD_LINES: u32 = 22_000;

fn numbered(first: u32, last: u32) -> Vec<u8> {
    let mut chunk = String::new();
    for number in first..=last {
        chunk.push_str(&"x".repeat(90));
        chunk.push_str(&format!("{number:05}\r\n"));
    }
    chunk.into_bytes()
}

fn visible_numbers(screen: &str) -> Vec<u32> {
    screen
        .split_whitespace()
        .filter_map(|word| {
            let (rest, digits) = word.split_at_checked(word.len().checked_sub(5)?)?;
            (rest.ends_with('x') && rest.bytes().all(|b| b == b'x'))
                .then(|| digits.parse().ok())
                .flatten()
        })
        .collect()
}

async fn flooded() -> Result<Capsule, Box<dyn Error>> {
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    let handle = rig.handle(1).await?;
    rig.world.seed_all(&rig.config).await?;
    let console = ConsoleRig::builder(&rig.world)
        .pane_inbox_bytes(128 * 1024)
        .build()
        .await?;
    let mut lab = Lab {
        rig,
        console,
        code: std::path::PathBuf::new(),
    };
    lab.console.skip_intro();
    let id = lab.world().instances[0].id.clone();
    let mut capsule = Capsule {
        lab,
        id,
        handle,
        marks: 0,
        seen: 0,
    };
    capsule.enter();
    capsule.attached().await?;
    Ok(capsule)
}

#[tokio::test]
async fn a_flood_backs_up_then_drains_and_the_console_stays_live() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = flooded().await?;
        for first in (1..=FLOOD_LINES).step_by(40) {
            let last = (first + 39).min(FLOOD_LINES);
            capsule.handle.print_main(&numbered(first, last));
        }
        let mut last = usize::MAX;
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let now = capsule.handle.undelivered_main_output();
            if now == last {
                break;
            }
            last = now;
        }
        assert!(last > 512 * 1024, "{last}");
        capsule.handle.print_main(b"FLOODEND\r\n");
        let screen = capsule.console().wait_for_text("FLOODEND").await?;
        assert_eq!(capsule.handle.undelivered_main_output(), 0);
        let numbers = visible_numbers(&screen);
        assert!(numbers.len() >= 2, "{screen}");
        assert_eq!(numbers.last(), Some(&FLOOD_LINES), "{screen}");
        assert!(
            numbers.windows(2).all(|pair| pair[1] == pair[0] + 1),
            "{numbers:?}"
        );
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn each_burst_of_a_flood_is_complete_when_it_has_arrived() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = flooded().await?;
        let mut next = 1_u32;
        for size in [700_u32, 1_500, 2_300, 900, 1_900, 1_100, 2_700, 500] {
            let last = next + size - 1;
            capsule.handle.print_main(&numbered(next, last));
            let screen = capsule
                .console()
                .wait_for_text(&format!("{last:05}"))
                .await?;
            let numbers = visible_numbers(&screen);
            assert!(numbers.len() >= 2, "{screen}");
            assert_eq!(numbers.last(), Some(&last), "{screen}");
            assert!(
                numbers.windows(2).all(|pair| pair[1] == pair[0] + 1),
                "{numbers:?}"
            );
            next = last + 1;
        }
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn closing_the_console_ends_every_pane_task() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        assert_eq!(capsule.console().console().pane_tasks(), 1);
        assert!(
            capsule
                .console()
                .console()
                .close_panes(Duration::from_secs(5))
                .await
        );
        assert_eq!(capsule.console().console().pane_tasks(), 0);
        capsule
            .pump_until(|handle| handle.attachments() == 0)
            .await?;
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn an_idle_console_has_no_pane_task() -> TestResult {
    guard(Box::pin(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let mut lab = Lab::with(rig).await?;
        lab.console.pump(10).await;
        assert_eq!(lab.console.console().pane_tasks(), 0);
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_running_sandbox_opens_one_codex_pane_and_a_second_codex_is_refused() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        let world = &mut capsule.lab.console.console().app().world;
        let instance = world.instances[0].clone();
        assert_eq!(instance.status, InstanceStatus::Running);
        let backend = Arc::clone(&world.backend);
        let requests = backend.initial_panes(&instance);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].instance, instance.id);
        assert_eq!(requests[0].agent(), Some(instance.agent));
        let codex = PaneKind::Agent {
            agent: instance.agent,
            account: None,
        };
        assert_eq!(
            backend.spawnable(world, &instance, &codex),
            Err(PaneRefusal::MainProcessOpen {
                agent: instance.agent
            })
        );
        assert_eq!(
            backend.spawnable(world, &instance, &PaneKind::Shell),
            Ok(())
        );
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_stopped_sandbox_has_no_initial_pane() -> TestResult {
    guard(Box::pin(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let mut lab = Lab::with(rig).await?;
        let mut instance = lab.world().instances[0].clone();
        instance.status = InstanceStatus::RestoreAvailable;
        let backend = Arc::clone(&lab.world().backend);
        assert_eq!(backend.initial_panes(&instance), Vec::<PaneRequest>::new());
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn a_second_client_leaves_the_pane_read_only_until_the_owner_goes() -> TestResult {
    guard(Box::pin(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let name = rig.handle(1).await?.name();
        let held = rig
            .world
            .session
            .attach(AttachRequest {
                sandbox: name,
                size: size(100, 30)?,
                input: InputMode::Owner,
                agent: None,
            })
            .await?;
        let mut capsule = Capsule::attach(rig, |_| {}).await?;
        let words = PaneWords::openshell();
        capsule.console().wait_for_text(&words.viewing).await?;
        drop(held);
        capsule
            .pump_until(|handle| handle.attachments() == 1)
            .await?;
        capsule.console().keys("y")?;
        capsule
            .pump_until(|handle| handle.main_input() == b"y")
            .await?;
        for _ in 0..400 {
            if !capsule.console().screen().contains(&words.viewing) {
                return Ok(());
            }
            capsule.console().pump(1).await;
        }
        Err(format!("still read-only:\n{}", capsule.console().screen()).into())
    }))
    .await
}

#[tokio::test]
async fn a_paste_reaches_codex_wrapped_when_codex_asked_for_brackets() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.handle.print_main(b"\x1b[?2004h");
        capsule.sync().await?;
        capsule.console().paste("one\ntwo");
        assert_eq!(capsule.sync().await?, b"\x1b[200~one\ntwo\x1b[201~");
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn pane_output_reaches_the_screen_with_no_tick() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.handle.print_main(b"WAKEMARK");
        for _ in 0..200 {
            capsule.lab.console.settle().await?;
            if capsule.lab.console.screen().contains("WAKEMARK") {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        Err(format!("no wake:\n{}", capsule.lab.console.screen()).into())
    }))
    .await
}

#[tokio::test]
async fn going_to_the_manager_detaches_codex_once() -> TestResult {
    guard(Box::pin(async {
        let mut capsule = Capsule::open().await?;
        capsule.lab.console.console().app().go(Go::Manager);
        capsule
            .pump_until(|handle| handle.attachments() == 0)
            .await?;
        Ok(())
    }))
    .await
}

#[tokio::test]
async fn an_unreachable_gateway_fails_the_pane_with_the_doctor_step() -> TestResult {
    guard(Box::pin(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let mut lab = Lab::with(rig).await?;
        let id = lab.world().instances[0].id.clone();
        lab.rig.world.gateway.inject(Fault::FailNext {
            op: GatewayOp::Find,
            error: GatewayError::Unreachable {
                endpoint: "10.0.0.9:8080".to_owned(),
                message: "connection refused".to_owned(),
            },
        });
        lab.console.console().app().go(Go::Attach {
            instance: id,
            pane: None,
        });
        let words = PaneWords::openshell();
        let step: String = words.next_doctor.chars().take(8).collect();
        lab.console.wait_for_text(&format!("· {step}")).await?;
        let screen = lab.console.screen();
        assert!(screen.contains("connection refused"), "{screen}");
        Ok(())
    }))
    .await
}
