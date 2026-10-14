// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub fn open_url(url: &str) -> anyhow::Result<()> {
    open::that_detached(url)?;
    Ok(())
}
