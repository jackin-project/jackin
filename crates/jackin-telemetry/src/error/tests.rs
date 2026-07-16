// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn result_extension_preserves_every_result_type() {
    let ok: Result<u8, &str> = Ok(7);
    assert_eq!(
        ok.record_telemetry_error(schema::enums::ErrorType::DbError),
        Ok(7)
    );

    let error: Result<u8, WithoutFormatting> = Err(WithoutFormatting);
    let _error = error
        .record_telemetry_error(schema::enums::ErrorType::DbError)
        .unwrap_err();
}

struct WithoutFormatting;
