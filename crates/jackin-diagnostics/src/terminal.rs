// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Product-local terminal ownership and title policy.

use std::io::{self, Write};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use crossterm::ExecutableCommand as _;
use crossterm::cursor::{Hide, Show};
use crossterm::event::EnableMouseCapture;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

pub use jackin_core::shorten_home;
pub use jackin_core::{TerminalActivity, TerminalOwnershipGuard};

#[derive(Default)]
struct OwnershipState {
    rich_depth: usize,
    host_depth: usize,
    mode_depth: usize,
    external_depth: usize,
    cleanup_input: Option<fn()>,
    foreground: Vec<Arc<()>>,
}

static OWNERSHIP: Mutex<OwnershipState> = Mutex::new(OwnershipState {
    rich_depth: 0,
    host_depth: 0,
    mode_depth: 0,
    external_depth: 0,
    cleanup_input: None,
    foreground: Vec::new(),
});

static FOREGROUND_GATE: LazyLock<Arc<Mutex<()>>> = LazyLock::new(|| Arc::new(Mutex::new(())));

fn ownership() -> MutexGuard<'static, OwnershipState> {
    OWNERSHIP
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Surface {
    Rich,
    Host,
    External,
}

#[derive(Clone, Copy)]
enum LogicalSurface {
    Rich,
    Host,
}

impl From<LogicalSurface> for Surface {
    fn from(surface: LogicalSurface) -> Self {
        match surface {
            LogicalSurface::Rich => Self::Rich,
            LogicalSurface::Host => Self::Host,
        }
    }
}

/// Claim logical rich-surface ownership without changing terminal modes.
pub fn claim_rich_surface() -> TerminalOwnershipGuard {
    claim_logical(LogicalSurface::Rich)
}

/// Claim logical host-screen ownership without changing terminal modes.
pub fn claim_host_screen() -> TerminalOwnershipGuard {
    claim_logical(LogicalSurface::Host)
}

/// Acquire rich-surface ownership and a shared terminal mode lease.
pub fn enter_rich_surface() -> io::Result<TerminalOwnershipGuard> {
    acquire(Surface::Rich, true, None)
}

/// Acquire host-screen ownership and a shared terminal mode lease.
pub fn enter_host_screen(cleanup_input: fn()) -> io::Result<TerminalOwnershipGuard> {
    acquire(Surface::Host, true, Some(cleanup_input))
}

/// Reserve foreground input/output for an interactive external process.
/// Host screen modes may remain inherited; active rich renderers are rejected.
pub fn claim_external_terminal() -> io::Result<TerminalOwnershipGuard> {
    acquire(Surface::External, false, None)
}

fn claim_logical(logical_surface: LogicalSurface) -> TerminalOwnershipGuard {
    let surface = Surface::from(logical_surface);
    let gate = Arc::clone(&FOREGROUND_GATE);
    let _gate = gate
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let token = Arc::new(());
    {
        let mut state = ownership();
        match logical_surface {
            LogicalSurface::Rich => state.rich_depth += 1,
            LogicalSurface::Host => state.host_depth += 1,
        }
        state.foreground.push(Arc::clone(&token));
    }
    make_guard(surface, false, Arc::clone(&gate), token)
}

fn acquire(
    surface: Surface,
    physical: bool,
    cleanup_input: Option<fn()>,
) -> io::Result<TerminalOwnershipGuard> {
    let gate = Arc::clone(&FOREGROUND_GATE);
    let _gate = gate
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let token = Arc::new(());
    let physical = {
        let mut state = ownership();
        let physical = physical || (surface == Surface::External && state.mode_depth > 0);
        state.acquire(surface, physical, cleanup_input, &mut HostModes)?;
        state.foreground.push(Arc::clone(&token));
        physical
    };
    Ok(make_guard(surface, physical, Arc::clone(&gate), token))
}

