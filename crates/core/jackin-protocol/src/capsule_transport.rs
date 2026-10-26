// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context as _, Result, bail};
#[cfg(unix)]
use nix::errno::Errno;
#[cfg(unix)]
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
#[cfg(unix)]
use nix::sys::socket::{MsgFlags, recv};
#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::os::fd::{AsFd, AsRawFd};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

pub const CONTROL_PROTOCOL_MAJOR: u16 = 2;

pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

const TRANSPORT_TAG: u8 = 0x7f;
const IMPOSSIBLE_LEGACY_LENGTH: [u8; 4] = [0xff; 4];
const MAGIC_LEN: usize = 8;
const PREFIX_LEN: usize = 5;
const PACKET_LEN: usize = PREFIX_LEN + MAGIC_LEN + 2;
const PREFACE_MAGIC: [u8; MAGIC_LEN] = *b"JCKCAPS1";
const ACK_MAGIC: [u8; MAGIC_LEN] = *b"JCKACK01";
const NAK_MAGIC: [u8; MAGIC_LEN] = *b"JCKNAK01";

fn packet(magic: [u8; MAGIC_LEN], major: u16) -> [u8; PACKET_LEN] {
    [
        TRANSPORT_TAG,
        IMPOSSIBLE_LEGACY_LENGTH[0],
        IMPOSSIBLE_LEGACY_LENGTH[1],
        IMPOSSIBLE_LEGACY_LENGTH[2],
        IMPOSSIBLE_LEGACY_LENGTH[3],
        magic[0],
        magic[1],
        magic[2],
        magic[3],
        magic[4],
        magic[5],
        magic[6],
        magic[7],
        (major >> 8) as u8,
        major as u8,
    ]
}

fn has_magic(packet: &[u8; PACKET_LEN], magic: [u8; MAGIC_LEN]) -> bool {
    packet
        .iter()
        .copied()
        .skip(PREFIX_LEN)
        .take(MAGIC_LEN)
        .eq(magic.iter().copied())
}

fn packet_major(packet: &[u8; PACKET_LEN]) -> u16 {
    let mut bytes = packet.iter().rev().copied();
    let low = bytes.next().unwrap_or_default();
    let high = bytes.next().unwrap_or_default();
    u16::from_be_bytes([high, low])
}

fn validate_ack(reply: &[u8; PACKET_LEN], expected_major: u16) -> Result<()> {
    if *reply == packet(ACK_MAGIC, expected_major) {
        return Ok(());
    }
    if has_magic(reply, NAK_MAGIC) {
        bail!(
            "Capsule daemon rejected transport protocol major {}; client requires major {}",
            packet_major(reply),
            expected_major
        );
    }
    if has_magic(reply, ACK_MAGIC) {
        bail!(
            "Capsule daemon ACKed transport protocol major {}; client requires major {}",
            packet_major(reply),
            expected_major
        );
    }
    bail!("Capsule daemon returned an invalid transport protocol ACK")
}

#[cfg(unix)]
pub fn client_handshake(stream: &mut UnixStream, timeout: Duration) -> Result<()> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .context("Capsule transport timeout is too large")?;
    write_all_until(
        stream,
        &packet(PREFACE_MAGIC, CONTROL_PROTOCOL_MAJOR),
        deadline,
    )
    .context("writing Capsule transport preface")?;
    stream
        .flush()
        .context("flushing Capsule transport preface")?;
    let mut reply = [0; PACKET_LEN];
    read_exact_until(stream, &mut reply, deadline).context("reading Capsule transport ACK")?;
    validate_ack(&reply, CONTROL_PROTOCOL_MAJOR)
}

#[cfg(unix)]
fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .context("Capsule transport handshake timed out")
}

