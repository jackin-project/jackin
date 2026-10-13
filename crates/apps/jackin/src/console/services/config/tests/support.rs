// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn workspace_file_contents(paths: &JackinPaths, name: &str) -> String {
    std::fs::read_to_string(paths.workspaces_dir.join(format!("{name}.toml"))).unwrap()
}

pub(super) fn cursor_credentials_fixture(home: &std::path::Path) {
    std::fs::create_dir_all(home.join(".cursor")).unwrap();
    std::fs::write(
        home.join(".cursor/auth.json"),
        r#"{"accessToken":"fixture"}"#,
    )
    .unwrap();
}

pub(super) fn poll_scan_to_ready(
    rx: &mut BlockingSubscription<(u64, Result<AccountScanOutcome, String>)>,
) -> (u64, Result<AccountScanOutcome, String>) {
    // Spin (no thread sleep: banned repo-wide): the worker is local
    // filesystem I/O and lands in milliseconds.
    for _ in 0..10_000_000 {
        match rx.poll_next() {
            SubscriptionPoll::Ready(result) => return result,
            SubscriptionPoll::Closed => panic!("scan worker dropped"),
            SubscriptionPoll::Pending => std::hint::spin_loop(),
        }
    }
    panic!("scan worker timed out");
}
