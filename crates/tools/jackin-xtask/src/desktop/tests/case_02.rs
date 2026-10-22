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
            "mbx +1.99.1 xtask desktop bindings-check",
            "xcodegen generate",
            "xcrun swift-format lint",
            "swiftlint lint --strict",
            "mbx +1.99.1 xtask desktop test",
            "mbx +1.99.1 xtask desktop build",
            "desktop test-swift",
            "mbx +1.99.1 xtask desktop verify",
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

#[expect(
    clippy::disallowed_methods,
    reason = "the test captures mise shim stderr and is not a render or runtime thread"
)]
fn mise_project_exec(root: &std::path::Path, args: &[&str], missing: &str) -> std::process::Output {
    let mut command = std::process::Command::new("mise");
    command
        .args(args)
        .current_dir(root)
        .env("MISE_AUTO_INSTALL", "false")
        .env("MISE_LOG_LEVEL", "trace")
        .env_remove("MISE_NO_CONFIG")
        .env_remove("MISE_NO_ENV")
        .env_remove("MISE_NO_HOOKS");
    command.output().expect(missing)
}

#[test]
fn mise_native_rust_option_routes_cargo_through_mbx() {
    let mise_text = repo_text("mise.toml");
    let mise: toml::Value =
        toml::from_str(&mise_text).expect("Mise configuration must parse as TOML");
    let rust_toolchain = repo_text("rust-toolchain.toml");
    let tools = mise
        .get("tools")
        .and_then(toml::Value::as_table)
        .expect("Mise tools table");
    let rust = tools
        .get("rust")
        .and_then(toml::Value::as_table)
        .expect("Rust tool configuration");
    assert!(
        rust.get("version").and_then(toml::Value::as_str) == Some("1.99.1")
            && rust
                .get("components")
                .and_then(toml::Value::as_array)
                .is_some_and(|components| {
                    components
                        .iter()
                        .filter_map(toml::Value::as_str)
                        .collect::<Vec<_>>()
                        == ["clippy", "rustfmt"]
                })
            && rust
                .get("targets")
                .and_then(toml::Value::as_array)
                .is_some_and(|targets| {
                    targets
                        .iter()
                        .filter_map(toml::Value::as_str)
                        .collect::<Vec<_>>()
                        == ["aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"]
                })
            && rust.get("mr_boxington").and_then(toml::Value::as_bool) == Some(true),
        "Mise's Rust integration must route Cargo through MBX natively"
    );
    let mbx = tools
        .get("mr-boxington")
        .and_then(toml::Value::as_table)
        .expect("MBX tool configuration");
    assert!(
        mbx.get("version").and_then(toml::Value::as_str) == Some("1.26.0")
            && mbx.get("workflow").and_then(toml::Value::as_str)
                == Some("release.yml@refs/heads/main"),
        "the native integration must pin the current MBX version and signer workflow"
    );
    assert!(
        rust_toolchain.contains("channel = \"1.99.1\""),
        "the Mise and rustup Rust pins must stay aligned"
    );
    assert!(
        mise_text.contains("idiomatic_version_file_enable_tools = [\"rust\"]"),
        "Mise must keep rust-toolchain.toml in its Rust version selection"
    );
    assert!(
        mise_text.contains("[tasks.renovate-upstream-sources]"),
        "the upstream asset check stays a root mise task"
    );
    let config: toml::Value = toml::from_str(&mise_text).expect("root Mise config must parse");
    let tasks = config["tasks"]
        .as_table()
        .expect("root Mise tasks must be a table");
    let mut task_ids: Vec<_> = tasks.keys().map(String::as_str).collect();
    task_ids.sort_unstable();
    assert_eq!(
        task_ids,
        [
            "check:custom-lints",
            "check:dependency-usage",
            "check:repository-links",
            "check:repository-policy",
            "release:package",
            "renovate-upstream-sources",
        ],
        "root Mise tasks must match the native checks, producer, and upstream check"
    );
    for (id, command) in [
        ("check:dependency-usage", "cargo shear --deny-warnings"),
        (
            "check:repository-links",
            "cargo run --locked -p jackin-xtask -- docs repo-links",
        ),
        (
            "check:repository-policy",
            "cargo run --locked -p jackin-xtask -- lint --strict",
        ),
    ] {
        assert_eq!(
            tasks[id]["run"].as_str(),
            Some(command),
            "native check task {id}"
        );
    }
    assert!(mise_text.contains("[tasks.renovate-upstream-sources]"));
    assert!(mise_text.contains("[tasks.\"release:package\"]"));

    let native_mise = repo_text("native/mise.toml");
    let desktop_ci = task_block(&native_mise, "ci");
    assert!(desktop_ci.contains("mbx +1.99.1 xtask desktop test-swift --jobs 2"));
    assert!(
        !desktop_ci.contains("cargo xtask desktop"),
        "native Rust task invocations must pass through MBX explicitly"
    );

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let output = mise_project_exec(
        &root,
        &["exec", "-v", "--", "cargo", "--version"],
        "Mise must be installed for the Rust MBX integration contract",
    );
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

    let native_output = mise_project_exec(
        &root,
        &["-C", "native", "exec", "-v", "--", "cargo", "--version"],
        "Mise must be installed for the native Rust MBX integration contract",
    );
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

#[test]
fn owner_mise_layout_wraps_cargo_with_mbx_and_a_minimal_profile() {
    let mise: toml::Value =
        toml::from_str(&repo_text("mise.toml")).expect("Mise configuration must parse as TOML");
    let rust_toolchain = repo_text("rust-toolchain.toml");
    let rust = mise
        .get("tools")
        .and_then(|tools| tools.get("rust"))
        .and_then(toml::Value::as_table)
        .expect("Rust tool configuration");
    let wrapper = mise
        .get("wrappers")
        .and_then(|wrappers| wrappers.get("cargo"))
        .and_then(toml::Value::as_table)
        .expect("owner standard: [wrappers.cargo] routes every cargo through MBX");
    assert!(
        wrapper.get("command").and_then(toml::Value::as_str) == Some("mbx")
            && wrapper
                .get("env")
                .and_then(|env| env.get("MBX_CARGO_SHIM_MODE"))
                .and_then(toml::Value::as_str)
                == Some("1"),
        "the owner-standard Cargo wrapper must run mbx in Cargo shim mode"
    );
    assert!(
        rust.get("profile").and_then(toml::Value::as_str) == Some("minimal")
            && rust_toolchain.contains("profile = \"minimal\""),
        "mise.toml and rust-toolchain.toml must agree on the minimal profile"
    );
}
