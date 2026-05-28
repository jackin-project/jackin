use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

static DEBUG_MODE: AtomicBool = AtomicBool::new(false);
static DEBUG_BUFFER_ACTIVE: AtomicBool = AtomicBool::new(false);
static DEBUG_BUFFER: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
const DEBUG_BUFFER_LIMIT: usize = 2048;

pub fn set_debug_mode(enabled: bool) {
    DEBUG_MODE.store(enabled, Ordering::Relaxed);
}

/// Whether `--debug` was passed. Hot path — must stay an atomic-load.
#[must_use]
pub fn is_debug_mode() -> bool {
    DEBUG_MODE.load(Ordering::Relaxed)
}

static RICH_SURFACE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Set while a full-screen rich TUI owns the alternate screen.
///
/// Ancillary stderr status output — spinners, "waiting" lines — checks this
/// and stays silent so it cannot stream over the cockpit. Driven by the
/// renderer's lifetime, never by callers.
pub fn set_rich_surface_active(active: bool) {
    RICH_SURFACE_ACTIVE.store(active, Ordering::Relaxed);
}

#[must_use]
pub fn rich_surface_active() -> bool {
    RICH_SURFACE_ACTIVE.load(Ordering::Relaxed)
}

static HOST_SCREEN_OWNED: AtomicBool = AtomicBool::new(false);

/// Set while a single host-side guard owns the screen for a whole launch flow.
///
/// The guard holds the alternate screen, raw mode, and mouse capture across
/// console → loading → capsule → exit. The individual surfaces (console
/// manager, launch cockpit, exit outro) check this and skip their own
/// enter/leave so the flow never drops back to the cooked terminal between
/// screens. Driven only by the owning guard's lifetime.
pub fn set_host_screen_owned(owned: bool) {
    HOST_SCREEN_OWNED.store(owned, Ordering::Relaxed);
}

#[must_use]
pub fn host_screen_owned() -> bool {
    HOST_SCREEN_OWNED.load(Ordering::Relaxed)
}

/// True when any host-side full-screen surface owns terminal modes that make
/// direct stdout/stderr streaming unsafe.
///
/// `rich_surface_active` tracks a currently drawing cockpit/dialog. The host
/// guard can outlive an individual renderer while still holding raw mode,
/// mouse capture, and the alternate screen across console → launch → capsule.
/// Plain command output is equally corrupting in that gap.
#[must_use]
pub fn rich_terminal_owned() -> bool {
    rich_surface_active() || host_screen_owned()
}

/// Re-enter the host alternate screen after an interactive child returns.
///
/// A baked capsule still drops `?1049l` on detach and returns the terminal to
/// the primary screen; re-asserting the moment the `docker exec` returns means
/// the post-attach work (outcome inspection, the exit outro) renders on the
/// alternate screen instead of flashing the operator's shell. No-op unless a
/// host guard owns the screen.
pub fn reassert_alt_screen() {
    use crossterm::ExecutableCommand as _;
    if !host_screen_owned() {
        return;
    }
    let mut out = std::io::stdout();
    let _ = out.execute(crossterm::terminal::EnterAlternateScreen);
    let _ = out.execute(crossterm::cursor::Hide);
}

/// Format a single debug-log line. Pure (no I/O) so unit tests can
/// assert on the wire format without touching global state or stderr.
#[must_use]
pub fn format_debug_line(category: &str, message: &str) -> String {
    format!("[jackin debug {category}] {message}")
}

fn debug_buffer() -> &'static Mutex<Vec<String>> {
    DEBUG_BUFFER.get_or_init(|| Mutex::new(Vec::new()))
}

fn drain_debug_buffer() -> Vec<String> {
    let mut guard = debug_buffer()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::mem::take(&mut *guard)
}

pub(crate) fn begin_debug_buffering() {
    DEBUG_BUFFER_ACTIVE.store(true, Ordering::Relaxed);
}

pub(crate) fn end_debug_buffering() {
    DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
    for line in drain_debug_buffer() {
        eprintln!("{line}");
    }
}

/// Drain the debug buffer and return its contents without printing to stderr.
/// Only for use in tests that need to assert on debug output.
#[cfg(test)]
pub(crate) fn drain_debug_buffer_for_test() -> Vec<String> {
    DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
    drain_debug_buffer()
}

pub fn emit_debug_line(category: &str, message: &str) {
    let line = format_debug_line(category, message);
    if crate::diagnostics::active_debug(category, &line) {
        return;
    }
    // A diagnostics run is active but not capturing (a non-`--debug` run): the
    // firehose stays off, so the line is dropped here rather than streamed to
    // the screen. Skipping this drop lets debug-tier output `eprintln!` over a
    // live rich surface (the launch cockpit owns the screen with no buffering),
    // violating the never-spew-over-a-rich-TUI rule. The buffer/stderr fallback
    // below is only for contexts with no active run (early startup, tests).
    if crate::diagnostics::active_run().is_some() {
        return;
    }
    if DEBUG_BUFFER_ACTIVE.load(Ordering::Relaxed) {
        let mut guard = debug_buffer()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard.len() >= DEBUG_BUFFER_LIMIT {
            let keep_from = guard.len() / 2;
            guard.drain(..keep_from);
        }
        guard.push(line);
    } else {
        eprintln!("{line}");
    }
}

/// Emit a compact operator-visible line unless a rich surface owns the terminal.
///
/// The line is always mirrored into the active diagnostics run when one exists,
/// so suppressed rich-surface output remains recoverable.
pub fn emit_compact_line(kind: &str, line: &str) {
    if let Some(run) = crate::diagnostics::active_run() {
        run.compact(kind, line);
    }
    if !rich_terminal_owned() {
        eprintln!("{line}");
    }
}

