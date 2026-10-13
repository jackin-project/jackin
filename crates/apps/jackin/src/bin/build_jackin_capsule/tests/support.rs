// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[cfg(unix)]
pub(super) fn fake_mise(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = root.join("fake mise");
    std::fs::write(
        &path,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> .fake-mise-args\nprintf '%s\\n' '--END-COMMAND--' >> .fake-mise-args\npwd > .fake-mise-cwd\nprintf '%s\\n' \"$$\" > .fake-mise-pid\nulimit -S -n > .fake-mise-fd-limit\nprintf '%s\\n' ready > .fake-mise-ready\nif [ \"$5\" = \"list\" ]; then\n  if [ -f .fake-rustup-list-failure ]; then exit 41; fi\n  if [ -f .fake-rustup-installed-targets ]; then cat .fake-rustup-installed-targets; fi\n  exit 0\nfi\nif [ -f .fake-mise-wait ]; then exec sleep 30; fi\nexit 37\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&path, permissions).unwrap();
    path
}

#[cfg(unix)]
#[expect(
    clippy::disallowed_methods,
    reason = "test helper reads fixture ulimits via a shell probe"
)]
pub(super) fn expected_fd_limit_after_best_effort_raise() -> String {
    let output = process::Command::new("sh")
        .args(["-c", "ulimit -S -n; ulimit -H -n"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let limits = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let soft = limits.first().unwrap();
    let hard = limits.get(1).unwrap();
    if hard == "unlimited" || hard.parse::<u64>().map_or(true, |limit| limit >= 20_480) {
        "20480".to_owned()
    } else {
        soft.clone()
    }
}
