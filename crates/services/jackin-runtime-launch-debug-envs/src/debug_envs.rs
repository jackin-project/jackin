// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Debug env strings for the launch run args.
//!
//! [`debug_runtime_envs`] reports the extra `-e` entries the
//! debug switch contributes to the container run args
//! (currently none: file-backed debug configuration must
//! not propagate into the container).

/// Extra container env entries contributed by the debug switch.
///
/// Currently always empty: debug configuration from files
/// must not propagate into the container (the hub launch
/// suite pins this: `debug_runtime_envs_do_not_propagate_file_configuration`).
pub fn debug_runtime_envs(_debug: bool) -> Vec<String> {
    Vec::new()
}
