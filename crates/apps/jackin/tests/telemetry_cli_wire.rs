// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use assert_cmd::Command;

#[test]
fn conformance_wire_real_host_cli_parse_failure_is_owned_once_without_argv() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    let home = tempfile::tempdir()?;
    let endpoint = testbed.endpoint();

    let output = Command::cargo_bin("jackin")?
        .timeout(std::time::Duration::from_secs(20))
        .arg("--wire-private-cli-argument=wire-private-cli-value")
        .env("JACKIN_HOME_DIR", home.path())
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", &endpoint)
        .env("OTEL_EXPORTER_OTLP_PROTOCOL", "grpc")
        .env_remove("OTEL_SDK_DISABLED")
        .output()?;
    assert_eq!(output.status.code(), Some(2), "{output:?}");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let spans = runtime.block_on(async {
        loop {
            let spans = testbed
                .spans()
                .into_iter()
                .filter(|span| span.name == "cli.command")
                .collect::<Vec<_>>();
            if spans.len() == 1 {
                break spans;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "host CLI command wire span did not arrive exactly once"
            );
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    });
    let wire_text = format!("{spans:?}");
    for expected in ["help", "failure", "config_error", "2"] {
        assert!(
            wire_text.contains(expected),
            "missing {expected}: {wire_text}"
        );
    }
    let private_home = home.path().to_string_lossy().into_owned();
    let prohibited = [
        "--wire-private-cli-argument",
        "wire-private-cli-value",
        private_home.as_str(),
    ];
    for value in prohibited {
        assert!(!wire_text.contains(value), "exported {value}");
    }
    assert_eq!(
        testbed.prohibited_value_violations(&prohibited),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    assert!(
        std::fs::read_dir(home.path())?.next().is_none(),
        "host CLI telemetry created a local artifact"
    );
    Ok(())
}
