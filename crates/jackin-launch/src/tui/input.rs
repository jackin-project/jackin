// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Dedicated terminal-input owner for the launch rich surface.

use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use crossterm::ExecutableCommand as _;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::LeaveAlternateScreen;

const DOUBLE_CTRL_C_WINDOW: Duration = Duration::from_millis(750);
const HARD_EXIT_DRAIN_LIMIT: usize = 16_384;
pub(super) const ANSI_RESET: &str = "\x1b[0m";

pub(super) fn disable_mouse_capture<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    out.write_all(b"\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l")?;
    out.flush()
}

#[derive(Debug)]
pub struct LaunchInput {
    rx: Arc<Mutex<mpsc::Receiver<Event>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    activity: jackin_core::TerminalActivity,
}

impl LaunchInput {
    #[expect(
        clippy::excessive_nesting,
        reason = "LaunchInput spawn wires the event-polling thread, double- \
                  Ctrl-C tracker, and IPC channels together. The nested `while` \
                  + `match` + `if let` is the per-event ARM/dispatch protocol."
    )]
    pub fn spawn(activity: jackin_core::TerminalActivity) -> Self {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread_activity = activity.clone();
        let thread = jackin_telemetry::spawn::thread_stream("launch.input", move || {
            let mut ctrl_c = DoubleCtrlC::new(DOUBLE_CTRL_C_WINDOW);
            while !thread_stop.load(Ordering::Relaxed) {
                let ready = thread_activity.run_if_active(|| {
                    match event::poll(Duration::from_millis(25)) {
                        Ok(true) if !thread_stop.load(Ordering::Relaxed) => event::read().map(Some),
                        Ok(_) => Ok(None),
                        Err(error) => Err(error),
                    }
                });
                match ready {
                    Some(Ok(Some(ev))) => {
                        if ctrl_c.observe(&ev, Instant::now()) == CtrlCAction::HardExit {
                            restore_terminal_for_process_exit();
                            std::process::exit(0);
                        }
                        if tx.send(ev).is_err() {
                            break;
                        }
                    }
                    Some(Ok(None)) => {}
                    Some(Err(_)) => break,
                    None => std::thread::sleep(Duration::from_millis(25)),
                }
            }
        });
        Self {
            rx: Arc::new(Mutex::new(rx)),
            stop,
            thread: Some(thread),
            activity,
        }
    }

    #[cfg(test)]
    pub(crate) fn queued_for_test(events: impl IntoIterator<Item = Event>) -> (Self, mpsc::Sender<Event>) {
        let (tx, rx) = mpsc::channel();
        for event in events {
            tx.send(event).unwrap();
        }
        (Self {
            rx: Arc::new(Mutex::new(rx)),
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
            activity: jackin_core::TerminalActivity::new(|| true, Arc::new(Mutex::new(()))),
        }, tx)
    }

    /// Wait until the bounded polling thread relinquishes terminal input.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            // The input owner lives on the renderer thread; guard against a
            // future caller moving it into its own input worker.
            if thread.thread().id() != std::thread::current().id() {
                drop(thread.join());
            }
        }
    }

    pub fn try_recv(&self) -> anyhow::Result<Option<Event>> {
        if !self.activity.is_active() {
            return Ok(None);
        }
        let receiver = self.rx.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => anyhow::bail!("launch input disconnected"),
        }
    }

    pub fn recv_key(&self, context: &'static str) -> anyhow::Result<event::KeyEvent> {
        loop {
            let received = self.activity.run_if_active(|| {
                self.try_recv().map_err(|_| anyhow::anyhow!(context))
            });
            let Some(event) = received.transpose()?.flatten() else {
                std::thread::sleep(Duration::from_millis(25));
                continue;
            };
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
        self.stop();
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
    out.write_all(ANSI_RESET.as_bytes())?;
    out.write_all(&termrock::osc::encode_pointer(
        termrock::osc::PointerShape::Default,
    ))?;
    disable_mouse_capture(out)?;
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

#[cfg(test)]
mod tests;
