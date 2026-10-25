// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn usage_relay_ready_frame_is_canonical_and_bounded() {
    let ready = UsageRelayReadyV1::canonical();
    let frame = ready.encode_frame().unwrap();
    assert_eq!(
        frame,
        b"{\"kind\":\"usage_relay_ready\",\"protocol\":\"usage-relay-owner-v2\",\"version\":1}\n"
    );
    assert!(frame.len() <= USAGE_RELAY_READY_MAX_FRAME_BYTES);
    assert_eq!(UsageRelayReadyV1::decode_frame(&frame).unwrap(), ready);
}

#[test]
fn usage_relay_ready_frame_rejects_noncanonical_or_open_schema_values() {
    for frame in [
        &b"{\"kind\":\"usage_relay_ready\",\"protocol\":\"usage-relay-owner-v2\",\"version\":1}"[..],
        &b"{\"kind\":\"usage_relay_ready\",\"protocol\":\"usage-relay-owner-v2\",\"version\":1}\n\n"[..],
        &b"{ \"kind\":\"usage_relay_ready\",\"protocol\":\"usage-relay-owner-v2\",\"version\":1}\n"[..],
        &b"{\"kind\":\"usage_relay_ready\",\"protocol\":\"usage-relay-owner-v2\",\"version\":1,\"extra\":false}\n"[..],
        &b"{\"kind\":\"usage_relay_ready\",\"protocol\":\"usage-relay-owner-v2\",\"version\":2}\n"[..],
        &b"{\"kind\":\"usage_relay_ready\",\"protocol\":\"usage-relay-owner-v1\",\"version\":1}\n"[..],
        &b"{\"kind\":\"usage_relay_ready\",\"kind\":\"usage_relay_ready\",\"protocol\":\"usage-relay-owner-v2\",\"version\":1}\n"[..],
        &b"{\"kind\":\"usage_relay_ready\",\"protocol\":\"usage-relay-owner-v2\",\"version\":1}\ntrailing"[..],
    ] {
        assert!(
            UsageRelayReadyV1::decode_frame(frame).is_err(),
            "unexpectedly accepted {:?}",
            String::from_utf8_lossy(frame)
        );
    }

    let _error = UsageRelayReadyV1::decode_frame(&[b'x'; USAGE_RELAY_READY_MAX_FRAME_BYTES + 1])
        .unwrap_err();
}