fn make_guard(
    surface: Surface,
    physical: bool,
    gate: Arc<Mutex<()>>,
    token: Arc<()>,
) -> TerminalOwnershipGuard {
    let check_token = Arc::clone(&token);
    let activity = TerminalActivity::new(
        move || {
            let state = ownership();
            if surface == Surface::External {
                state.external_depth > 0
                    && state
                        .foreground
                        .iter()
                        .any(|owner| Arc::ptr_eq(owner, &check_token))
            } else {
                state.external_depth == 0
                    && state
                        .foreground
                        .last()
                        .is_some_and(|top| Arc::ptr_eq(top, &check_token))
            }
        },
        Arc::clone(&gate),
    );
    TerminalOwnershipGuard::new_with_activity(
        move || {
            let _gate = FOREGROUND_GATE
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut state = ownership();
            state.foreground.retain(|owner| !Arc::ptr_eq(owner, &token));
            state.release(surface, physical, &mut HostModes);
        },
        activity,
    )
}

impl OwnershipState {
    fn acquire(
        &mut self,
        surface: Surface,
        physical: bool,
        cleanup_input: Option<fn()>,
        modes: &mut impl ModeBackend,
    ) -> io::Result<()> {
        if (surface == Surface::External && (self.rich_depth > 0 || self.external_depth > 0))
            || (physical && surface != Surface::External && self.external_depth > 0)
        {
            return Err(io::Error::other(
                "terminal already reserved by an incompatible owner",
            ));
        }
        if surface == Surface::External && !physical {
            modes.begin_buffering();
        }
        if physical {
            if self.mode_depth == 0 {
                enter_modes(modes)?;
                modes.begin_buffering();
            }
            self.mode_depth += 1;
            if self.cleanup_input.is_none() {
                self.cleanup_input = cleanup_input;
            }
        }
        match surface {
            Surface::Rich => self.rich_depth += 1,
            Surface::Host => self.host_depth += 1,
            Surface::External => self.external_depth += 1,
        }
        Ok(())
    }

