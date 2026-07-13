#![cfg(feature = "otlp")]

use opentelemetry::trace::{SpanContext, SpanId, TraceFlags, TraceId, TraceState};

use super::{Screen, format_traceparent};

#[test]
fn screen_names_are_stable() {
    assert_eq!(Screen::List.as_str(), "list");
    assert_eq!(Screen::Capsule.as_str(), "capsule");
    assert_eq!(Screen::Launch.as_str(), "launch");
}

#[test]
fn traceparent_is_w3c_format() {
    let ctx = SpanContext::new(
        TraceId::from_bytes([
            0x0a, 0xf7, 0x65, 0x19, 0x16, 0xcd, 0x43, 0xdd, 0x84, 0x48, 0xeb, 0x21, 0x1c, 0x80,
            0x31, 0x9c,
        ]),
        SpanId::from_bytes([0xb7, 0xad, 0x6b, 0x71, 0x69, 0x20, 0x33, 0x31]),
        TraceFlags::SAMPLED,
        true,
        TraceState::default(),
    );
    assert_eq!(
        format_traceparent(&ctx),
        "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
    );
}
