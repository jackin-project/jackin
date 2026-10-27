// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{DesktopCommand, parse_dwarf_uuid, swift_build_args, swift_test_args};
use clap::Parser;
use std::path::Path;

#[test]
fn swift_job_limit_cli_defaults_and_enforces_one_through_eight() {
    let parse = |jobs: Option<&str>| {
        let mut arguments = vec!["jackin-xtask", "desktop", "test-swift"];
        if let Some(jobs) = jobs {
            arguments.extend(["--jobs", jobs]);
        }
        crate::Cli::try_parse_from(arguments)
    };

    for (jobs, expected) in [("1", 1), ("8", 8)] {
        let parsed = parse(Some(jobs)).expect("valid job count should parse");
        match parsed.command {
            crate::Command::Desktop(DesktopCommand::TestSwift(args)) => {
                assert_eq!(args.jobs, expected);
            }
            _ => panic!("expected desktop test-swift command"),
        }
    }

    for invalid in ["0", "9", "nope", "3\n"] {
        assert!(
            parse(Some(invalid)).is_err(),
            "accepted invalid jobs value {invalid:?}"
        );
    }

    let parsed = parse(None).expect("the default job count should parse");
    match parsed.command {
        crate::Command::Desktop(DesktopCommand::TestSwift(args)) => {
            assert_eq!(args.jobs, 2);
        }
        _ => panic!("expected desktop test-swift command"),
    }
}

#[test]
fn swiftpm_build_and_test_arguments_share_a_bounded_worker_limit() {
    for jobs in [0, 9] {
        let _build_error = swift_build_args(jobs).unwrap_err();
        let _test_error =
            swift_test_args(jobs, Path::new("/tmp/native package/tests.xml")).unwrap_err();
    }

    assert_eq!(
        swift_build_args(3).expect("valid build job count should produce arguments"),
        vec!["build", "-c", "release", "--jobs", "3"]
    );

    let report = "/tmp/native package/tests.xml";
    let test_arguments = swift_test_args(3, Path::new(report))
        .expect("valid test job count should produce arguments");
    assert_eq!(test_arguments.len(), 12);
    assert_eq!(test_arguments[11], report);
    assert_eq!(
        test_arguments,
        vec![
            "test",
            "-c",
            "release",
            "--jobs",
            "3",
            "--parallel",
            "--num-workers",
            "3",
            "--experimental-maximum-parallelization-width",
            "3",
            "--xunit-output",
            report,
        ]
    );
}

#[test]
fn dwarf_uuid_reads_the_arm64_slice() {
    assert_eq!(parse_dwarf_uuid("dwarfdump returned no uuid here"), None);
    assert_eq!(parse_dwarf_uuid(""), None);

    assert_eq!(
        parse_dwarf_uuid(
            "UUID: 11111111-2222-3333-4444-555555555555 (x86_64) /tmp/app\nUUID: AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE (arm64) /tmp/app\n"
        ),
        Some("AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE".to_owned())
    );
    assert_eq!(parse_dwarf_uuid("UUID: 1111 (x86_64) /tmp/app\n"), None);
}
