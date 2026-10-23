// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use super::{
    ACK_MAGIC, CONTROL_PROTOCOL_MAJOR, PACKET_LEN, packet, read_exact_until, validate_ack,
};
use std::io::Write as _;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

#[test]
fn buffered_ack_remains_readable_after_peer_closes() -> anyhow::Result<()> {
    let (mut writer, mut reader) = UnixStream::pair()?;
    writer.write_all(&packet(ACK_MAGIC, CONTROL_PROTOCOL_MAJOR))?;
    drop(writer);

    let mut reply = [0; PACKET_LEN];
    let result = read_exact_until(
        &mut reader,
        &mut reply,
        Instant::now() + Duration::from_secs(2),
    );
    anyhow::ensure!(
        result.is_ok(),
        "reading buffered Capsule ACK failed: {}",
        result
            .err()
            .map_or_else(|| "no error".to_owned(), |error| format!("{error:#}"))
    );

    anyhow::ensure!(
        reply == packet(ACK_MAGIC, CONTROL_PROTOCOL_MAJOR),
        "buffered ACK bytes differ: {reply:?}"
    );
    validate_ack(&reply, CONTROL_PROTOCOL_MAJOR)?;
    Ok(())
}

#[test]
fn truncated_closed_ack_is_rejected() -> anyhow::Result<()> {
    let (mut writer, mut reader) = UnixStream::pair()?;
    let ack = packet(ACK_MAGIC, CONTROL_PROTOCOL_MAJOR);
    writer.write_all(&ack[..3])?;
    drop(writer);

    let mut reply = [0; PACKET_LEN];
    let result = read_exact_until(
        &mut reader,
        &mut reply,
        Instant::now() + Duration::from_secs(2),
    );
    anyhow::ensure!(result.is_err(), "a truncated ACK must fail: {result:?}");
    Ok(())
}

#[test]
fn malformed_and_wrong_major_acks_are_rejected() {
    assert!(validate_ack(&[0; PACKET_LEN], CONTROL_PROTOCOL_MAJOR).is_err());
    let wrong_major = packet(ACK_MAGIC, CONTROL_PROTOCOL_MAJOR + 1);
    assert!(validate_ack(&wrong_major, CONTROL_PROTOCOL_MAJOR).is_err());
}

#[test]
fn silent_and_partial_peers_expire_at_original_deadline() -> anyhow::Result<()> {
    for partial in [false, true] {
        let (mut writer, mut reader) = UnixStream::pair()?;
        if partial {
            let ack = packet(ACK_MAGIC, CONTROL_PROTOCOL_MAJOR);
            writer.write_all(&ack[..3])?;
        }

        let deadline = Instant::now() + Duration::from_millis(25);
        let mut reply = [0; PACKET_LEN];
        let result = read_exact_until(&mut reader, &mut reply, deadline);
        anyhow::ensure!(
            result.as_ref().is_err_and(
                |error| format!("{error:#}").contains("Capsule transport handshake timed out")
            ),
            "expected deadline expiry, got {result:?}"
        );
    }
    Ok(())
}

#[test]
fn already_expired_deadline_rejects_buffered_ack() -> anyhow::Result<()> {
    let (mut writer, mut reader) = UnixStream::pair()?;
    writer.write_all(&packet(ACK_MAGIC, CONTROL_PROTOCOL_MAJOR))?;
    drop(writer);

    let Some(deadline) = Instant::now().checked_sub(Duration::from_millis(1)) else {
        anyhow::bail!("the monotonic clock has not advanced one millisecond");
    };
    let mut reply = [0; PACKET_LEN];
    let result = read_exact_until(&mut reader, &mut reply, deadline);
    anyhow::ensure!(result.is_err(), "expired deadline must reject buffered ACK");
    Ok(())
}
