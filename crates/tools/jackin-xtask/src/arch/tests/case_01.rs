// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn clean_graph_produces_no_problems() {
    let t = tiers(&[("a", 0), ("b", 1), ("c", 2)]);
    let prod = edges(&[("c", &["b", "a"]), ("b", &["a"]), ("a", &[])]);
    let dev = BTreeMap::new();
    let m = members(&["a", "b", "c"]);
    let problems = evaluate(&t, &prod, &dev, &m, &[]);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn missing_tier_crate_fails_with_fix() {
    let t = tiers(&[("a", 0)]);
    let prod = edges(&[("a", &[]), ("orphan", &["a"])]);
    let dev = BTreeMap::new();
    let m = members(&["a", "orphan"]);
    let problems = evaluate(&t, &prod, &dev, &m, &[]);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("orphan") && p.contains("add it to TIERS")),
        "{problems:?}"
    );
}

#[test]
fn upward_production_edge_fails_with_strictly_lower_tier() {
    let t = tiers(&[("low", 0), ("high", 2)]);
    let prod = edges(&[("low", &["high"]), ("high", &[])]);
    let dev = BTreeMap::new();
    let m = members(&["low", "high"]);
    let problems = evaluate(&t, &prod, &dev, &m, &[]);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("strictly lower tier") && p.contains("low") && p.contains("high")),
        "{problems:?}"
    );
}

#[test]
fn same_tier_production_edge_fails() {
    let t = tiers(&[("a", 1), ("b", 1)]);
    let prod = edges(&[("a", &["b"]), ("b", &[])]);
    let dev = BTreeMap::new();
    let m = members(&["a", "b"]);
    let problems = evaluate(&t, &prod, &dev, &m, &[]);
    assert!(
        problems.iter().any(|p| p.contains("strictly lower tier")),
        "{problems:?}"
    );
}

#[test]
fn production_cycle_is_reported() {
    let t = tiers(&[("a", 0), ("b", 1)]);
    let prod = edges(&[("a", &["b"]), ("b", &["a"])]);
    let dev = BTreeMap::new();
    let m = members(&["a", "b"]);
    let problems = evaluate(&t, &prod, &dev, &m, &[]);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("production dependency cycle")),
        "{problems:?}"
    );
}

#[test]
fn dev_cycle_not_in_allowlist_fails() {
    let t = tiers(&[("a", 0), ("b", 1)]);
    let prod = edges(&[("b", &["a"]), ("a", &[])]);
    let dev = edges(&[("a", &["b"])]);
    let m = members(&["a", "b"]);
    let problems = evaluate(&t, &prod, &dev, &m, &[]);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("--dev-->") && p.contains("not in DEV_CYCLE_ALLOWLIST")),
        "{problems:?}"
    );
}

#[test]
fn allowlisted_dev_cycle_is_quiet() {
    let t = tiers(&[("a", 0), ("b", 1)]);
    let prod = edges(&[("b", &["a"]), ("a", &[])]);
    let dev = edges(&[("a", &["b"])]);
    let m = members(&["a", "b"]);
    let allow = &[("a", "b")];
    let problems = evaluate(&t, &prod, &dev, &m, allow);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn stale_allowlist_row_fails() {
    let t = tiers(&[("a", 0), ("b", 1)]);
    let prod = edges(&[("b", &["a"]), ("a", &[])]);
    let dev = BTreeMap::new();
    let m = members(&["a", "b"]);
    let allow = &[("a", "b")];
    let problems = evaluate(&t, &prod, &dev, &m, allow);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("remove the stale") || p.contains("stale allowlist")),
        "{problems:?}"
    );
}

