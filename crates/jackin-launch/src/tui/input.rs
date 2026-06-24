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

#[derive(Debug)]
pub struct LaunchInput {
    rx: Arc<Mutex<mpsc::Receiver<Event>>>,
    stop: Arc<AtomicBool>,
}

impl LaunchInput {
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
    drop(stdout.execute(crossterm::cursor::Show));
    drop(jackin_tui::terminal_modes::disable_mouse_capture(
        &mut stdout,
    ));
    drop(crossterm::terminal::disable_raw_mode());
    drop(stdout.execute(LeaveAlternateScreen));
    drop(stdout.flush());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    #[test]
    fn second_ctrl_c_inside_window_hard_exits() {
        let mut detector = DoubleCtrlC::new(Duration::from_millis(750));
        let start = Instant::now();
        let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

        assert_eq!(detector.observe(&ctrl_c, start), CtrlCAction::Continue);
        assert_eq!(
            detector.observe(&ctrl_c, start + Duration::from_millis(100)),
            CtrlCAction::HardExit
        );
    }

    #[test]
    fn ctrl_c_outside_window_starts_new_sequence() {
        let mut detector = DoubleCtrlC::new(Duration::from_millis(750));
        let start = Instant::now();
        let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

        assert_eq!(detector.observe(&ctrl_c, start), CtrlCAction::Continue);
        assert_eq!(
            detector.observe(&ctrl_c, start + Duration::from_millis(900)),
            CtrlCAction::Continue
        );
    }

    #[test]
    fn non_ctrl_c_resets_sequence() {
        let mut detector = DoubleCtrlC::new(Duration::from_millis(750));
        let start = Instant::now();
        let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        let other = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

        assert_eq!(detector.observe(&ctrl_c, start), CtrlCAction::Continue);
        assert_eq!(
            detector.observe(&other, start + Duration::from_millis(100)),
            CtrlCAction::Continue
        );
        assert_eq!(
            detector.observe(&ctrl_c, start + Duration::from_millis(200)),
            CtrlCAction::Continue
        );
    }
}
