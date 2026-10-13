// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn result_extension_marks_ownership_without_formatting() {
    let ok: Result<u8, WithoutFormatting> = Ok(7);
    assert_eq!(
        ok.record_telemetry_error(schema::enums::ErrorType::DbError)
            .unwrap(),
        7
    );

    let error: Result<u8, WithoutFormatting> = Err(WithoutFormatting);
    let error = error
        .record_telemetry_error(schema::enums::ErrorType::DbError)
        .unwrap_err();
    assert!(error.downcast_ref::<WithoutFormatting>().is_some());
    let reowned = Err::<(), _>(error)
        .record_telemetry_error(schema::enums::ErrorType::IoError)
        .unwrap_err();
    assert_eq!(reowned.error_type(), schema::enums::ErrorType::DbError);
    record_recovered_degradation().expect("registered recovered warning");
}

#[derive(Debug)]
struct WithoutFormatting;

impl std::fmt::Display for WithoutFormatting {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        panic!("telemetry formatted a private error")
    }
}

impl std::error::Error for WithoutFormatting {}
