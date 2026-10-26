// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::MetadataExt,
    path::Path,
};

use anyhow::{Context, Result, ensure};
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};

use super::super::process::SetupControls;

pub(super) const RUST_TOOLCHAIN_FILE_LIMIT: u64 = 1024 * 1024;
const BOUNDED_READ_CHUNK_BYTES: usize = 64 * 1024;

pub(super) fn rust_release(path: &Path, controls: &SetupControls) -> Result<String> {
    let contents = read_bounded_regular_file(
        path,
        controls,
        RUST_TOOLCHAIN_FILE_LIMIT,
        "staged rust-toolchain.toml",
        "staged rust-toolchain.toml exceeds its byte limit",
    )?;
    let contents = String::from_utf8(contents).context("decoding staged rust-toolchain.toml")?;
    let value: toml::Value =
        toml::from_str(&contents).context("parsing staged rust-toolchain.toml")?;
    controls.check_start()?;
    let channel = value
        .get("toolchain")
        .and_then(|toolchain| toolchain.get("channel"))
        .and_then(toml::Value::as_str)
        .context("staged rust-toolchain.toml has no toolchain.channel")?;
    ensure!(
        channel.split('.').count() == 3
            && channel
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())),
        "hook requires an exact numeric Rust release pin"
    );
    Ok(channel.to_owned())
}

pub(super) fn read_bounded_regular_file(
    path: &Path,
    controls: &SetupControls,
    byte_limit: u64,
    description: &str,
    oversized_message: &str,
) -> Result<Vec<u8>> {
    controls.check_start()?;
    let before = fs::metadata(path).with_context(|| format!("inspecting {description}"))?;
    ensure!(
        before.file_type().is_file(),
        "{description} is not a regular file"
    );
    ensure!(before.len() <= byte_limit, "{oversized_message}");

    let mut file = File::from(
        open(
            path,
            OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .with_context(|| format!("opening {description}"))?,
    );
    let opened = file
        .metadata()
        .with_context(|| format!("restating {description}"))?;
    ensure!(
        same_file_state(&before, &opened),
        "{description} changed while opening"
    );

    let capacity = usize::try_from(before.len()).context("converting bounded file size")?;
    let mut contents = Vec::with_capacity(capacity);
    let mut buffer = vec![0_u8; BOUNDED_READ_CHUNK_BYTES];
    loop {
        controls.check_start()?;
        let count = file
            .read(&mut buffer)
            .with_context(|| format!("reading {description}"))?;
        if count == 0 {
            break;
        }
        let current_length =
            u64::try_from(contents.len()).context("converting bounded file byte count")?;
        let next_length = u64::try_from(count)
            .context("converting bounded read length")?
            .checked_add(current_length)
            .context("counting bounded file bytes")?;
        ensure!(next_length <= byte_limit, "{oversized_message}");
        contents.extend_from_slice(&buffer[..count]);
    }
    controls.check_start()?;

    let after_fd = file
        .metadata()
        .with_context(|| format!("restating {description}"))?;
    let after_path = fs::metadata(path).with_context(|| format!("restating {description}"))?;
    ensure!(
        u64::try_from(contents.len()).context("converting bounded file byte count")?
            == before.len()
            && same_file_state(&before, &after_fd)
            && same_file_state(&before, &after_path),
        "{description} changed while reading"
    );
    Ok(contents)
}

fn same_file_state(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.file_type().is_file()
        && right.file_type().is_file()
        && left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.mode() == right.mode()
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
}
