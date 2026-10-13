// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn proc_info(
    pid: u32,
    pgid: u32,
    tpgid: i32,
    exe_path: Option<&str>,
    comm: &str,
    cmdline: &[&str],
) -> ProcessInfo {
    ProcessInfo {
        pid,
        pgid,
        tpgid,
        cmdline: cmdline.iter().map(|part| (*part).to_owned()).collect(),
        exe_path: exe_path.map(PathBuf::from),
        comm: comm.to_owned(),
    }
}
