// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn check(status: CheckStatus) -> CheckResult {
    CheckResult {
        name: "fixture",
        status,
        message: "fixture diagnostic".to_owned(),
        hint: Some("fixture hint".to_owned()),
    }
}

#[test]
fn failed_checks_return_error_in_every_output_format() {
    let results = [check(CheckStatus::Warn), check(CheckStatus::Fail)];

    for format in [OutputFormat::Human, OutputFormat::Json] {
        let mut output = Vec::new();
        let error = report_results(format, &results, &mut output).unwrap_err();

        assert_eq!(error.to_string(), "doctor checks failed", "{format:?}");
        assert!(!output.is_empty(), "{format:?} must render before failing");
    }
}

#[test]
fn informational_checks_succeed_in_every_output_format() {
    for statuses in [
        vec![],
        vec![CheckStatus::Ok],
        vec![CheckStatus::Warn],
        vec![CheckStatus::Skip],
        vec![CheckStatus::Ok, CheckStatus::Warn, CheckStatus::Skip],
    ] {
        let results: Vec<_> = statuses.into_iter().map(check).collect();
        for format in [OutputFormat::Human, OutputFormat::Json] {
            let mut output = Vec::new();
            report_results(format, &results, &mut output)
                .unwrap_or_else(|error| panic!("{format:?}: {results:?}: {error}"));
            assert!(!output.is_empty());
        }
    }
}

#[test]
fn failed_json_report_preserves_complete_machine_readable_diagnostics() {
    let results = [check(CheckStatus::Fail), check(CheckStatus::Warn)];
    let mut output = Vec::new();

    assert!(report_results(OutputFormat::Json, &results, &mut output).is_err());

    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report,
        serde_json::json!({
            "schema_version": "v1",
            "data": [
                {
                    "name": "fixture",
                    "status": "fail",
                    "message": "fixture diagnostic",
                    "hint": "fixture hint",
                },
                {
                    "name": "fixture",
                    "status": "warn",
                    "message": "fixture diagnostic",
                    "hint": "fixture hint",
                },
            ],
        })
    );
}

#[test]
fn report_propagates_output_errors_in_every_format_and_check_outcome() {
    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "fixture output failure",
            ))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    for status in [CheckStatus::Ok, CheckStatus::Warn, CheckStatus::Fail] {
        for format in [OutputFormat::Human, OutputFormat::Json] {
            let error = report_results(format, &[check(status)], &mut FailingWriter).unwrap_err();
            let io_error = error
                .downcast_ref::<std::io::Error>()
                .unwrap_or_else(|| panic!("{format:?}, {status:?}: {error}"));
            assert_eq!(io_error.kind(), std::io::ErrorKind::BrokenPipe);
        }
    }
}
