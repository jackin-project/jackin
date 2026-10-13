// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Async spinner-wait helper for polling operations.
//!
//! Animates a braille spinner on stderr while polling an async function.
//! Silences itself when the rich launch cockpit owns the terminal so the
//! spinner never streams over the alternate screen.

#![expect(
    clippy::print_stderr,
    reason = "spinner redraws intentional terminal control sequences on stderr"
)]

use std::io::{self, Write as _};

use jackin_brand::{PHOSPHOR_DIM, PHOSPHOR_GREEN, owo_rgb};
use jackin_diagnostics::{is_debug_mode, rich_terminal_owned};

/// Display a spinner while waiting, returning when `poll` returns `Ok(())`.
///
/// `poll` is called up to `max_attempts` times with `interval` between calls.
/// The spinner animates smoothly independent of the poll interval.
pub async fn spin_wait<F, Fut>(
    message: &str,
    max_attempts: u32,
    interval: std::time::Duration,
    mut poll: F,
) -> anyhow::Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    spin_wait_with_intervals(message, max_attempts, |_| interval, &mut poll).await
}

pub async fn spin_wait_ramped<F, Fut>(
    message: &str,
    max_attempts: u32,
    initial_interval: std::time::Duration,
    max_interval: std::time::Duration,
    mut poll: F,
) -> anyhow::Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    spin_wait_with_intervals(
        message,
        max_attempts,
        |attempt| ramped_interval(initial_interval, max_interval, attempt),
        &mut poll,
    )
    .await
}

fn ramped_interval(
    initial: std::time::Duration,
    cap: std::time::Duration,
    attempt: u32,
) -> std::time::Duration {
    let factor = 1_u32.checked_shl(attempt).unwrap_or(u32::MAX);
    initial.saturating_mul(factor).min(cap)
}

async fn spin_wait_with_intervals<F, Fut>(
    message: &str,
    max_attempts: u32,
    mut interval_for_attempt: impl FnMut(u32) -> std::time::Duration,
    poll: &mut F,
) -> anyhow::Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    use owo_colors::OwoColorize as _;

    const FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    const SPIN_MS: u64 = 80;
    let mg = owo_rgb(PHOSPHOR_GREEN);
    let mut last_err = None;
    let mut frame_idx: usize = 0;

    let debug = is_debug_mode();
    // A full-screen rich surface (the launch cockpit) owns the terminal —
    // its own waiting animation conveys progress, so the spinner must stay
    // silent or it streams over the alternate screen.
    let suppressed = rich_terminal_owned();
    for attempt in 0..max_attempts {
        if debug && !suppressed {
            eprint!("\r\x1b[2K");
            drop(io::stderr().flush());
        }
        match poll().await {
            Ok(()) => {
                if !suppressed {
                    eprint!("\r\x1b[2K");
                    drop(io::stderr().flush());
                }
                return Ok(());
            }
            Err(e) => last_err = Some(e),
        }
        let mut remaining = interval_for_attempt(attempt);
        while !remaining.is_zero() {
            if !suppressed {
                let frame = FRAMES[frame_idx % FRAMES.len()];
                eprint!(
                    "\r   {}   {}",
                    frame.color(mg).bold(),
                    message.color(owo_rgb(PHOSPHOR_DIM)).bold()
                );
                drop(io::stderr().flush());
            }
            let sleep_for = remaining.min(std::time::Duration::from_millis(SPIN_MS));
            tokio::time::sleep(sleep_for).await;
            remaining = remaining.saturating_sub(sleep_for);
            frame_idx += 1;
        }
    }
    if !suppressed {
        eprint!("\r\x1b[2K");
        drop(io::stderr().flush());
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("timed out: {message}")))
}

#[cfg(test)]
mod tests;
