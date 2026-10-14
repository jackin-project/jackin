// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::JackinPaths;
use jackin_runtime_cleanup_prune_dir::prune_dir::prune_dir;
use jackin_runtime_coordination::coordination::ensure_prunable;

pub fn prune_roles(paths: &JackinPaths) -> anyhow::Result<()> {
    ensure_prunable(paths, &paths.roles_dir)?;
    prune_dir(
        &paths.roles_dir,
        "Role Cache",
        "removing cached role repositories",
        "role cache",
    )
}