#[cfg(unix)]
fn write_all_until(stream: &mut UnixStream, bytes: &[u8], deadline: Instant) -> Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        stream.set_write_timeout(Some(remaining(deadline)?))?;
        let remaining_bytes = bytes
            .get(offset..)
            .context("Capsule transport preface write offset exceeded buffer")?;
        match stream.write(remaining_bytes) {
            Ok(0) => bail!("Capsule transport preface write returned zero bytes"),
            Ok(count) => offset += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn poll_timeout(timeout: Duration) -> PollTimeout {
    let milliseconds = timeout.as_millis().saturating_add(u128::from(
        !timeout.subsec_nanos().is_multiple_of(1_000_000),
    ));
    if milliseconds >= i32::MAX as u128 {
        PollTimeout::MAX
    } else {
        PollTimeout::try_from(milliseconds as u32).unwrap_or(PollTimeout::MAX)
    }
}

#[cfg(unix)]
fn read_exact_until(stream: &mut UnixStream, bytes: &mut [u8], deadline: Instant) -> Result<()> {
    let mut offset = 0;
    let mut terminal_readiness = false;
    while offset < bytes.len() {
        remaining(deadline)?;
        let remaining_bytes = bytes
            .get_mut(offset..)
            .context("Capsule transport ACK read offset exceeded buffer")?;
        match recv(stream.as_raw_fd(), remaining_bytes, MsgFlags::MSG_DONTWAIT) {
            Ok(0) => bail!("Capsule transport ACK ended before all bytes arrived"),
            Ok(count) => offset += count,
            Err(Errno::EINTR) => {}
            Err(Errno::EAGAIN) if terminal_readiness => {
                bail!("Capsule transport ACK socket closed before all bytes arrived")
            }
            Err(Errno::EAGAIN) => {
                let mut descriptors = [PollFd::new(stream.as_fd(), PollFlags::POLLIN)];
                match poll(&mut descriptors, poll_timeout(remaining(deadline)?)) {
                    Ok(0) => {
                        remaining(deadline)?;
                    }
                    Ok(_) => {
                        let readiness = descriptors[0]
                            .revents()
                            .context("Capsule transport ACK poll returned unknown flags")?;
                        remaining(deadline)?;
                        if readiness.contains(PollFlags::POLLNVAL) {
                            bail!("Capsule transport ACK socket became invalid");
                        }
                        terminal_readiness =
                            readiness.intersects(PollFlags::POLLHUP | PollFlags::POLLERR);
                        if !readiness
                            .intersects(PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR)
                        {
                            bail!(
                                "Capsule transport ACK poll returned unexpected flags: {readiness:?}"
                            );
                        }
                    }
                    Err(Errno::EINTR) => {
                        remaining(deadline)?;
                    }
                    Err(error) => return Err(error).context("polling Capsule transport ACK"),
                }
            }
            Err(error) => return Err(error).context("reading Capsule transport ACK"),
        }
    }
    Ok(())
}

pub async fn client_handshake_async<S>(stream: &mut S) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        stream
            .write_all(&packet(PREFACE_MAGIC, CONTROL_PROTOCOL_MAJOR))
            .await
            .context("writing Capsule transport preface")?;
        stream
            .flush()
            .await
            .context("flushing Capsule transport preface")?;

        let mut reply = [0; PACKET_LEN];
        stream
            .read_exact(&mut reply)
            .await
            .context("reading Capsule transport ACK")?;
        validate_ack(&reply, CONTROL_PROTOCOL_MAJOR)
    })
    .await
    .context("timed out negotiating Capsule transport")?
}

pub async fn server_handshake_async<S>(stream: &mut S) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut preface = [0; PACKET_LEN];
    stream
        .read_exact(&mut preface)
        .await
        .context("reading Capsule transport preface")?;

    let expected = packet(PREFACE_MAGIC, CONTROL_PROTOCOL_MAJOR);
    if preface != expected {
        stream
            .write_all(&packet(NAK_MAGIC, CONTROL_PROTOCOL_MAJOR))
            .await
            .context("writing Capsule transport NAK")?;
        stream
            .flush()
            .await
            .context("flushing Capsule transport NAK")?;
        if has_magic(&preface, PREFACE_MAGIC) {
            bail!(
                "peer requested Capsule transport protocol major {}; daemon requires major {}",
                packet_major(&preface),
                CONTROL_PROTOCOL_MAJOR
            );
        }
        bail!("peer sent a malformed or legacy Capsule transport preface");
    }

    stream
        .write_all(&packet(ACK_MAGIC, CONTROL_PROTOCOL_MAJOR))
        .await
        .context("writing Capsule transport ACK")?;
    stream
        .flush()
        .await
        .context("flushing Capsule transport ACK")?;
    Ok(())
}
