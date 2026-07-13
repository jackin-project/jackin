//! Debug-log port trait (A5 unblock, D2 pattern).
//!
//! Domain crates (L0) emit verbose-trace messages without depending
//! on the L2 diagnostics layer. The trait is defined here so the
//! type system can enforce the no-direct-edge invariant; the real
//! impl lives in `jackin-diagnostics` and is registered at process
//! init time via [`set_global_sink`].
//!
//! The default state is a no-op sink (anything that runs before
//! the entry layer registers the real impl still compiles and
//! passes, just with no log output — observable behaviour matches
//! what the binary's tests need today, and live invocations register
//! the real sink via `jackin_diagnostics::install_debug_log_sink`
//! before the CLI starts processing the first command).
//!
//! Architecture Invariant: depends only on `std::sync` and
//! `std::fmt`. No `jackin-*` deps.

use std::sync::{OnceLock, RwLock};

/// Receives verbose-trace debug log lines when `--debug` is active.
pub trait DebugLogSink: Send + Sync + 'static {
    /// True iff verbose-trace logging is enabled for this run.
    fn is_active(&self) -> bool;
    /// Append one debug-log line.
    fn log(&self, category: &str, line: &str);
}

static GLOBAL_SINK: OnceLock<RwLock<Box<dyn DebugLogSink>>> = OnceLock::new();

fn global() -> &'static RwLock<Box<dyn DebugLogSink>> {
    GLOBAL_SINK.get_or_init(|| RwLock::new(Box::new(NoopSink)))
}

/// Install the process-wide debug-log sink (typically at startup).
pub fn set_global_sink(sink: Box<dyn DebugLogSink>) {
    *global().write().unwrap_or_else(PoisonError::into_inner) = sink;
}

/// Whether the installed sink reports verbose-trace logging as active.
pub fn is_debug_mode() -> bool {
    global()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .is_active()
}

use std::sync::PoisonError;

/// Emit one debug-log line if the sink is active; no-op otherwise.
pub fn emit_debug_line(category: &str, line: &str) {
    if let Ok(guard) = global().read() {
        let sink = &**guard;
        if sink.is_active() {
            sink.log(category, line);
        }
    }
}

/// Verbose-trace helper for `--debug` runs. No-op when the flag is off.
///
/// `category` is a short tag (`isolation`, `worktree`, etc.) that keeps shared
/// logs greppable. Use `format!`-style trailing args:
///
/// ```ignore
/// debug_log!("isolation", "git worktree add -b {branch} {path}");
/// ```
#[macro_export]
macro_rules! debug_log {
    ($category:expr, $($arg:tt)*) => {
        if $crate::debug_log::is_debug_mode() {
            $crate::debug_log::emit_debug_line($category, &::std::format!($($arg)*));
        }
    };
}

struct NoopSink;
impl DebugLogSink for NoopSink {
    fn is_active(&self) -> bool {
        false
    }
    fn log(&self, _category: &str, _line: &str) {}
}
