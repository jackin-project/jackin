// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! In-container clipboard asset staging for host-affordance bridge frames.
//!
//! The host may claim an image format, but the Capsule validates magic bytes
//! before writing a container-readable path under jackin❯'s runtime root.

use std::collections::HashMap;
use std::fs::{
    OpenOptions, create_dir_all, read_dir, remove_file, set_permissions, symlink_metadata,
};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use jackin_core::container_paths;
use jackin_core::{Clock, SystemClock};
use jackin_protocol::attach::{
    ClipboardImage, ClipboardImageChunk, ClipboardImageEnd, ClipboardImageFormat,
    ClipboardImageStart, FILE_EXPORT_DIGEST_BYTES, MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES,
    MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES_U64,
};
use sha2::{Digest, Sha256};

pub(crate) const CLIPBOARD_RUN_DIR: &str = container_paths::CLIPBOARD_DIR;
pub(crate) const CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT: Duration = Duration::from_mins(5);

pub(crate) fn stage_clipboard_image(image: &ClipboardImage) -> Result<PathBuf> {
    stage_clipboard_image_at(Path::new(CLIPBOARD_RUN_DIR), image)
}

pub(crate) fn cleanup_clipboard_run_dir() {
    if let Err(err) = cleanup_clipboard_run_dir_at(Path::new(CLIPBOARD_RUN_DIR)) {
        crate::clog!("clipboard-image: cleanup failed: {err:#}");
    }
}

#[derive(Debug)]
pub(crate) struct ClipboardImageTransfers {
    active: HashMap<u64, ActiveClipboardImageTransfer>,
    clock: Arc<dyn Clock>,
}

impl Default for ClipboardImageTransfers {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct ActiveClipboardImageTransfer {
    format: ClipboardImageFormat,
    expected_size: u64,
    bytes: Vec<u8>,
    hasher: Sha256,
    last_activity: Instant,
}

impl ClipboardImageTransfers {
    pub(crate) fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }

    pub(crate) fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            active: HashMap::new(),
            clock,
        }
    }

    pub(crate) fn start(&mut self, start: ClipboardImageStart) -> Result<()> {
        if self.active.contains_key(&start.transfer_id) {
            bail!(
                "clipboard image transfer {} already active",
                start.transfer_id
            );
        }
        if start.size == 0 {
            bail!("clipboard image transfer is empty");
        }
        if start.size > MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES_U64 {
            bail!(
                "clipboard image transfer {} bytes exceeds cap {MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES}",
                start.size
            );
        }
        self.active.insert(
            start.transfer_id,
            ActiveClipboardImageTransfer {
                format: start.format,
                expected_size: start.size,
                bytes: Vec::with_capacity(start.size.min(1024 * 1024) as usize),
                hasher: Sha256::new(),
                last_activity: self.clock.now(),
            },
        );
        Ok(())
    }

    pub(crate) fn chunk(&mut self, chunk: ClipboardImageChunk) -> Result<()> {
        let transfer_id = chunk.transfer_id;
        let result = self.chunk_inner(chunk);
        if result.is_err() {
            self.active.remove(&transfer_id);
        }
        result
    }

    fn chunk_inner(&mut self, chunk: ClipboardImageChunk) -> Result<()> {
        let Some(active) = self.active.get_mut(&chunk.transfer_id) else {
            bail!(
                "clipboard image transfer {} has no active start",
                chunk.transfer_id
            );
        };
        let written = u64::try_from(active.bytes.len())
            .map_err(|_| anyhow::anyhow!("clipboard image byte count overflow"))?;
        if chunk.offset != written {
            bail!(
                "clipboard image transfer {} offset {} did not match expected {}",
                chunk.transfer_id,
                chunk.offset,
                written
            );
        }
        let chunk_len = u64::try_from(chunk.bytes.len())
            .map_err(|_| anyhow::anyhow!("clipboard image chunk length overflow"))?;
        let new_written = written
            .checked_add(chunk_len)
            .ok_or_else(|| anyhow::anyhow!("clipboard image byte count overflow"))?;
        if new_written > active.expected_size {
            bail!(
                "clipboard image transfer {} wrote {new_written} bytes, expected {}",
                chunk.transfer_id,
                active.expected_size
            );
        }
        active.hasher.update(&chunk.bytes);
        active.bytes.extend_from_slice(&chunk.bytes);
        active.last_activity = self.clock.now();
        Ok(())
    }

    pub(crate) fn end(&mut self, end: ClipboardImageEnd) -> Result<ClipboardImage> {
        let Some(active) = self.active.remove(&end.transfer_id) else {
            bail!(
                "clipboard image transfer {} has no active start",
                end.transfer_id
            );
        };
        let written = u64::try_from(active.bytes.len())
            .map_err(|_| anyhow::anyhow!("clipboard image byte count overflow"))?;
        if written != active.expected_size {
            bail!(
                "clipboard image transfer {} ended after {written} bytes, expected {}",
                end.transfer_id,
                active.expected_size
            );
        }
        let actual: [u8; FILE_EXPORT_DIGEST_BYTES] = active.hasher.finalize().into();
        if actual != end.sha256 {
            bail!(
                "clipboard image transfer {} SHA-256 mismatch",
                end.transfer_id
            );
        }
        let image = ClipboardImage {
            format: active.format,
            bytes: active.bytes,
        };
        validate_image_magic(&image)?;
        Ok(image)
    }

    pub(crate) fn abort_idle_older_than(&mut self, max_idle: Duration) -> usize {
        let now = self.clock.now();
        let cutoff = now.checked_sub(max_idle).unwrap_or(now);
        self.abort_idle_before(cutoff)
    }

    fn abort_idle_before(&mut self, cutoff: Instant) -> usize {
        let stale_ids: Vec<u64> = self
            .active
            .iter()
            .filter_map(|(transfer_id, active)| {
                (active.last_activity <= cutoff).then_some(*transfer_id)
            })
            .collect();
        let count = stale_ids.len();
        for transfer_id in stale_ids {
            if let Some(active) = self.active.remove(&transfer_id) {
                crate::cdebug!(
                    "clipboard-image: abort stale transfer id={} format={:?} buffered={} expected={}",
                    transfer_id,
                    active.format,
                    active.bytes.len(),
                    active.expected_size
                );
            }
        }
        count
    }
}