#[expect(
    clippy::too_many_lines,
    reason = "the tier pin lists every workspace member so an unreviewed crate fails this test"
)]
#[test]
fn real_tiers_table_covers_every_expected_member() {
    let expected: BTreeSet<&str> = [
        "jackin",
        "jackin-agent-status",
        "jackin-brand",
        "jackin-build-meta",
        "jackin-capsule",
        "jackin-config",
        "jackin-console",
        "jackin-core",
        "jackin-dev",
        "jackin-diagnostics",
        "jackin-docker",
        "jackin-env",
        "jackin-host",
        "jackin-image",
        "jackin-instance",
        "jackin-isolation",
        "jackin-launch",
        "jackin-manifest",
        "jackin-omp-store",
        "jackin-oppicker",
        "jackin-otlp-testbed",
        "jackin-pr-trailers",
        "jackin-process",
        "jackin-process-directory",
        "jackin-protocol",
        "jackin-runtime",
        "jackin-runtime-apple-container-activate-entry",
        "jackin-runtime-apple-container-attach",
        "jackin-runtime-apple-container-attach-outcome",
        "jackin-runtime-apple-container-check-dns",
        "jackin-runtime-apple-container-client",
        "jackin-runtime-apple-container-probe-version",
        "jackin-runtime-apple-container-running",
        "jackin-runtime-apple-container-session-contract",
        "jackin-runtime-apple-container-stop",
        "jackin-runtime-apple-container-supervisor-env",
        "jackin-runtime-apple-container-wait",
        "jackin-runtime-attach-admission",
        "jackin-runtime-attach-capsule-ready",
        "jackin-runtime-attach-exec-args",
        "jackin-runtime-attach-inspect",
        "jackin-runtime-attach-running-gate",
        "jackin-runtime-attach-sessions",
        "jackin-runtime-attach-transport",
        "jackin-runtime-backend-selection",
        "jackin-runtime-cleanup-absent-guards",
        "jackin-runtime-cleanup-dind-gc",
        "jackin-runtime-cleanup-eject-resources",
        "jackin-runtime-cleanup-prune-cache",
        "jackin-runtime-cleanup-prune-dir",
        "jackin-runtime-cleanup-prune-home",
        "jackin-runtime-cleanup-prune-images",
        "jackin-runtime-cleanup-prune-roles",
        "jackin-runtime-cleanup-resolve",
        "jackin-runtime-cleanup-socket-dir",
        "jackin-runtime-cleanup-timing",
        "jackin-runtime-coordination",
        "jackin-runtime-discovery",
        "jackin-runtime-docker-profile",
        "jackin-runtime-drift",
        "jackin-runtime-exec-host",
        "jackin-runtime-exit-summary",
        "jackin-runtime-host-attach",
        "jackin-runtime-host-daemon",
        "jackin-runtime-identity",
        "jackin-runtime-image",
        "jackin-runtime-isolation",
        "jackin-runtime-launch-account-config",
        "jackin-runtime-launch-account-identity",
        "jackin-runtime-launch-attach-outcome",
        "jackin-runtime-launch-auth-error",
        "jackin-runtime-launch-capsule-setup",
        "jackin-runtime-launch-debug-envs",
        "jackin-runtime-launch-dind",
        "jackin-runtime-launch-dry-run",
        "jackin-runtime-launch-exit-diagnosis",
        "jackin-runtime-launch-exit-outro",
        "jackin-runtime-launch-failure",
        "jackin-runtime-launch-git-pull",
        "jackin-runtime-launch-image-plan",
        "jackin-runtime-launch-load-cleanup",
        "jackin-runtime-launch-load-options",
        "jackin-runtime-launch-mounts",
        "jackin-runtime-launch-plan",
        "jackin-runtime-launch-post-run-failure",
        "jackin-runtime-launch-programmatic-selection",
        "jackin-runtime-launch-progress-helpers",
        "jackin-runtime-launch-restore",
        "jackin-runtime-launch-run-envs",
        "jackin-runtime-launch-sibling-auth-prewarm",
        "jackin-runtime-launch-slot",
        "jackin-runtime-launch-supported-agents",
        "jackin-runtime-launch-trust",
        "jackin-runtime-naming",
        "jackin-runtime-prewarm-trigger",
        "jackin-runtime-process-telemetry",
        "jackin-runtime-progress",
        "jackin-runtime-prune-output",
        "jackin-runtime-reactive-daemon",
        "jackin-runtime-repo-cache",
        "jackin-runtime-session-control",
        "jackin-runtime-shared-runner",
        "jackin-runtime-snapshot",
        "jackin-runtime-spin-wait",
        "jackin-runtime-universe",
        "jackin-runtime-universe-claims",
        "jackin-runtime-usage-relay",
        "jackin-telemetry",
        "jackin-test-support",
        "jackin-tui",
        "jackin-usage",
        "jackin-usage-broker",
        "jackin-usage-broker-publish",
        "jackin-usage-broker-wire",
        "jackin-usage-capsule-telemetry",
        "jackin-usage-coordinator",
        "jackin-usage-credential-resolver",
        "jackin-usage-credential-snapshots",
        "jackin-usage-destination",
        "jackin-usage-discovery",
        "jackin-usage-ffi",
        "jackin-usage-host-accounts",
        "jackin-usage-host-credentials",
        "jackin-usage-host-glance",
        "jackin-usage-host-presentation",
        "jackin-usage-host-runtime",
        "jackin-usage-output",
        "jackin-usage-projection",
        "jackin-usage-provider-amp",
        "jackin-usage-provider-antigravity",
        "jackin-usage-provider-claude",
        "jackin-usage-provider-codex",
        "jackin-usage-provider-core",
        "jackin-usage-provider-cursor",
        "jackin-usage-provider-gemini",
        "jackin-usage-provider-grok",
        "jackin-usage-provider-hermes",
        "jackin-usage-provider-kimi",
        "jackin-usage-provider-minimax",
        "jackin-usage-provider-muse",
        "jackin-usage-provider-opencode",
        "jackin-usage-provider-openrouter",
        "jackin-usage-provider-zai",
        "jackin-usage-snapshot-store",
        "jackin-usage-store-backend",
        "jackin-usage-token-monitor",
        "jackin-xtask",
    ]
    .into_iter()
    .collect();
    let declared: BTreeSet<&str> = TIERS.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        declared, expected,
        "TIERS drifted from the pinned member set — update both if a crate was added/removed"
    );
    assert_eq!(TIERS.len(), 147);
}