    fn release(&mut self, surface: Surface, physical: bool, modes: &mut impl ModeBackend) {
        match surface {
            Surface::Rich => self.rich_depth -= 1,
            Surface::Host => self.host_depth -= 1,
            Surface::External => self.external_depth -= 1,
        }
        if physical {
            self.mode_depth -= 1;
            if self.mode_depth == 0 {
                if let Some(cleanup) = self.cleanup_input.take() {
                    modes.cleanup_input(cleanup);
                }
                restore_modes(modes);
                modes.end_buffering();
            } else if self.external_depth == 0 {
                if surface == Surface::External {
                    // Capsule may reset terminal modes while it owns the foreground.
                    // Reassert every mode for the surviving host TUI lease.
                    reassert_host_modes(modes);
                } else {
                    // Retiring ratatui backends can show the surviving owner's cursor.
                    drop(modes.apply(ModeOperation::Hide));
                }
            }
        } else if surface == Surface::External && self.external_depth == 0 {
            modes.end_buffering();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModeOperation {
    RawOn,
    AltOn,
    MouseOn,
    Hide,
    MouseOff,
    RawOff,
    AltOff,
    Show,
    Flush,
}

trait ModeBackend {
    fn apply(&mut self, operation: ModeOperation) -> io::Result<()>;
    fn begin_buffering(&mut self);
    fn end_buffering(&mut self);
    fn cleanup_input(&mut self, cleanup: fn());
}

struct HostModes;

impl ModeBackend for HostModes {
    fn apply(&mut self, operation: ModeOperation) -> io::Result<()> {
        let mut out = io::stdout();
        match operation {
            ModeOperation::RawOn => enable_raw_mode(),
            ModeOperation::RawOff => disable_raw_mode(),
            ModeOperation::AltOn => out.execute(EnterAlternateScreen).map(|_| ()),
            ModeOperation::AltOff => out.execute(LeaveAlternateScreen).map(|_| ()),
            ModeOperation::MouseOn => out.execute(EnableMouseCapture).map(|_| ()),
            ModeOperation::MouseOff => disable_mouse_capture(&mut out),
            ModeOperation::Hide => out.execute(Hide).map(|_| ()),
            ModeOperation::Show => out.execute(Show).map(|_| ()),
            ModeOperation::Flush => out.flush(),
        }
    }

    fn begin_buffering(&mut self) {
        crate::logging::begin_debug_buffering();
    }

    fn end_buffering(&mut self) {
        crate::logging::end_debug_buffering();
    }

    fn cleanup_input(&mut self, cleanup: fn()) {
        cleanup();
    }
}

// Roll back attempted operations even when a terminal write partially succeeds.
fn enter_modes(modes: &mut impl ModeBackend) -> io::Result<()> {
    use ModeOperation::{AltOff, AltOn, Flush, Hide, MouseOff, MouseOn, RawOff, RawOn};
    modes.apply(RawOn)?;
    if let Err(error) = modes.apply(AltOn) {
        drop(modes.apply(AltOff));
        drop(modes.apply(RawOff));
        drop(modes.apply(Flush));
        return Err(error);
    }
    if let Err(error) = modes.apply(MouseOn) {
        drop(modes.apply(MouseOff));
        drop(modes.apply(RawOff));
        drop(modes.apply(AltOff));
        drop(modes.apply(Flush));
        return Err(error);
    }
    if let Err(error) = modes.apply(Hide).and_then(|()| modes.apply(Flush)) {
        restore_modes(modes);
        return Err(error);
    }
    Ok(())
}

fn disable_mouse_capture(out: &mut impl Write) -> io::Result<()> {
    // Reset every supported mouse protocol, including URXVT's 1015 mode.
    out.write_all(b"\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l")?;
    out.flush()
}

fn restore_modes(modes: &mut impl ModeBackend) {
    use ModeOperation::{AltOff, Flush, MouseOff, RawOff, Show};
    for operation in [MouseOff, RawOff, AltOff, Show, Flush] {
        drop(modes.apply(operation));
    }
}

fn reassert_host_modes(modes: &mut impl ModeBackend) {
    use ModeOperation::{AltOn, Flush, Hide, MouseOn, RawOn};
    for operation in [RawOn, AltOn, MouseOn, Hide, Flush] {
        drop(modes.apply(operation));
    }
}

#[must_use]
pub fn rich_surface_active() -> bool {
    ownership().rich_depth > 0
}

#[must_use]
pub fn host_screen_owned() -> bool {
    ownership().host_depth > 0
}

#[must_use]
pub fn rich_terminal_owned() -> bool {
    let state = ownership();
    state.rich_depth > 0 || state.host_depth > 0 || state.external_depth > 0
}

pub fn reassert_alt_screen() {
    let state = ownership();
    if state.host_depth == 0 {
        return;
    }
    let mut out = io::stdout();
    drop(out.execute(EnterAlternateScreen));
    drop(out.execute(Hide));
}

pub fn set_terminal_title(title: &str) {
    let mut stderr = io::stderr().lock();
    drop(write!(stderr, "\x1b]0;jackin❯ · {title}\x07"));
    drop(stderr.flush());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DIAGNOSTICS_TEST_LOCK;

    #[test]
    fn external_scope_reserves_foreground_without_claiming_host_modes() {
        let _lock = DIAGNOSTICS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let external = claim_external_terminal().unwrap();
        assert!(rich_terminal_owned());
        assert!(!host_screen_owned());
        assert!(!rich_surface_active());
        assert_eq!(
            external.activity().run_if_active(rich_terminal_owned),
            Some(true)
        );
        assert!(claim_external_terminal().err().is_some());
        assert!(enter_rich_surface().err().is_some());
        assert!(enter_host_screen(cleanup_first).err().is_some());
        let logical = claim_rich_surface();
        assert!(!logical.activity().is_active());
        assert!(external.activity().is_active());
        drop(logical);
        drop(external);
        assert!(!rich_terminal_owned());
    }

    #[test]
    fn external_scope_rejects_existing_rich_reader_without_mutation() {
        let _lock = DIAGNOSTICS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let rich = claim_rich_surface();
        assert!(claim_external_terminal().err().is_some());
        assert!(rich.activity().is_active());
        assert!(rich_surface_active());
        drop(rich);
        assert!(!rich_terminal_owned());
    }

    #[test]
    fn foreground_activity_tracks_nested_and_out_of_order_claims() {
        let _lock = DIAGNOSTICS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let host = claim_host_screen();
        let host_activity = host.activity();
        assert_eq!(host_activity.run_if_active(|| 1), Some(1));
        let rich = claim_rich_surface();
        let rich_activity = rich.activity();
        assert!(!host_activity.is_active());
        assert_eq!(
            host_activity.run_if_active(|| -> () { panic!("suspended scope ran") }),
            None
        );
        let inner = claim_rich_surface();
        let inner_activity = inner.activity();
        assert!(!rich_activity.is_active());
        assert!(inner_activity.is_active());
        drop(rich);
        assert!(inner_activity.is_active());
        drop(inner);
        assert!(host_activity.is_active());
        assert!(!rich_activity.is_active());
        assert!(!inner_activity.is_active());
        drop(host);
        assert!(!host_activity.is_active());
    }

    #[test]
    fn activity_gate_serializes_work_and_nested_claim() {
        let _lock = DIAGNOSTICS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let host = claim_host_screen();
        let activity = host.activity();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (acquired_tx, acquired_rx) = std::sync::mpsc::channel();
        let thread = activity
            .run_if_active(|| {
                assert!(matches!(
                    FOREGROUND_GATE.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ));
                let thread = std::thread::spawn(move || {
                    started_tx.send(()).unwrap();
                    acquired_tx.send(claim_rich_surface()).unwrap();
                });
                started_rx.recv().unwrap();
                assert!(matches!(
                    acquired_rx.try_recv(),
                    Err(std::sync::mpsc::TryRecvError::Empty)
                ));
                thread
            })
            .unwrap();
        let nested = acquired_rx.recv().unwrap();
        thread.join().unwrap();
        assert_eq!(
            activity.run_if_active(|| -> () { panic!("outer ran after nested acquire") }),
            None
        );
        assert!(nested.activity().is_active());
        drop(nested);
        assert_eq!(activity.run_if_active(|| "resumed"), Some("resumed"));
        drop(host);
    }

    #[test]
    fn standalone_guard_activity_expires_on_drop() {
        let releases = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = Arc::clone(&releases);
        let guard = TerminalOwnershipGuard::new(move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        let activity = guard.activity();
        assert_eq!(activity.run_if_active(|| true), Some(true));
        drop(guard);
        assert!(!activity.is_active());
        assert_eq!(
            activity.run_if_active(|| -> () { panic!("released scope ran") }),
            None
        );
        assert_eq!(releases.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Recorded {
        Mode(ModeOperation),
        BeginBuffer,
        Cleanup,
        EndBuffer,
    }

    #[derive(Default)]
    struct OracleModes {
        events: Vec<Recorded>,
        fail_once: Option<ModeOperation>,
    }

    impl ModeBackend for OracleModes {
        fn apply(&mut self, operation: ModeOperation) -> io::Result<()> {
            self.events.push(Recorded::Mode(operation));
            if self.fail_once == Some(operation) {
                self.fail_once = None;
                return Err(io::Error::other("injected terminal operation failure"));
            }
            Ok(())
        }

        fn begin_buffering(&mut self) {
            self.events.push(Recorded::BeginBuffer);
        }

        fn end_buffering(&mut self) {
            self.events.push(Recorded::EndBuffer);
        }

        fn cleanup_input(&mut self, cleanup: fn()) {
            self.events.push(Recorded::Cleanup);
            cleanup();
        }
    }

    fn cleanup_first() {}

    fn cleanup_replacement() {
        panic!("first host cleanup callback must be retained");
    }

    #[test]
    fn external_cooked_scope_only_buffers_and_flushes_after_reservation() {
        let mut state = OwnershipState::default();
        let mut modes = OracleModes::default();
        state
            .acquire(Surface::External, false, None, &mut modes)
            .unwrap();
        assert_eq!(modes.events, [Recorded::BeginBuffer]);
        assert_eq!(
            (
                state.host_depth,
                state.rich_depth,
                state.mode_depth,
                state.external_depth
            ),
            (0, 0, 0, 1)
        );
        modes.events.clear();
        state.release(Surface::External, false, &mut modes);
        assert_eq!(modes.events, [Recorded::EndBuffer]);
        assert_eq!(state.external_depth, 0);
    }

    #[test]
    fn external_borrows_host_modes_without_writing_over_child_on_host_drop() {
        use ModeOperation::*;
        use Recorded::{Cleanup, EndBuffer, Mode};
        let mut state = OwnershipState::default();
        let mut modes = OracleModes::default();
        state
            .acquire(Surface::Host, true, Some(cleanup_first), &mut modes)
            .unwrap();
        modes.events.clear();
        state
            .acquire(Surface::External, true, None, &mut modes)
            .unwrap();
        assert!(modes.events.is_empty());
        state.release(Surface::Host, true, &mut modes);
        assert!(
            modes.events.is_empty(),
            "child cursor and modes remain untouched"
        );
        assert_eq!(
            (state.host_depth, state.mode_depth, state.external_depth),
            (0, 1, 1)
        );
        state.release(Surface::External, true, &mut modes);
        assert_eq!(
            modes.events,
            [
                Cleanup,
                Mode(MouseOff),
                Mode(RawOff),
                Mode(AltOff),
                Mode(Show),
                Mode(Flush),
                EndBuffer
            ]
        );
        assert_eq!((state.mode_depth, state.external_depth), (0, 0));
    }

    #[test]
    fn physical_leases_share_modes_and_restore_after_out_of_order_release() {
        use ModeOperation::*;
        use Recorded::{BeginBuffer, Cleanup, EndBuffer, Mode};
        let mut state = OwnershipState::default();
        let mut modes = OracleModes::default();
        state
            .acquire(Surface::Host, true, Some(cleanup_first), &mut modes)
            .unwrap();
        assert_eq!(
            modes.events,
            [
                Mode(RawOn),
                Mode(AltOn),
                Mode(MouseOn),
                Mode(Hide),
                Mode(Flush),
                BeginBuffer
            ]
        );
        modes.events.clear();
        state
            .acquire(Surface::Rich, true, None, &mut modes)
            .unwrap();
        state
            .acquire(Surface::Host, true, Some(cleanup_replacement), &mut modes)
            .unwrap();
        assert!(modes.events.is_empty());
        state.release(Surface::Host, true, &mut modes);
        state.release(Surface::Host, true, &mut modes);
        assert_eq!(modes.events, [Mode(Hide), Mode(Hide)]);
        assert_eq!(
            (state.host_depth, state.rich_depth, state.mode_depth),
            (0, 1, 1)
        );
        modes.events.clear();
        state.release(Surface::Rich, true, &mut modes);
        assert_eq!(
            modes.events,
            [
                Cleanup,
                Mode(MouseOff),
                Mode(RawOff),
                Mode(AltOff),
                Mode(Show),
                Mode(Flush),
                EndBuffer
            ]
        );
        assert_eq!(
            (state.host_depth, state.rich_depth, state.mode_depth),
            (0, 0, 0)
        );
        assert!(state.cleanup_input.is_none());
    }

    #[test]
    fn failed_mode_entry_rolls_back_attempts_without_claim_or_buffer() {
        use ModeOperation::*;
        for (failure, expected) in [
            (RawOn, vec![RawOn]),
            (AltOn, vec![RawOn, AltOn, AltOff, RawOff, Flush]),
            (
                MouseOn,
                vec![RawOn, AltOn, MouseOn, MouseOff, RawOff, AltOff, Flush],
            ),
            (
                Hide,
                vec![
                    RawOn, AltOn, MouseOn, Hide, MouseOff, RawOff, AltOff, Show, Flush,
                ],
            ),
            (
                Flush,
                vec![
                    RawOn, AltOn, MouseOn, Hide, Flush, MouseOff, RawOff, AltOff, Show, Flush,
                ],
            ),
        ] {
            let mut state = OwnershipState::default();
            let mut modes = OracleModes {
                fail_once: Some(failure),
                ..OracleModes::default()
            };
            let error = state
                .acquire(Surface::Host, true, Some(cleanup_first), &mut modes)
                .unwrap_err();
            assert_eq!(error.to_string(), "injected terminal operation failure");
            assert_eq!(
                modes.events,
                expected.into_iter().map(Recorded::Mode).collect::<Vec<_>>(),
                "failure: {failure:?}"
            );
            assert_eq!(
                (state.host_depth, state.rich_depth, state.mode_depth),
                (0, 0, 0)
            );
            assert!(state.cleanup_input.is_none());
            modes.events.clear();
            state
                .acquire(Surface::Rich, true, None, &mut modes)
                .unwrap();
            assert_eq!(modes.events.last(), Some(&Recorded::BeginBuffer));
            state.release(Surface::Rich, true, &mut modes);
        }
    }

    #[test]
    fn restore_continues_after_output_failure_and_finishes_buffering() {
        use ModeOperation::*;
        let mut state = OwnershipState::default();
        let mut modes = OracleModes::default();
        state
            .acquire(Surface::Rich, true, None, &mut modes)
            .unwrap();
        modes.events.clear();
        modes.fail_once = Some(MouseOff);
        state.release(Surface::Rich, true, &mut modes);
        assert_eq!(
            modes.events,
            [
                Recorded::Mode(MouseOff),
                Recorded::Mode(RawOff),
                Recorded::Mode(AltOff),
                Recorded::Mode(Show),
                Recorded::Mode(Flush),
                Recorded::EndBuffer
            ]
        );
        assert_eq!(state.mode_depth, 0);
    }

    #[test]
    fn mouse_reset_writes_all_protocols_and_flushes() {
        #[derive(Default)]
        struct Output {
            bytes: Vec<u8>,
            flushes: usize,
        }
        impl Write for Output {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                self.flushes += 1;
                Ok(())
            }
        }
        let mut out = Output::default();
        disable_mouse_capture(&mut out).unwrap();
        assert_eq!(
            out.bytes,
            b"\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l"
        );
        assert_eq!(out.flushes, 1);
    }

    #[test]
    fn nested_rich_claims_release_only_their_own_ownership() {
        let _lock = DIAGNOSTICS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(!rich_terminal_owned());
        let outer = claim_rich_surface();
        assert_eq!(
            outer.activity().run_if_active(rich_terminal_owned),
            Some(true)
        );
        let inner = claim_rich_surface();
        assert!(rich_surface_active());
        assert!(!host_screen_owned());
        drop(outer);
        assert!(rich_surface_active());
        drop(inner);
        assert!(!rich_terminal_owned());
    }

    #[test]
    fn mixed_claims_survive_out_of_order_release() {
        let _lock = DIAGNOSTICS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let host = claim_host_screen();
        let rich = claim_rich_surface();
        let nested_host = claim_host_screen();
        assert!(host_screen_owned());
        assert!(rich_surface_active());
        drop(host);
        drop(rich);
        assert!(host_screen_owned());
        assert!(!rich_surface_active());
        assert!(rich_terminal_owned());
        assert_eq!(ownership().mode_depth, 0);
        drop(nested_host);
        assert!(!rich_terminal_owned());
    }

    #[test]
    fn guard_releases_once_after_move_and_unwind() {
        let _lock = DIAGNOSTICS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let result = std::panic::catch_unwind(|| {
            let guard = claim_host_screen();
            let _moved_guard = guard;
            assert!(host_screen_owned());
            panic!("ownership unwind test");
        });
        assert!(result.is_err());
        assert!(!rich_terminal_owned());
    }
}