fn stage_clipboard_image_at(root: &Path, image: &ClipboardImage) -> Result<PathBuf> {
    validate_image_magic(image)?;
    create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
    set_permissions(root, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("setting permissions on {}", root.display()))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for attempt in 0..16u8 {
        let path = root.join(format!(
            "clipboard-{now}-{}-{attempt}.{}",
            std::process::id(),
            image.format.extension()
        ));
        #[expect(
            clippy::disallowed_methods,
            reason = "clipboard staging is an explicit bounded host-affordance action, not render emission"
        )]
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(&image.bytes)
                    .with_context(|| format!("writing {}", path.display()))?;
                file.flush()
                    .with_context(|| format!("flushing {}", path.display()))?;
                return Ok(path);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err).with_context(|| format!("opening {}", path.display())),
        }
    }
    bail!(
        "could not allocate unique clipboard image path under {}",
        root.display()
    );
}

fn cleanup_clipboard_run_dir_at(root: &Path) -> Result<usize> {
    let entries = match read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(err) => return Err(err).with_context(|| format!("reading {}", root.display())),
    };

    let mut removed = 0usize;
    for entry in entries {
        let entry = entry.with_context(|| format!("reading entry in {}", root.display()))?;
        let path = entry.path();
        let meta = symlink_metadata(&path)
            .with_context(|| format!("reading metadata for {}", path.display()))?;
        if meta.is_file() || meta.file_type().is_symlink() {
            remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            removed += 1;
        } else {
            crate::clog!(
                "clipboard-image: leaving non-file staged entry during cleanup: {}",
                path.display()
            );
        }
    }
    Ok(removed)
}

fn validate_image_magic(image: &ClipboardImage) -> Result<()> {
    if image.bytes.is_empty() {
        bail!("clipboard image payload is empty");
    }
    let ok = match image.format {
        ClipboardImageFormat::Png => image.bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        ClipboardImageFormat::Jpeg => image.bytes.starts_with(&[0xff, 0xd8, 0xff]),
        ClipboardImageFormat::Gif => {
            image.bytes.starts_with(b"GIF87a") || image.bytes.starts_with(b"GIF89a")
        }
        ClipboardImageFormat::Webp => {
            image.bytes.len() >= 12
                && image.bytes.starts_with(b"RIFF")
                && &image.bytes[8..12] == b"WEBP"
        }
        ClipboardImageFormat::Tiff => {
            image.bytes.starts_with(b"MM\0*") || image.bytes.starts_with(b"II*\0")
        }
    };
    if !ok {
        bail!(
            "clipboard image magic bytes do not match {:?}",
            image.format
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
