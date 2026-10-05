use super::{
    DesktopCommand, MIN_OS, XunitTotals, assert_broker_version, assert_executable_file,
    assert_native_broker_archs, broker_path, minos_matches_target, normalize_generated_text,
    parse_dwarf_uuid, parse_swift_jobs, parse_xunit_totals, read_xunit_totals, swift_build_args,
    swift_test_args, tree_differences, validate_build, validate_test_totals, validate_version,
};

#[test]
fn broker_is_sibling_of_the_desktop_executable() {
    assert_eq!(
        broker_path(std::path::Path::new("JackinDesktop.app")),
        std::path::Path::new("JackinDesktop.app/Contents/MacOS/jackin-usage-broker")
    );
}

#[test]
fn broker_requires_exact_native_architecture() {
    assert_native_broker_archs("arm64\n").unwrap();
    for wrong in ["", "x86_64", "arm64 x86_64", "arm64 arm64", "arm64e"] {
        assert!(assert_native_broker_archs(wrong).is_err(), "{wrong}");
    }
}

#[test]
fn broker_version_probe_requires_matching_binary_and_release() {
    assert_broker_version("jackin-usage-broker 0.6.0\n", "0.6.0").unwrap();
    for wrong in [
        "jackin-usage-broker 0.5.0",
        "jackin 0.6.0",
        "jackin-usage-broker 0.6.0-dev",
        "jackin-usage-broker 0.6.0\nstarting daemon",
        "",
    ] {
        assert!(assert_broker_version(wrong, "0.6.0").is_err(), "{wrong}");
    }
}

