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
    let mise = repo_text("native/mise.toml");
    assert_subsequence(
        task_block(&mise, "ci"),
        &[
            "mbx +1.97.1 xtask desktop bindings-check",
            "xcodegen generate",
            "format-check",
            "lint",
            "mbx +1.97.1 xtask desktop test",
            "mbx +1.97.1 xtask desktop build",
            "desktop test-swift",
            "mbx +1.97.1 xtask desktop verify",
        ],
        "native ci",
    );
    assert!(
        !task_block(&mise, "ci").contains("cargo xtask desktop"),
        "native ci must dispatch Rust work through MBX"
    );
    assert_subsequence(
        task_block(&mise, "merge"),
        &["mise -C native run ci", "run-ui-tests.sh"],
        "native merge",
    );
    assert_subsequence(
        task_block(&mise, "scheduled"),
        &["mise -C native run merge", "mise -C native run deadcode"],
        "native scheduled",
    );
}

#[test]
fn mise_native_rust_option_routes_cargo_through_mbx() {
    let mise = repo_text("mise.toml");
    let rust_toolchain = repo_text("rust-toolchain.toml");
    assert!(
        mise.contains("rust = { version = \"1.97.1\", mr_boxington = true }"),
        "Mise's Rust integration must route Cargo through MBX natively"
    );
    assert!(
        mise.contains("mr-boxington = \"1.23.0\""),
        "the native integration must resolve the current pinned MBX tool"
    );
    assert!(
        rust_toolchain.contains("channel = \"1.97.1\""),
        "the Mise and rustup Rust pins must stay aligned"
    );
    assert!(
        mise.contains("idiomatic_version_file_enable_tools = [\"rust\"]"),
        "Mise must keep rust-toolchain.toml in its Rust version selection"
    );
    assert!(
        !mise.contains("[wrappers.cargo]"),
        "legacy Cargo wrapper removed"
    );
    assert!(!mise.contains("[tasks."), "root task aliases removed");

    let native_mise = repo_text("native/mise.toml");
    let desktop_ci = task_block(&native_mise, "ci");
    assert!(desktop_ci.contains("mbx +1.97.1 xtask desktop test-swift --jobs 2"));
    assert!(
        !desktop_ci.contains("cargo xtask desktop"),
        "native Rust task invocations must pass through MBX explicitly"
    );

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let output = std::process::Command::new("mise")
        .args(["exec", "-v", "--", "cargo", "--version"])
        .current_dir(&root)
        .env("MISE_AUTO_INSTALL", "false")
        .env("MISE_LOG_LEVEL", "trace")
        .output()
        .expect("Mise must be installed for the Rust MBX integration contract");
    assert!(
        output.status.success(),
        "Mise command wrapper failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("shim[cargo] WRAPPER command: mbx"),
        "Mise must dispatch Cargo through mbx; stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).starts_with("cargo "),
        "MBX must delegate Cargo's version query to the selected Rust toolchain"
    );

    let native_output = std::process::Command::new("mise")
        .args(["-C", "native", "exec", "-v", "--", "cargo", "--version"])
        .current_dir(root)
        .env("MISE_AUTO_INSTALL", "false")
        .env("MISE_LOG_LEVEL", "trace")
        .output()
        .expect("Mise must be installed for the native Rust MBX integration contract");
    assert!(
        native_output.status.success(),
        "native Mise command wrapper failed: {}",
        String::from_utf8_lossy(&native_output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&native_output.stderr).contains("shim[cargo] WRAPPER command: mbx"),
        "native Mise must dispatch Cargo through mbx; stderr was: {}",
        String::from_utf8_lossy(&native_output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&native_output.stdout).starts_with("cargo "),
        "MBX must delegate native Cargo's version query to the selected Rust toolchain"
    );
}