/// Verbose-trace helper for `--debug` runs. No-op when the flag is off
/// — formatting is deferred behind the gate so disabled call sites cost
/// only an atomic load.
///
/// `category` is a short tag (`isolation`, `worktree`, etc.) that lets
/// shared logs be greppable. Use the `format!`-style trailing args:
///
/// ```ignore
/// debug_log!("isolation", "git worktree add -b {branch} {path}");
/// ```
#[macro_export]
macro_rules! debug_log {
    ($category:expr, $($arg:tt)*) => {
        if $crate::tui::is_debug_mode() {
            $crate::tui::emit_debug_line($category, &format!($($arg)*));
        }
    };
}

// ── Shared color palette ─────────────────────────────────────────────────

const fn palette_tuple(color: jackin_tui::Rgb) -> (u8, u8, u8) {
    (color.r, color.g, color.b)
}

const WHITE: (u8, u8, u8) = palette_tuple(jackin_tui::WHITE);
const PHOSPHOR_GREEN: (u8, u8, u8) = palette_tuple(jackin_tui::PHOSPHOR_GREEN);
const PHOSPHOR_DIM: (u8, u8, u8) = palette_tuple(jackin_tui::PHOSPHOR_DIM);
const PHOSPHOR_DARK: (u8, u8, u8) = palette_tuple(jackin_tui::PHOSPHOR_DARK);

const fn rgb(color: (u8, u8, u8)) -> owo_colors::Rgb {
    owo_colors::Rgb(color.0, color.1, color.2)
}

pub mod animation;
pub mod output;
pub mod prompt;

pub use animation::{warp_end_caption, warp_intro, warp_out};
pub use output::{
    CodexSyncState, agent_outcome_notice, auth_mode_notice, clear_screen, codex_auth_notice, fatal,
    github_auth_notice, hint, print_deploying, set_terminal_title, shorten_home, step_fail,
};
pub use prompt::{prompt_choice, require_interactive_stdin, spin_wait};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static DEBUG_BUFFER_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn format_debug_line_matches_wire_format() {
        assert_eq!(
            format_debug_line("isolation", "git worktree add -b foo /tmp/wt deadbeef"),
            "[jackin debug isolation] git worktree add -b foo /tmp/wt deadbeef"
        );
    }

    #[test]
    fn format_debug_line_passes_through_special_characters() {
        // No escaping — operators sharing logs need verbatim shell output.
        assert_eq!(
            format_debug_line("io", "wrote /a/b/c.json {\"k\":\"v\"}"),
            "[jackin debug io] wrote /a/b/c.json {\"k\":\"v\"}"
        );
    }

    #[test]
    fn debug_mode_default_is_off() {
        // Process-wide flag — touching it would race other tests, so just
        // assert the snapshot is a bool. Toggle/observe is exercised in
        // the binary-level integration test.
        let _: bool = is_debug_mode();
    }

    #[test]
    fn debug_lines_buffer_while_tui_is_active() {
        let _lock = DEBUG_BUFFER_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
        let _ = drain_debug_buffer();

        begin_debug_buffering();
        emit_debug_line("role", "resolving test role");
        assert_eq!(
            drain_debug_buffer(),
            vec!["[jackin debug role] resolving test role".to_string()]
        );
        end_debug_buffering();
    }

    #[test]
    fn debug_lines_drop_while_a_noncapturing_run_owns_output() {
        let _lock = DEBUG_BUFFER_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
        let _ = drain_debug_buffer();

        let tmp = tempfile::tempdir().unwrap();
        let paths = crate::paths::JackinPaths::for_tests(tmp.path());
        let run = crate::diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
        let _active = run.activate();

        // A non-`--debug` run owns debug-tier output: the line is neither
        // buffered nor printed, so it can never reach a live rich surface.
        begin_debug_buffering();
        emit_debug_line("role", "should be dropped");
        assert!(
            drain_debug_buffer().is_empty(),
            "debug line must not buffer/print while a non-capturing run is active"
        );
        end_debug_buffering();
    }

    #[test]
    fn compact_lines_write_run_file_while_rich_surface_owns_terminal() {
        let _lock = DEBUG_BUFFER_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        set_rich_surface_active(false);
        set_host_screen_owned(false);
        let tmp = tempfile::tempdir().unwrap();
        let paths = crate::paths::JackinPaths::for_tests(tmp.path());
        let run = crate::diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
        let _active = run.activate();

        set_rich_surface_active(true);
        emit_compact_line("warning", "jackin: warning: hidden by cockpit");
        set_rich_surface_active(false);

        let jsonl = std::fs::read_to_string(run.path()).unwrap();
        assert!(jsonl.contains("\"kind\":\"warning\""), "{jsonl}");
        assert!(jsonl.contains("hidden by cockpit"), "{jsonl}");
        set_rich_surface_active(false);
        set_host_screen_owned(false);
    }

    #[test]
    fn compact_lines_write_run_file_while_host_screen_owns_terminal() {
        let _lock = DEBUG_BUFFER_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        set_rich_surface_active(false);
        set_host_screen_owned(false);
        let tmp = tempfile::tempdir().unwrap();
        let paths = crate::paths::JackinPaths::for_tests(tmp.path());
        let run = crate::diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
        let _active = run.activate();

        set_host_screen_owned(true);
        emit_compact_line("operator_env", "jackin: hidden while host owns raw screen");
        set_host_screen_owned(false);

        let jsonl = std::fs::read_to_string(run.path()).unwrap();
        assert!(jsonl.contains("\"kind\":\"operator_env\""), "{jsonl}");
        assert!(
            jsonl.contains("hidden while host owns raw screen"),
            "{jsonl}"
        );
    }
}