#[cfg(unix)]
#[test]
fn broker_requires_regular_executable_file() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = super::tempfile_dir("jackin-desktop-broker-file-test").unwrap();
    let broker = temp.join("broker");
    assert!(assert_executable_file(&broker).is_err());
    assert!(assert_executable_file(&temp).is_err());
    std::fs::write(&broker, b"broker").unwrap();
    std::fs::set_permissions(&broker, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(assert_executable_file(&broker).is_err());
    std::fs::set_permissions(&broker, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_executable_file(&broker).unwrap();
    let link = temp.join("link");
    symlink(&broker, &link).unwrap();
    assert!(assert_executable_file(&link).is_err());
    std::fs::remove_dir_all(&temp).unwrap();
}

#[test]
fn desktop_build_packages_native_broker_before_bundle_signing() {
    let source = include_str!("../desktop.rs");
    let build = source.split("fn build_app(").nth(1).unwrap();
    let build = build.split("pub(super) fn verify_app(").next().unwrap();
    assert_subsequence(
        build,
        &[
            ".env(\"JACKIN_VERSION_OVERRIDE\", version)",
            "DESKTOP_PROFILE,",
            "HOST_TARGET,",
            "\"jackin\",",
            "\"--bin\",",
            "BROKER_EXECUTABLE,",
            ".arg(\"--target-dir\")",
            ".arg(root.join(\"target\"))",
            "verify_broker(&built_broker, version)?;",
            "fs::copy(&built_broker, &broker)",
            "verify_broker(&broker, version)?;",
            "sign_broker(&dist, \"-\", false)?;",
            "dist.to_str().context(\"dist utf-8\")?",
        ],
        "native broker assembly",
    );
}

#[test]
fn developer_id_signs_and_checks_broker_before_notarization() {
    assert_subsequence(
        include_str!("sign_notarize.rs"),
        &[
            "verify_app(&app, None, &version, &build, false)?;",
            "sign_broker(&app, &identity, true)?;",
            "app.to_str().context(\"app utf-8\")?",
            "check_expected_cert(&broker)?;",
            "check_expected_team(&broker)?;",
            "reject_get_task_allow(&broker)?;",
            "run_notarytool(&submit_zip, &notary_json)?;",
        ],
        "nested broker signing",
    );
}

#[test]
fn xcframework_pack_generates_bindings_and_headers_for_clean_builds() {
    // The pack command must use Boltffi's default generation path: the clean
    // build needs generated headers before XCFramework assembly.
    assert!(include_str!("../desktop.rs").contains(".args([\"pack\", \"apple\"]);"));
}

#[test]
fn version_accepts_dotted_numeric() {
    validate_version("0.6.0").unwrap();
    validate_version("1").unwrap();
    validate_version("10.20.30").unwrap();
}

#[test]
fn version_rejects_semver_prerelease_and_empty() {
    assert!(validate_version("").is_err());
    assert!(validate_version("0.6.0-dev").is_err());
    assert!(validate_version("v0.6.0").is_err());
    assert!(validate_version("0..1").is_err());
}

#[test]
fn build_accepts_numeric_only() {
    validate_build("1").unwrap();
    validate_build("42").unwrap();
    assert!(validate_build("").is_err());
    assert!(validate_build("1a").is_err());
}

#[test]
fn minos_must_match_current_baseline() {
    assert!(minos_matches_target("26.0", MIN_OS));
    assert!(minos_matches_target("26.0.0", MIN_OS));
    assert!(!minos_matches_target("25.0", MIN_OS));
    assert!(!minos_matches_target("26.1", MIN_OS));
    assert!(!minos_matches_target("27.0", MIN_OS));
}

#[test]
fn generated_bindings_have_stable_whitespace() {
    assert_eq!(
        normalize_generated_text("one  \n  two\t\n\n"),
        "one\n  two\n"
    );
    assert_eq!(normalize_generated_text("one"), "one\n");
    assert_eq!(normalize_generated_text(" \t\n"), "");
}

fn write_tree(root: &std::path::Path, files: &[(&str, &[u8])]) {
    for (relative, bytes) in files {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}

#[test]
fn tree_differences_clean_when_identical() {
    let temp = std::env::temp_dir().join(format!("jackin-bindings-clean-{}", std::process::id()));
    let expected = temp.join("expected");
    let actual = temp.join("actual");
    drop(std::fs::remove_dir_all(&temp));
    write_tree(&expected, &[("a.bin", b"one"), ("nested/b.bin", b"two")]);
    write_tree(&actual, &[("a.bin", b"one"), ("nested/b.bin", b"two")]);
    assert!(
        tree_differences(&expected, &actual, "label")
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(&temp).unwrap();
}

#[test]
fn tree_differences_flags_stale_missing_and_extra() {
    let temp = std::env::temp_dir().join(format!("jackin-bindings-drift-{}", std::process::id()));
    let expected = temp.join("expected");
    let actual = temp.join("actual");
    drop(std::fs::remove_dir_all(&temp));
    write_tree(
        &expected,
        &[("stale.bin", b"old"), ("missing.bin", b"gone")],
    );
    write_tree(&actual, &[("stale.bin", b"new"), ("extra.bin", b"added")]);
    let differences = tree_differences(&expected, &actual, "label").unwrap();
    assert_eq!(
        differences,
        vec![
            "label/missing.bin: missing after regeneration".to_owned(),
            "label/extra.bin: not committed".to_owned(),
            "label/stale.bin: content drift".to_owned(),
        ]
    );
    std::fs::remove_dir_all(&temp).unwrap();
}

#[test]
fn xunit_totals_sum_every_testsuite() {
    let source = concat!(
        "<?xml version=\"1.0\"?>\n",
        "<testsuites>\n",
        "<testsuite name=\"a\" tests=\"3\" failures=\"0\" errors=\"0\"></testsuite>\n",
        "<testsuite name=\"b\" tests=\"4\" failures=\"1\" errors=\"2\"></testsuite>\n",
        "</testsuites>\n"
    );
    assert_eq!(
        parse_xunit_totals(source).unwrap(),
        XunitTotals {
            tests: 7,
            failures: 1,
            errors: 2,
        }
    );
}

#[test]
fn parallel_xctest_xunit_counts_every_worker_suite() {
    let source = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<testsuites tests=\"5\" failures=\"0\" errors=\"0\">\n",
        "  <testsuite name=\"worker-1\" tests=\"2\" failures=\"0\" errors=\"0\"/>\n",
        "  <testsuite name=\"worker-2\" tests=\"3\" failures=\"0\" errors=\"0\"/>\n",
        "</testsuites>\n"
    );
    let totals = parse_xunit_totals(source).unwrap();
    assert_eq!(
        totals,
        XunitTotals {
            tests: 5,
            failures: 0,
            errors: 0,
        }
    );
    validate_test_totals("XCTest", &totals).unwrap();
}

#[test]
fn xunit_totals_reject_corrupt_or_incomplete_reports() {
    parse_xunit_totals("").unwrap_err();
    parse_xunit_totals("<testsuites></testsuites>").unwrap_err();
    parse_xunit_totals(
        "<testsuites><testsuite name=\"a\" tests=\"1\" failures=\"0\" errors=\"0\">",
    )
    .unwrap_err();
    parse_xunit_totals("<testsuite name=\"a\" tests=\"1\" failures=\"0\" errors=\"0\"")
        .unwrap_err();
    parse_xunit_totals("<testsuites><testsuite name=\"a\" tests=\"1\" errors=\"0\"/></testsuites>")
        .unwrap_err();
    parse_xunit_totals(
        "<testsuites><testsuite name=\"a\" tests=\"many\" failures=\"0\" errors=\"0\"/></testsuites>",
    )
        .unwrap_err();
    parse_xunit_totals(
        "<testsuites><testsuite name=\"a\" tests=\"1\" failures=\"0\" errors=\"0\"></testsuites>",
    )
    .unwrap_err();
    parse_xunit_totals(
        "<testsuites><testsuite name=\"worker\" tests=\"7\" failures=\"0\" errors=\"0\"/>",
    )
    .unwrap_err();
    parse_xunit_totals(concat!(
        "<testsuites><testsuite name=\"a\" tests=\"1\" failures=\"0\" errors=\"0\"/>",
        "</testsuites><testsuites><testsuite name=\"b\" tests=\"1\" failures=\"0\" errors=\"0\"/>",
        "</testsuites>"
    ))
    .unwrap_err();
    parse_xunit_totals("<testsuite name=\"worker\" tests=\"7\" failures=\"0\" errors=\"0\"/>")
        .unwrap_err();
}

#[test]
fn xunit_counts_failures_and_errors_and_rejects_zero_tests() {
    let failures = parse_xunit_totals(
        "<testsuites><testsuite name=\"xctest\" tests=\"3\" failures=\"1\" errors=\"0\"/></testsuites>",
    )
    .unwrap();
    assert!(validate_test_totals("XCTest", &failures).is_err());

    let errors = parse_xunit_totals(
        "<testsuites><testsuite name=\"swift-testing\" tests=\"2\" failures=\"0\" errors=\"1\"/></testsuites>",
    )
    .unwrap();
    assert!(validate_test_totals("Swift Testing", &errors).is_err());

    let empty = parse_xunit_totals(
        "<testsuites><testsuite name=\"empty\" tests=\"0\" failures=\"0\" errors=\"0\"/></testsuites>",
    )
    .unwrap();
    assert!(validate_test_totals("Swift Testing", &empty).is_err());
}

#[test]
fn report_reader_fails_closed_on_missing_or_invalid_framework_reports() {
    let temp = tempfile::tempdir().unwrap();
    let xctest = temp.path().join("swift-unit-tests.xml");
    assert!(
        read_xunit_totals(&xctest, "XCTest")
            .unwrap_err()
            .to_string()
            .contains("missing XCTest xUnit report")
    );

    std::fs::write(&xctest, "<testsuites><testsuite").unwrap();
    let error = read_xunit_totals(&xctest, "XCTest").unwrap_err();
    assert!(format!("{error:#}").contains("invalid XCTest xUnit report"));
}

#[test]
fn swift_testing_xunit_is_a_separate_required_counted_report() {
    let report = parse_xunit_totals(
        "<testsuites><testsuite name=\"ProjectBaselineTests\" tests=\"2\" failures=\"0\" errors=\"0\"/></testsuites>",
    )
    .unwrap();
    assert_eq!(report.tests, 2);
    validate_test_totals("Swift Testing", &report).unwrap();
}

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
    assert!(swift_build_args(0).is_err());
    assert!(swift_test_args(9, std::path::Path::new("tests.xml")).is_err());
}

fn repo_text(relative: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
}

fn task_block<'a>(mise: &'a str, name: &str) -> &'a str {
    let marker = format!("[tasks.{name}]\n");
    let start = mise
        .find(&marker)
        .unwrap_or_else(|| panic!("mise.toml missing {marker}"));
    let rest = &mise[start + marker.len()..];
    let end = rest.find("\n[").map_or(rest.len(), |index| index + 1);
    &rest[..end]
}

