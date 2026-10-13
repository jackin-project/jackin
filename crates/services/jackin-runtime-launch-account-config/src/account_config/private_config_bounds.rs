// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Resource bounds for private provider configuration input and serialization.

use std::fs::File;
use std::io::Read as _;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use fs4::{FileExt, TryLockError};

pub(super) const MAX_CONFIG_BYTES: usize = 1024 * 1024;
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const LOCK_POLL: Duration = Duration::from_millis(25);

pub(super) fn ensure_size(size: u64, name: &str, limit: usize) -> anyhow::Result<()> {
    anyhow::ensure!(
        size <= limit as u64,
        "private provider config {name} exceeds {limit} byte limit"
    );
    Ok(())
}

pub(super) fn read(file: &mut File, name: &str, limit: usize) -> anyhow::Result<Vec<u8>> {
    read_with_hook(file, name, limit, || Ok(()))
}

/// Stat limits sparse files before allocation; the bounded reader also catches
/// growth after stat. Neither input size nor concurrent append can grow the
/// number of bytes read beyond the configured limit plus one detection byte.
pub(super) fn read_with_hook<F>(
    file: &mut File,
    name: &str,
    limit: usize,
    after_stat: F,
) -> anyhow::Result<Vec<u8>>
where
    F: FnOnce() -> anyhow::Result<()>,
{
    let size = file
        .metadata()
        .context("stat private provider config input")?
        .len();
    ensure_size(size, name, limit)?;
    after_stat()?;
    let mut contents = Vec::new();
    file.take((limit as u64).saturating_add(1))
        .read_to_end(&mut contents)
        .with_context(|| format!("read private provider config {name}"))?;
    ensure_size(contents.len() as u64, name, limit)?;
    Ok(contents)
}

pub(super) fn lock(file: &File) -> anyhow::Result<()> {
    lock_with_timeout(file, LOCK_TIMEOUT)
}

pub(super) fn lock_with_timeout(file: &File, timeout: Duration) -> anyhow::Result<()> {
    let started = Instant::now();
    let mut first_attempt = true;
    loop {
        anyhow::ensure!(
            first_attempt || started.elapsed() < timeout,
            "timed out locking private provider config directory"
        );
        first_attempt = false;
        match FileExt::try_lock(file) {
            Ok(()) => return Ok(()),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(error)) => {
                return Err(error).context("lock private provider config directory");
            }
        }
        let remaining = timeout.saturating_sub(started.elapsed());
        anyhow::ensure!(
            !remaining.is_zero(),
            "timed out locking private provider config directory"
        );
        std::thread::park_timeout(LOCK_POLL.min(remaining));
    }
}
