#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::manual_assert,
    clippy::duration_suboptimal_units,
    clippy::filter_map_next,
    clippy::map_unwrap_or,
    clippy::redundant_closure,
    unreachable_pub,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]

//! Sentinel-report assertion helpers + `REPORT_BEGIN` / `REPORT_END`
//! markers + `docker` `cleanup_role` + the generic `run` shell helper used
//! by every fixture seeder.

use std::path::Path;
use std::process::Command;

use super::diagnostics::{diagnostics_snapshot, latest_docker_build_log};

pub(super) const REPORT_BEGIN: &str = "===JACKIN_E2E_REPORT_BEGIN===";
pub(super) const REPORT_END: &str = "===JACKIN_E2E_REPORT_END===";

pub(super) fn assert_sentinel_report(report: &str, stdout: &str, stderr: &str) {
    assert!(
        report.contains("JACKIN_SENTINEL_REPORT_BEGIN"),
        "sentinel report missing begin marker\nreport:\n{report}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        report.contains("JACKIN_SENTINEL_REPORT_END"),
        "sentinel report missing end marker\nreport:\n{report}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(report.contains("JACKIN=1"), "{report}");
    assert!(report.contains("JACKIN_AGENT=codex"), "{report}");
    assert!(report.contains("STATIC_DEFAULT=static-value"), "{report}");
    assert!(
        report.contains("LITERAL_TEMPLATE=preserve-${other.VALUE}"),
        "{report}"
    );
    assert!(report.contains("FREE_TEXT=typed-default"), "{report}");
    assert!(
        report.contains("FREE_TEXT_REQUIRED=required-value"),
        "{report}"
    );
    assert!(report.contains("SELECT_PROJECT=frontend"), "{report}");
    assert!(report.contains("SELECT_MODE=diagnostic"), "{report}");
    assert!(report.contains("BRANCH=feature/frontend"), "{report}");
    assert!(
        report.contains("COMBINED_LABEL=frontend-typed-default"),
        "{report}"
    );
    assert!(report.contains("OPTIONAL_API_KEY=unset"), "{report}");
    assert!(report.contains("OPTIONAL_DERIVED=unset"), "{report}");
    assert!(report.contains("JACKIN_SENTINEL_SOURCE_HOOK=1"), "{report}");
    assert!(
        report.contains("JACKIN_SENTINEL_PREFLIGHT_COUNT=1"),
        "{report}"
    );
}

pub(super) fn assert_sentinel_build_output_routed_to_log(home: &Path, stdout: &str, stderr: &str) {
    let raw_build_marker = "[internal] load build definition";
    assert!(
        !stdout.contains(raw_build_marker)
            && !stderr.contains(raw_build_marker)
            && !stdout.contains("DerivedDockerfile")
            && !stderr.contains("DerivedDockerfile"),
        "Docker build output leaked onto the rich screen\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Choose launch agent")
            && stdout.contains("Sentinel free text:")
            && stdout.contains("↵")
            && stdout.contains("save"),
        "PTY transcript should prove the rich launch dialogs rendered\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let build_log = latest_docker_build_log(home).unwrap_or_else(|| {
        panic!(
            "expected docker build log artifact under diagnostics\n{}",
            diagnostics_snapshot(home)
        )
    });
    let build_log_contents = std::fs::read_to_string(&build_log).unwrap_or_else(|error| {
        panic!(
            "failed to read docker build log {}: {error}",
            build_log.display()
        )
    });
    assert!(
        build_log_contents.contains("command: docker ")
            && build_log_contents.contains("buildx build")
            && build_log_contents.contains(raw_build_marker)
            && build_log_contents.contains("DerivedDockerfile"),
        "Docker build output should be captured in the build log artifact {}\n{}",
        build_log.display(),
        build_log_contents
    );
}

pub(super) fn find_report_value<'a>(report: &'a str, key: &str) -> Option<&'a str> {
    report.lines().find_map(|line| {
        let (_, value) = line.split_once(key)?;
        value
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-'))
            .next()
            .filter(|value| !value.is_empty())
    })
}

pub(super) fn cleanup_role(role_key: &str, image: &str) {
    let output = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("label=jackin.class={role_key}"),
            "--format",
            "{{.Names}}",
        ])
        .output();
    if let Ok(output) = output {
        for name in String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
        {
            drop(Command::new("docker").args(["rm", "-f", name]).output());
            let _unused = Command::new("docker")
                .args(["rm", "-f", &format!("{name}-dind")])
                .output();
            let _unused = Command::new("docker")
                .args(["network", "rm", &format!("{name}-net")])
                .output();
            let _unused = Command::new("docker")
                .args(["volume", "rm", &format!("{name}-dind-certs")])
                .output();
        }
    }
    drop(Command::new("docker").args(["rmi", image]).output());
}

pub(super) fn run(program: &str, args: &[&str], cwd: Option<&Path>) {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command
        .output()
        .unwrap_or_else(|e| panic!("{program} {} failed to spawn: {e}", args.join(" ")));
    assert!(
        output.status.success(),
        "{program} {} failed\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
