// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Dedicated terminal-input owner for the launch rich surface.

use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use crossterm::ExecutableCommand as _;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::LeaveAlternateScreen;

const DOUBLE_CTRL_C_WINDOW: Duration = Duration::from_millis(750);
const HARD_EXIT_DRAIN_LIMIT: usize = 16_384;

#[derive(Debug)]
pub struct LaunchInput {
    rx: Arc<Mutex<mpsc::Receiver<Event>>>,
    stop: Arc<AtomicBool>,
}

impl LaunchInput {
    #[expect(
        clippy::excessive_nesting,
        reason = "LaunchInput spawn wires the event-polling thread, double- \
                  Ctrl-C tracker, and IPC channels together. The nested `while` \
                  + `match` + `if let` is the per-event ARM/dispatch protocol."
    )]
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut ctrl_c = DoubleCtrlC::new(DOUBLE_CTRL_C_WINDOW);
            while !thread_stop.load(Ordering::Relaxed) {
                match event::poll(Duration::from_millis(25)) {
                    Ok(true) => {
                        let Ok(ev) = event::read() else {
                            continue;
                        };
                        if ctrl_c.observe(&ev, Instant::now()) == CtrlCAction::HardExit {
                            restore_terminal_for_process_exit();
                            std::process::exit(0);
                        }
                        if tx.send(ev).is_err() {
                            break;
                        }
                    }
                    Ok(false) => {}
                    Err(_) => break,
                }
            }
        });
        Self {
            rx: Arc::new(Mutex::new(rx)),
            stop,
        }
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.rx.lock().ok()?.try_recv().ok()
    }

    pub fn recv_key(&self, context: &'static str) -> anyhow::Result<event::KeyEvent> {
        loop {
            let event = self
                .rx
                .lock()
                .map_err(|_| anyhow::anyhow!("launch input mutex poisoned"))?
                .recv()
                .context(context)?;
            let Event::Key(key) = event else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            return Ok(key);
        }
    }
}

impl Drop for LaunchInput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CtrlCAction {
    Continue,
    HardExit,
}

#[derive(Debug)]
pub(super) struct DoubleCtrlC {
    window: Duration,
    last: Option<Instant>,
}

impl DoubleCtrlC {
    pub(super) const fn new(window: Duration) -> Self {
        Self { window, last: None }
    }

    pub(super) fn observe(&mut self, event: &Event, now: Instant) -> CtrlCAction {
        if !is_ctrl_c_event(event) {
            self.last = None;
            return CtrlCAction::Continue;
        }
        let action = if self
            .last
            .is_some_and(|last| now.duration_since(last) <= self.window)
        {
            CtrlCAction::HardExit
        } else {
            CtrlCAction::Continue
        };
        self.last = Some(now);
        action
    }
}

pub(super) fn is_ctrl_c_event(ev: &Event) -> bool {
    matches!(
        ev,
        Event::Key(k)
            if k.kind == KeyEventKind::Press
                && k.code == KeyCode::Char('c')
                && k.modifiers.contains(KeyModifiers::CONTROL)
    )
}

pub(super) fn restore_terminal_for_process_exit() {
    let mut stdout = std::io::stdout();
    drop(write_forced_terminal_restore(&mut stdout));
    drain_pending_terminal_events(HARD_EXIT_DRAIN_LIMIT);
    drop(crossterm::terminal::disable_raw_mode());
    drain_pending_terminal_events(HARD_EXIT_DRAIN_LIMIT);
    drop(stdout.execute(LeaveAlternateScreen));
    drop(stdout.flush());
}

pub(super) fn write_forced_terminal_restore<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    out.write_all(jackin_tui::ansi::RESET.as_bytes())?;
    out.write_all(&termrock::osc::encode_pointer(
        termrock::osc::PointerShape::Default,
    ))?;
    jackin_tui::terminal_modes::disable_mouse_capture(out)?;
    // Defensive teardown for modes used by hosted agent UIs. The launch
    // surface does not enable all of them, but a hard process exit must leave
    // the operator's terminal cooked even when it interrupts a transition.
    out.write_all(b"\x1b[?1004l\x1b[?2004l\x1b[?25h")?;
    out.flush()
}

pub(super) fn drain_pending_terminal_events(limit: usize) {
    for _ in 0..limit {
        match event::poll(Duration::ZERO) {
            Ok(true) => {
                drop(event::read());
            }
            Ok(false) | Err(_) => break,
        }
    }
}

pub(super) fn restore_renderer_terminal_for_process_exit(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
) {
    drop(write_forced_terminal_restore(terminal.backend_mut()));
    drain_pending_terminal_events(HARD_EXIT_DRAIN_LIMIT);
    drop(crossterm::terminal::disable_raw_mode());
    drain_pending_terminal_events(HARD_EXIT_DRAIN_LIMIT);
    drop(terminal.backend_mut().execute(LeaveAlternateScreen));
    drop(terminal.backend_mut().flush());
}

#[cfg(test)]
mod tests;
