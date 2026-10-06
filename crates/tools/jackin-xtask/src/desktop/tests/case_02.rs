// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn swift_job_limit_cli_defaults_and_enforces_one_through_eight() {
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(subcommand)]
        command: DesktopCommand,
    }

    let defaults = Cli::try_parse_from(["xtask", "test-swift"]).unwrap();
    match defaults.command {
        DesktopCommand::TestSwift(args) => assert_eq!(args.jobs, 2),
        _ => panic!("expected test-swift command"),
    }

    let maximum = Cli::try_parse_from(["xtask", "test-swift", "--jobs", "8"]).unwrap();
    match maximum.command {
        DesktopCommand::TestSwift(args) => assert_eq!(args.jobs, 8),
        _ => panic!("expected test-swift command"),
    }

    for invalid in ["0", "9", "nope", "2\n--parallel"] {
        assert!(
            Cli::try_parse_from(["xtask", "test-swift", "--jobs", invalid]).is_err(),
            "accepted invalid jobs value {invalid:?}"
        );
    }
    assert_eq!(parse_swift_jobs("1").unwrap(), 1);
    assert_eq!(parse_swift_jobs("8").unwrap(), 8);
}

#[test]
fn swiftpm_build_and_test_arguments_share_a_bounded_worker_limit() {
    assert_eq!(
        swift_build_args(3).unwrap(),
        ["build", "-c", "release", "--jobs", "3"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        swift_test_args(3, std::path::Path::new("/tmp/native package/tests.xml")).unwrap(),
        [
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
            "/tmp/native package/tests.xml",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>()
    );
    swift_build_args(0).unwrap_err();
    swift_test_args(9, std::path::Path::new("tests.xml")).unwrap_err();
}

#[test]
fn dwarf_uuid_reads_the_arm64_slice() {
    let output = "UUID: 11111111-2222-3333-4444-555555555555 (x86_64) /tmp/app\n\
                  UUID: AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE (arm64) /tmp/app\n";
    assert_eq!(
        parse_dwarf_uuid(output).as_deref(),
        Some("AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE")
    );
    assert!(parse_dwarf_uuid("no uuid here").is_none());
    assert!(parse_dwarf_uuid("UUID: 1111 (x86_64) /tmp/app\n").is_none());
}

#[test]
fn cadence_tasks_define_the_canonical_graph() {
    let mise = repo_text("mise.toml");
    assert_subsequence(
        task_block(&mise, "desktop-ci"),
        &[
            "desktop-bindings-check",
            "desktop-generate",
            "desktop-format-check",
            "desktop-lint",
            "desktop-test\n",
            "desktop-build",
            "desktop test-swift",
            "desktop-verify",
        ],
        "desktop-ci",
    );
    assert_subsequence(
        task_block(&mise, "desktop-merge"),
        &["desktop-ci", "desktop-test-ui"],
        "desktop-merge",
    );
    assert_subsequence(
        task_block(&mise, "desktop-scheduled"),
        &["desktop-merge", "desktop-deadcode"],
        "desktop-scheduled",
    );
}

#[test]
fn cargo_wrapper_routes_native_commands_through_mbx() {
    let mise = repo_text("mise.toml");
    assert!(
        mise.contains("[wrappers.cargo]\ncommand = \"mbx\"\nenv = { MBX_CARGO_SHIM_MODE = \"1\" }"),
        "all Cargo calls must use MBX's transparent Mise shim"
    );
    assert!(
        mise.contains("mr-boxington = \"1.22.0\""),
        "the transparent wrapper must resolve the locked MBX tool"
    );
    assert!(
        mise.contains("idiomatic_version_file_enable_tools = [\"rust\"]"),
        "rust-toolchain.toml remains the single Rust version source"
    );

    let desktop_ci = task_block(&mise, "desktop-ci");
    assert!(desktop_ci.contains("cargo xtask desktop test-swift --jobs 2"));
    assert!(
        !desktop_ci.contains("mbx build"),
        "do not nest explicit MBX builds inside the transparent Cargo wrapper"
    );
}

#[test]
fn standalone_native_package_ci_uses_counted_bounded_swift_driver() {
    let repo = repo_text("mise.toml");
    let task = task_block(&repo, "swift-package-native-ci");
    assert_subsequence(
        task,
        &[
            "mise run desktop-xcframework",
            "cargo xtask desktop test-swift --jobs 2",
        ],
        "swift-package-native-ci",
    );
    assert!(
        !task.contains("swift build") && !task.contains("swift test"),
        "native CI must use the counted xtask driver for all SwiftPM build/test work"
    );
}
