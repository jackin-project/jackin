// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host browser side-effect adapter for console input.

pub fn open_url(url: &str) -> anyhow::Result<()> {
    #[cfg(unix)]
    let _native_spawn = jackin_process_directory::native_spawn_guard();
    open::that_detached(url)?;
    Ok(())
}
