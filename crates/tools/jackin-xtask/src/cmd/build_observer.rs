// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Optional archive-only host observations. They never authorize artifacts or
//! change Cargo's result, environment, inherited process group or outer deadline.

mod capture;
mod snapshot;
#[cfg(test)]
mod tests;

use std::{
    fs::File,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};

const INTERVAL: Duration = Duration::from_secs(20);
const SAMPLE_BUDGET: Duration = Duration::from_millis(500);
const MAX_SAMPLES: usize = 64;
const MAX_BYTES: usize = 16 * 1024;
const TRUNCATED: &[u8] = b"jackin-build-observer truncated=1\n";

#[expect(
    clippy::disallowed_methods,
    reason = "archive observation polls synchronous host-side xtask work, outside runtime/render threads"
)]
pub(super) fn run(cmd: &mut Command, path: &Path) -> Result<()> {
    let display = super::display_command(cmd);
    let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    // Keep the SDK-owned group. A detached Cargo group would escape its timeout.
    let mut child = cmd
        .stdout(Stdio::from(file))
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("running {display}"))?;
    let mut reporter = Reporter::new(Instant::now());
    loop {
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                capture::stop(&mut child, Instant::now() + Duration::from_millis(50));
                return Err(error).with_context(|| format!("running {display}"));
            }
        };
        if let Some(status) = status {
            return if status.success() {
                Ok(())
            } else {
                Err(anyhow!("{display} failed with {status}"))
            };
        }
        let now = Instant::now();
        if reporter.due(now) {
            let stats = snapshot::read(child.id(), now + SAMPLE_BUDGET);
            if let Some(line) = reporter.record(Instant::now(), stats) {
                // A broken diagnostic sink must not become a Cargo failure.
                drop(std::io::stderr().lock().write_all(&line));
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
}

struct Reporter {
    started: Instant,
    next: Instant,
    samples: usize,
    bytes: usize,
    stopped: bool,
}

impl Reporter {
    const fn new(started: Instant) -> Self {
        Self {
            started,
            next: started,
            samples: 0,
            bytes: 0,
            stopped: false,
        }
    }

    fn due(&self, now: Instant) -> bool {
        !self.stopped && now >= self.next
    }

    fn record(&mut self, now: Instant, stats: Option<snapshot::Stats>) -> Option<Vec<u8>> {
        if !self.due(now) {
            return None;
        }
        self.next = now + INTERVAL;
        let line = format!(
            "jackin-build-observer observed_ms={} available_parallelism={} stats={}\n",
            now.saturating_duration_since(self.started).as_millis(),
            thread::available_parallelism().map_or(0, std::num::NonZeroUsize::get),
            stats.map_or_else(|| "unknown".to_owned(), |value| value.line()),
        )
        .into_bytes();
        if self.samples >= MAX_SAMPLES - 1 || self.bytes + line.len() + TRUNCATED.len() > MAX_BYTES
        {
            self.stopped = true;
            if self.samples < MAX_SAMPLES && self.bytes + TRUNCATED.len() <= MAX_BYTES {
                self.samples += 1;
                self.bytes += TRUNCATED.len();
                return Some(TRUNCATED.to_vec());
            }
            return None;
        }
        self.samples += 1;
        self.bytes += line.len();
        Some(line)
    }
}
