// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Mandatory version negotiation for the shared Capsule control/attach socket.
//!
//! Every socket user exchanges this fixed preface before writing a control
//! frame or attach Hello. The reserved `0x7f || 0xffff_ffff` prefix is invalid
//! to the legacy attach decoder, so an old daemon cannot interpret it as a
//! request. No compatibility or downgrade path exists.

use anyhow::{Context as _, Result, bail};
#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

/// Major version shared by the Capsule control and attach socket transports.
pub const CONTROL_PROTOCOL_MAJOR: u16 = 2;

/// Maximum time spent sending a preface and waiting for its exact ACK.
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

/// Negotiate the Capsule transport on a synchronous Unix stream.
///
/// `timeout` bounds the full send-and-ACK exchange. Socket timeouts are
/// reduced after every partial read or write, so a peer cannot renew the
/// deadline by trickling bytes.
///
/// # Errors
///
/// Returns an error when writing the preface fails, the peer closes or times
/// out, or the peer does not ACK the exact major version.
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
fn read_exact_until(stream: &mut UnixStream, bytes: &mut [u8], deadline: Instant) -> Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        let remaining_bytes = bytes
            .get_mut(offset..)
            .context("Capsule transport ACK read offset exceeded buffer")?;
        match stream.read(remaining_bytes) {
            Ok(0) => bail!("Capsule transport ACK ended before all bytes arrived"),
            Ok(count) => offset += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// Negotiate the Capsule transport on an asynchronous stream.
///
/// Returns only after receiving the exact ACK for this client's major version;
/// callers must not send application bytes before it succeeds.
///
/// # Errors
///
/// Returns an error when the preface write or ACK read fails, times out, or the
/// peer does not ACK the exact major version.
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

/// Validate a client's preface and ACK it before the daemon routes the socket.
///
/// The caller must bound this operation with a deadline. Invalid, legacy, and
/// mismatched peers receive a fixed-length NAK when the stream can still be
/// written, then return an error without reaching either request decoder.
///
/// # Errors
///
/// Returns an error when the peer sends a missing, malformed, or mismatched
/// preface, or when reading/writing negotiation bytes fails.
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