#[test]
fn evaluate_accepts_live_tier_shape_on_synthetic_clean_graph() {
    let t: BTreeMap<&str, u8> = TIERS.iter().copied().collect();
    let prod = edges(&[
        ("jackin-core", &[]),
        ("jackin-config", &["jackin-core"]),
        ("jackin-manifest", &["jackin-config", "jackin-core"]),
        ("jackin-test-support", &["jackin-core", "jackin-manifest"]),
    ]);
    let dev = BTreeMap::new();
    let m = members(&[
        "jackin-core",
        "jackin-config",
        "jackin-manifest",
        "jackin-test-support",
    ]);
    let problems = evaluate(&t, &prod, &dev, &m, &[]);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn tui_ownership_gate_accepts_clean_boundaries() {
    let temp = tempfile::tempdir().unwrap();
    for path in [
        "crates/core/jackin-core/src",
        "crates/services/jackin-runtime/src",
        "crates/ui/jackin-tui/src",
    ] {
        std::fs::create_dir_all(temp.path().join(path)).unwrap();
    }
    std::fs::write(
        temp.path().join("crates/core/jackin-core/Cargo.toml"),
        "[package]\n",
    )
    .unwrap();
    std::fs::write(
        temp.path()
            .join("crates/services/jackin-runtime/Cargo.toml"),
        "[package]\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("crates/ui/jackin-tui/src/lib.rs"),
        "pub mod tokens;\n",
    )
    .unwrap();

    check_tui_ownership(temp.path()).unwrap();
}

#[test]
fn tui_ownership_gate_rejects_core_presentation_and_shared_run_loop() {
    let temp = tempfile::tempdir().unwrap();
    for path in [
        "crates/core/jackin-core/src",
        "crates/services/jackin-runtime/src",
        "crates/ui/jackin-tui/src",
    ] {
        std::fs::create_dir_all(temp.path().join(path)).unwrap();
    }
    std::fs::write(
        temp.path().join("crates/core/jackin-core/Cargo.toml"),
        "[dependencies]\nratatui = \"*\"\n",
    )
    .unwrap();
    std::fs::write(
        temp.path()
            .join("crates/services/jackin-runtime/Cargo.toml"),
        "[package]\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("crates/core/jackin-core/src/lib.rs"),
        "use termrock::Theme;\n",
    )
    .unwrap();
    std::fs::write(temp.path().join("crates/ui/jackin-tui/src/run.rs"), "").unwrap();

    let error = check_tui_ownership(temp.path()).unwrap_err().to_string();
    assert!(error.contains("jackin-core/Cargo.toml"), "{error}");
    assert!(error.contains("jackin-core/src/lib.rs"), "{error}");
    assert!(error.contains("jackin-tui/src/run.rs"), "{error}");
}

#[test]
fn tui_ownership_gate_rejects_surface_local_host_color_handshake() {
    let temp = tempfile::tempdir().unwrap();
    for path in [
        "crates/core/jackin-core/src",
        "crates/services/jackin-runtime/src/runtime",
        "crates/ui/jackin-tui/src",
        "crates/apps/jackin-capsule/src/tui",
    ] {
        std::fs::create_dir_all(temp.path().join(path)).unwrap();
    }
    std::fs::write(
        temp.path().join("crates/core/jackin-core/Cargo.toml"),
        "[package]\n",
    )
    .unwrap();
    std::fs::write(
        temp.path()
            .join("crates/services/jackin-runtime/Cargo.toml"),
        "[package]\n",
    )
    .unwrap();
    std::fs::write(
        temp.path()
            .join("crates/apps/jackin-capsule/src/tui/host_colors.rs"),
        "",
    )
    .unwrap();

    let error = check_tui_ownership(temp.path()).unwrap_err().to_string();
    assert!(error.contains("host_colors.rs"), "{error}");
    assert!(error.contains("jackin-protocol"), "{error}");
}