fn assert_subsequence(haystack: &str, needles: &[&str], label: &str) {
    let mut cursor = 0;
    for needle in needles {
        let found = haystack[cursor..]
            .find(needle)
            .unwrap_or_else(|| panic!("{label}: `{needle}` missing or out of order"));
        cursor += found + needle.len();
    }
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
    let task = task_block(&repo_text("mise.toml"), "swift-package-native-ci");
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

#[test]
fn release_workflow_invokes_canonical_mise_tasks() {
    let release = repo_text(".github/workflows/release.yml");
    let mise = repo_text("mise.toml");
    let release_tools = task_block(&mise, "desktop-release-tools");
    assert!(
        release_tools.contains("mise install --locked rust cargo:boltffi_cli xcodegen"),
        "release tool task must explicitly install its locked closure"
    );
    assert_subsequence(
        &release,
        &[
            "mise run desktop-release-tools",
            "mise run desktop-release-env",
        ],
        "release tool setup",
    );
    for task in [
        "mise run desktop-build",
        "mise run desktop-verify",
        "mise run desktop-sign-notarize",
        "mise run desktop-release-state",
    ] {
        assert!(release.contains(task), "release.yml must invoke `{task}`");
    }
    for restated in [
        "cargo xtask desktop build",
        "cargo xtask desktop verify",
        "cargo xtask desktop sign-notarize",
        "cargo xtask desktop release-state",
    ] {
        assert!(
            !release.contains(restated),
            "release.yml must not restate `{restated}` beside the mise task"
        );
    }
}

#[test]
fn generated_ci_includes_configured_native_verification_tasks() -> anyhow::Result<()> {
    use anyhow::Context as _;
    use std::{collections::BTreeSet, fs, path::Path};

    const TASK_IDS: [&str; 2] = ["native-swift-format", "native-swiftlint"];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config_path = root.join(".velnor/config.toml");
    let config_text = fs::read_to_string(&config_path)
        .with_context(|| format!("reading {}", config_path.display()))?;
    let config: toml::Value = toml::from_str(&config_text)
        .with_context(|| format!("parsing {}", config_path.display()))?;
    let configured_tasks = config
        .get("workflow")
        .and_then(|value| value.get("tasks"))
        .and_then(toml::Value::as_array)
        .context("maintained verification tasks are declared")?;
    for task_id in TASK_IDS {
        anyhow::ensure!(
            configured_tasks.iter().any(|task| {
                task.get("kind").and_then(toml::Value::as_str) == Some("verification")
                    && task.get("id").and_then(toml::Value::as_str) == Some(task_id)
            }),
            "Velnor config must declare native task {task_id}"
        );
    }

    let workflow_dir = root.join(".github/workflows");
    let entries = fs::read_dir(&workflow_dir)
        .with_context(|| format!("reading {}", workflow_dir.display()))?;
    let mut has_required_fan_in = false;
    let mut job_ids = BTreeSet::new();
    let mut required_needs = BTreeSet::new();
    for entry in entries {
        let path = entry.context("reading workflow entry")?.path();
        if !path
            .extension()
            .is_some_and(|extension| extension == "yml" || extension == "yaml")
        {
            continue;
        }
        let contents =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let workflow: serde_json::Value = serde_yaml_ng::from_str(&contents)
            .with_context(|| format!("parsing {}", path.display()))?;
        let jobs = workflow
            .get("jobs")
            .and_then(serde_json::Value::as_object)
            .with_context(|| format!("{} has no jobs mapping", path.display()))?;
        for task_id in TASK_IDS {
            let job_id = format!("task-{task_id}");
            if jobs.contains_key(&job_id) {
                job_ids.insert(job_id);
            }
        }
        if let Some(required) = jobs.get("required") {
            has_required_fan_in = true;
            let needs = required
                .get("needs")
                .and_then(serde_json::Value::as_array)
                .with_context(|| format!("{} Required.needs must be a sequence", path.display()))?;
            required_needs.extend(
                needs
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned),
            );
        }
    }
    anyhow::ensure!(
        has_required_fan_in,
        "generated CI has no Required fan-in job"
    );
    for task_id in TASK_IDS {
        let job_id = format!("task-{task_id}");
        anyhow::ensure!(job_ids.contains(&job_id), "generated CI omits {job_id}");
        anyhow::ensure!(
            required_needs.contains(&job_id),
            "generated Required.needs omits {job_id}"
        );
    }
    Ok(())
}
