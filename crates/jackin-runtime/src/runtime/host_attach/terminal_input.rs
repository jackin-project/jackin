// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Session-owned terminal input. Cancellation wakes the reader before joining
//! it, so an idle terminal cannot hold Tokio's blocking-pool shutdown forever.
//! This owns the only reader for the input descriptor during attachment;
//! readiness followed by a blocking read requires that exclusive ownership.

use std::fs::File;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream as BlockingStream;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::thread::JoinHandle;

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use tokio::io::{AsyncRead, ReadBuf};
use tokio::net::UnixStream;

pub(super) struct TerminalInput {
    stream: UnixStream,
    cancellation: BlockingStream,
    worker: Option<JoinHandle<io::Result<()>>>,
}

impl TerminalInput {
    pub(super) fn stdin() -> io::Result<Self> {
        // Duplicating preserves the descriptor's flags and does not take
        // ownership of the process's standard-input descriptor.
        Self::from_reader(File::from(io::stdin().as_fd().try_clone_to_owned()?))
    }

    fn from_reader<I: Read + AsFd + Send + 'static>(input: I) -> io::Result<Self> {
        let (receiver, sender) = BlockingStream::pair()?;
        receiver.set_nonblocking(true)?;
        let cancellation = sender.try_clone()?;
        let stream = UnixStream::from_std(receiver)?;
        let worker = jackin_telemetry::spawn::thread_joined_named(
            "jackin-terminal-input".into(),
            move || forward_input(input, sender),
        )?;
        Ok(Self {
            stream,
            cancellation,
            worker: Some(worker),
        })
    }

    fn join_worker(&mut self) -> io::Result<()> {
        self.worker.take().map_or(Ok(()), |worker| {
            worker
                .join()
                .map_err(|_| io::Error::other("terminal input worker panicked"))?
        })
    }
}

impl AsyncRead for TerminalInput {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buffer.filled().len();
        match Pin::new(&mut self.stream).poll_read(context, buffer) {
            Poll::Ready(Ok(())) if buffer.filled().len() == before && buffer.remaining() != 0 => {
                Poll::Ready(self.join_worker())
            }
            result => result,
        }
    }
}

impl Drop for TerminalInput {
    fn drop(&mut self) {
        // Wake either poll or a backpressured socket write before joining.
        let _shutdown = self.cancellation.shutdown(Shutdown::Both);
        let _joined = self.join_worker();
    }
}

fn forward_input<I: Read + AsFd>(input: I, mut sender: BlockingStream) -> io::Result<()> {
    let result = forward_input_inner(input, &mut sender);
    // The session keeps a duplicate writer for cancellation. Explicitly send
    // EOF when this worker finishes so that duplicate cannot hide read errors
    // or input EOF from the async receiver.
    let _shutdown = sender.shutdown(Shutdown::Write);
    result
}

fn forward_input_inner<I: Read + AsFd>(
    mut input: I,
    sender: &mut BlockingStream,
) -> io::Result<()> {
    // A peer shutdown need not interrupt a blocking write on every supported
    // kernel. Keep writes nonblocking and observe cancellation in the poll set
    // even while the async reader has stopped draining forwarded input.
    sender.set_nonblocking(true)?;
    let mut bytes = [0u8; 4096];
    let mut pending = 0..0;
    loop {
        let writing = !pending.is_empty();
        let mut ready = [
            PollFd::new(
                sender.as_fd(),
                if writing {
                    PollFlags::POLLIN | PollFlags::POLLOUT
                } else {
                    PollFlags::POLLIN
                },
            ),
            PollFd::new(input.as_fd(), PollFlags::POLLIN),
        ];
        let descriptors = if writing {
            &mut ready[..1]
        } else {
            &mut ready[..]
        };
        match poll(descriptors, PollTimeout::NONE) {
            Err(nix::errno::Errno::EINTR) => continue,
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        let socket_events = ready[0].revents().unwrap_or_else(PollFlags::empty);
        if socket_events.intersects(
            PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR | PollFlags::POLLNVAL,
        ) {
            return Ok(());
        }
        if writing {
            if !socket_events.contains(PollFlags::POLLOUT) {
                continue;
            }
            match sender.write(&bytes[pending.clone()]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "terminal forwarding socket closed",
                    ));
                }
                Ok(count) => pending.start += count,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) => {}
                Err(error) => return Err(error),
            }
            continue;
        }
        let count = match input.read(&mut bytes) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            return Ok(());
        }
        pending = 0..count;
    }
}

#[cfg(test)]
mod tests;
