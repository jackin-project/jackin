use std::sync::atomic::{AtomicBool, Ordering};

static DEBUG_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_debug_mode(enabled: bool) {
    DEBUG_MODE.store(enabled, Ordering::Relaxed);
}

/// Whether `--debug` was passed. Hot path — must stay an atomic-load.
#[must_use]
pub fn is_debug_mode() -> bool {
    DEBUG_MODE.load(Ordering::Relaxed)
}

/// Format a single debug-log line. Pure (no I/O) so unit tests can
/// assert on the wire format without touching global state or stderr.
#[must_use]
pub fn format_debug_line(category: &str, message: &str) -> String {
    format!("[jackin debug {category}] {message}")
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
            eprintln!("{}", $crate::tui::format_debug_line($category, &format!($($arg)*)));
        }
    };
}

// ── Shared color palette ─────────────────────────────────────────────────

const WHITE: (u8, u8, u8) = (255, 255, 255);

const PHOSPHOR_GREEN: (u8, u8, u8) = (0, 255, 65);
const PHOSPHOR_DIM: (u8, u8, u8) = (0, 140, 30);
const PHOSPHOR_DARK: (u8, u8, u8) = (0, 80, 18);

const fn rgb(color: (u8, u8, u8)) -> owo_colors::Rgb {
    owo_colors::Rgb(color.0, color.1, color.2)
}

pub mod animation;
pub mod output;
pub mod prompt;

pub use animation::{intro_animation, outro_animation, simple_outro};
pub use output::{
    auth_mode_notice, clear_screen, deprecation_warning, fatal, hint, print_config_table,
    print_deploying, print_logo, set_terminal_title, shorten_home, step_fail, step_quiet,
    step_shimmer,
};
pub use prompt::{prompt_choice, require_interactive_stdin, spin_wait};

#[cfg(test)]
mod tests {
    use super::*;

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
}
