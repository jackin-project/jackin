// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[expect(
    clippy::unwrap_used,
    reason = "filesystem race fixture must fail immediately if the expected syscall boundary is absent"
)]
pub(super) fn canonicalize_with_appearing_directory(
    ancestor: &Path,
    raced: &Path,
    retry_calls: &mut usize,
) -> io::Result<PathBuf> {
    let result = std::fs::canonicalize(ancestor);
    if ancestor == raced {
        *retry_calls += 1;
        if *retry_calls == 1 {
            assert_eq!(result.as_ref().unwrap_err().kind(), io::ErrorKind::NotFound);
            // Actual filesystem creation at the vulnerable syscall boundary.
            std::fs::create_dir(raced).unwrap();
        }
    }
    result
}

pub(super) fn open_after_barrier(
    barrier: &std::sync::Barrier,
    directory: &Path,
    key: &str,
) -> io::Result<std::fs::File> {
    barrier.wait();
    open_in_namespace(directory, key)
}
