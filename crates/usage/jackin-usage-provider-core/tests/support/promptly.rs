// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub(crate) fn promptly<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = mpsc::channel();
    let _worker = thread::spawn(move || {
        let _sent = sender.send(work());
    });
    receiver
        .recv_timeout(Duration::from_secs(20))
        .map_err(|error| error.to_string())
}
