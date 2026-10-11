// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn write_fixture_crate(root: &Path, group: &str, crate_name: &str, body: &str) {
    let dir = root.join("crates").join(group).join(crate_name);
    fs::create_dir_all(dir.join("src")).expect("mkdir");
    fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"{crate_name}\"\n"),
    )
    .expect("write manifest");
    fs::write(dir.join("src/lib.rs"), body).expect("write lib");
}
