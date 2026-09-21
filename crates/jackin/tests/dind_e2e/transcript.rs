//! PTY transcript helpers: spawn per-stream pipe collectors that drain into
//! `Arc<Mutex<Vec<u8>>>` buffers, plus substring / deadline-based waiters
//! used by the `pty_runner` family.

#![expect(
    clippy::expect_used,
    clippy::disallowed_methods,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]
use std::io::{Read, Write as _};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

pub(super) fn spawn_pipe_collector<R>(
    reader: R,
) -> (Arc<Mutex<Vec<u8>>>, std::thread::JoinHandle<()>)
where
    R: Read + Send + 'static,
{
    collect_pipe(reader, None)
}

pub(super) fn spawn_logged_pipe_collector<R>(
    reader: R,
    path: &std::path::Path,
) -> (Arc<Mutex<Vec<u8>>>, std::thread::JoinHandle<()>)
where
    R: Read + Send + 'static,
{
    let log = std::fs::File::create(path).expect("create live launch transcript");
    collect_pipe(reader, Some(log))
}

/// Live session log BSD `script` writes on macOS (see `pty_command`).
pub(super) const MACOS_TYPESCRIPT_NAME: &str = "e2e-typescript.log";

/// Spawn the stdout collector for a `script(1)` child.
///
/// BSD `script` on macOS block-buffers its stdout when it is a pipe: zero
/// bytes arrive while the child is alive and the whole transcript flushes
/// only at exit, so live matching must follow the typescript file (written
/// per-frame) instead. Elsewhere the pipe itself is live and used directly.
pub(super) fn spawn_stdout_collector<R>(
    reader: R,
    cwd: &std::path::Path,
    done: &Arc<AtomicBool>,
) -> (Arc<Mutex<Vec<u8>>>, std::thread::JoinHandle<()>)
where
    R: Read + Send + 'static,
{
    #[cfg(target_os = "macos")]
    {
        let path = cwd.join(MACOS_TYPESCRIPT_NAME);
        drop(std::fs::remove_file(&path));
        // Drain the buffered pipe so `script` never blocks on it; the bytes
        // are discarded because the typescript is the transcript source.
        let (_, pipe_handle) = collect_pipe(reader, None);
        let log = std::fs::File::create(cwd.join("e2e-launch-stdout.log"))
            .expect("create live launch transcript");
        let (buffer, follow_handle) = spawn_typescript_follower(path, log, Arc::clone(done));
        let handle = std::thread::spawn(move || {
            pipe_handle.join().expect("stdout pipe drain must finish");
            follow_handle
                .join()
                .expect("typescript follower must finish");
        });
        (buffer, handle)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = done;
        spawn_logged_pipe_collector(reader, &cwd.join("e2e-launch-stdout.log"))
    }
}

#[cfg(target_os = "macos")]
fn spawn_typescript_follower(
    path: std::path::PathBuf,
    mut log: std::fs::File,
    done: Arc<AtomicBool>,
) -> (Arc<Mutex<Vec<u8>>>, std::thread::JoinHandle<()>) {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let thread_buffer = Arc::clone(&buffer);
    let handle = std::thread::spawn(move || {
        let mut offset = 0_u64;
        loop {
            if done.load(Ordering::Relaxed) {
                // The child has exited, so the typescript is complete:
                // replace any delta-appended prefix with the whole file.
                if let Ok(full) = std::fs::read(&path) {
                    use std::io::Seek as _;
                    log.set_len(0).expect("truncate live launch transcript");
                    log.rewind().expect("rewind live launch transcript");
                    log.write_all(&full).expect("write live launch transcript");
                    *thread_buffer
                        .lock()
                        .expect("pty output buffer mutex must not be poisoned") = full;
                }
                break;
            }
            if let Some((bytes, next)) = read_typescript_delta(&path, offset) {
                offset = next;
                if !bytes.is_empty() {
                    log.write_all(&bytes).expect("write live launch transcript");
                    thread_buffer
                        .lock()
                        .expect("pty output buffer mutex must not be poisoned")
                        .extend_from_slice(&bytes);
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    (buffer, handle)
}

#[cfg(target_os = "macos")]
fn read_typescript_delta(path: &std::path::Path, offset: u64) -> Option<(Vec<u8>, u64)> {
    use std::io::Seek as _;
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    // The file only grows within one run; a shorter file means it was
    // recreated, so restart from the beginning instead of wedging.
    let offset = if len < offset { 0 } else { offset };
    file.seek(std::io::SeekFrom::Start(offset)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let next = offset + bytes.len() as u64;
    Some((bytes, next))
}

fn collect_pipe<R>(
    mut reader: R,
    mut log: Option<std::fs::File>,
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
                Ok(n) => {
                    if let Some(log) = &mut log {
                        log.write_all(&chunk[..n])
                            .expect("write live launch transcript");
                    }
                    thread_buffer
                        .lock()
                        .expect("pty output buffer mutex must not be poisoned")
                        .extend_from_slice(&chunk[..n]);
                }
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
