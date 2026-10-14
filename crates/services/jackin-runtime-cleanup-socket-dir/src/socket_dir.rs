// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::print_stderr,
    reason = "runtime cleanup and GC report operator-visible warnings and results"
)]

use jackin_core::JackinPaths;
use jackin_runtime_coordination::coordination::ensure_prunable;
use jackin_runtime_isolation::isolation::safe_remove::safe_remove_dir_contained;

pub async fn remove_socket_dir(paths: &JackinPaths, container_name: &str) {
    let paths = paths.clone();
    let dir = paths.jackin_home.join("sockets").join(container_name);
    let displayed = dir.clone();
    let result = jackin_telemetry::spawn::joined_blocking(move || {
        ensure_prunable(&paths, &dir)
            .and_then(|()| safe_remove_dir_contained(&paths.jackin_home, &dir))
    })
    .await;
    let error = match result {
        Ok(Ok(())) => return,
        Ok(Err(error)) => error,
        Err(error) => std::io::Error::other(error),
    };
    eprintln!(
        "jackin: warning: failed to remove socket dir {}: {error}",
        displayed.display()
    );
}
