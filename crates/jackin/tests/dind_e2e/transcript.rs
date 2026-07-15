//! PTY transcript helpers: spawn per-stream pipe collectors that drain into
//! `Arc<Mutex<Vec<u8>>>` buffers, plus substring / deadline-based waiters
//! used by the `pty_runner` family.

#![expect(
    clippy::expect_used,
    clippy::disallowed_methods,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]
use std::io::Read;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

pub(super) fn spawn_pipe_collector<R>(
    mut reader: R,
) -> (Arc<Mutex<Vec<u8>>>, std::thread::JoinHandle<()>)
where
    R: Read + Send + 'static,
{
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let thread_buffer = Arc::clone(&buffer);
    let handle = std::thread::spawn(move || {
        let mut chunk = [0_u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => thread_buffer
                    .lock()
                    .expect("pty output buffer mutex must not be poisoned")
                    .extend_from_slice(&chunk[..n]),
            }
        }
    });
    (buffer, handle)
}

pub(super) fn wait_for_transcript_text(
    buffer: &Arc<Mutex<Vec<u8>>>,
    needle: &str,
    done: &AtomicBool,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline && !done.load(Ordering::Relaxed) {
        if transcript_contains(buffer, needle) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

pub(super) fn transcript_contains(buffer: &Arc<Mutex<Vec<u8>>>, needle: &str) -> bool {
    String::from_utf8_lossy(
        &buffer
            .lock()
            .expect("pty output buffer mutex must not be poisoned"),
    )
    .contains(needle)
}

pub(super) fn buffer_bytes(buffer: &Arc<Mutex<Vec<u8>>>) -> Vec<u8> {
    buffer
        .lock()
        .expect("pty output buffer mutex must not be poisoned")
        .clone()
}
