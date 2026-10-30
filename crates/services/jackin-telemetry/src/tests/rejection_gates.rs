// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::event::{Rejection, Value};
use crate::health::{Signal, facade_health, record_export_rejection};
use crate::limits::{clamp_body, redact_and_clamp, validate_name};
use crate::privacy::validate_string;

#[test]
fn credential_like_values_are_rejected() {
    assert_eq!(
        validate_string("db password=hunter2"),
        Err(Rejection::Privacy)
    );
    assert_eq!(
        validate_string("header Authorization: Bearer abc"),
        Err(Rejection::Privacy)
    );
}

#[test]
fn benign_values_are_accepted() {
    assert_eq!(validate_string("worker started"), Ok(()));
    assert_eq!(validate_string("sessions:2"), Ok(()));
}

#[test]
fn names_must_be_nonempty_and_bounded() {
    assert_eq!(validate_name(""), Err(Rejection::SizeLimit));
    assert_eq!(validate_name(&"x".repeat(129)), Err(Rejection::SizeLimit));
    assert_eq!(validate_name("worker"), Ok(()));
}

#[test]
fn bodies_are_clamped_to_the_configured_limit() {
    assert_eq!(redact_and_clamp("short"), "short");
    let long = "y".repeat(4608);
    let clamped = redact_and_clamp(&long);
    assert_eq!(clamped.len(), 4096);
}

#[test]
fn clamp_keeps_a_custom_redaction() {
    let redacted = clamp_body("token abcdef", |body| format!("redacted:{body}").into());
    assert_eq!(redacted, "redacted:token abcdef");
}

#[test]
fn export_rejection_counter_advances_for_that_signal_reason() {
    let before = facade_health();
    record_export_rejection(Signal::Metric, Rejection::SizeLimit);
    let after = facade_health();
    assert_eq!(after.size_limit, before.size_limit + 1);
    assert_eq!(after.unknown_name, before.unknown_name);
    assert_eq!(
        after.by_signal_reason[Signal::Metric as usize][Rejection::SizeLimit as usize],
        before.by_signal_reason[Signal::Metric as usize][Rejection::SizeLimit as usize] + 1
    );
}

#[test]
fn exception_attributes_accept_plain_strings_only() {
    assert_eq!(
        crate::limits::validate_attribute_value("exception.message", &Value::Str("boom")),
        Ok(())
    );
    assert_eq!(
        crate::limits::validate_attribute_value("exception.message", &Value::Bool(true)),
        Err(Rejection::InvalidValue)
    );
}
