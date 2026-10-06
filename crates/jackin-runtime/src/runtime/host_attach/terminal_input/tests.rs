// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use tokio::io::AsyncReadExt as _;

#[test]
fn idle_terminal_reader_joins_when_session_and_runtime_end() {
    let (input, _open_terminal) = BlockingStream::pair().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut reader = TerminalInput::from_reader(input).unwrap();
        let mut byte = [0];
        tokio::time::timeout(std::time::Duration::from_millis(20), reader.read(&mut byte))
            .await
            .unwrap_err();
        drop(reader);
    });
    drop(runtime);
}

#[tokio::test]
async fn terminal_input_preserves_bytes_and_eof() {
    let (input, mut terminal) = BlockingStream::pair().unwrap();
    let mut reader = TerminalInput::from_reader(input).unwrap();
    terminal.write_all(b"hello").unwrap();
    terminal.shutdown(Shutdown::Write).unwrap();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, b"hello");
}

#[tokio::test]
async fn cancellation_wakes_a_backpressured_forwarding_socket() {
    use std::io::Seek as _;
    use std::time::Duration;

    let mut input = tempfile::tempfile().unwrap();
    input.write_all(&vec![b'x'; 1_048_576]).unwrap();
    input.rewind().unwrap();
    let (_receiver, sender) = BlockingStream::pair().unwrap();
    nix::sys::socket::setsockopt(&sender, nix::sys::socket::sockopt::SndBuf, &4096).unwrap();
    let observer = sender.try_clone().unwrap();
    let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
    let worker = std::thread::spawn(move || {
        let result = forward_input(input, sender);
        let _sent = finished_tx.send(result);
    });

    // Inspect kernel write readiness, proving the forwarding socket really
    // became backpressured rather than relying on a scheduling delay.
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let mut ready = [PollFd::new(observer.as_fd(), PollFlags::POLLOUT)];
            poll(&mut ready, PollTimeout::ZERO).unwrap();
            if ready[0]
                .revents()
                .is_none_or(|events| !events.contains(PollFlags::POLLOUT))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("forwarding socket should fill");

    // Cancel the writer endpoint, matching TerminalInput ownership.
    observer.shutdown(Shutdown::Both).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), finished_rx)
        .await
        .expect("cancellation must release the forwarding worker")
        .unwrap();
    if let Err(error) = result {
        assert!(matches!(
            error.kind(),
            io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
        ));
    }
    worker.join().unwrap();
}

struct FailingInput(BlockingStream);

impl AsFd for FailingInput {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl Read for FailingInput {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "injected input failure",
        ))
    }
}

#[tokio::test]
async fn terminal_input_propagates_read_errors_instead_of_eof() {
    let (input, mut terminal) = BlockingStream::pair().unwrap();
    let mut reader = TerminalInput::from_reader(FailingInput(input)).unwrap();
    terminal.write_all(b"ready").unwrap();
    let mut byte = [0];
    let error = tokio::time::timeout(std::time::Duration::from_secs(2), reader.read(&mut byte))
        .await
        .expect("input failure must reach the async reader")
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(error.to_string(), "injected input failure");
}
