// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_testkit::{TestResult, World, guard};

use super::support::Harness;

const CASES: &[(&[&str], &[&str])] = &[
    (&["role", "check"], &["role", "check", "."]),
    (&["bogus"], &["role", "check", "."]),
    (&["role", "check", ".", "--nope"], &["role", "check", "."]),
    (
        &["--gateway", "Bad Name", "role", "check", "."],
        &["--gateway", "good", "role", "check", "."],
    ),
    (&["role"], &["role", "check", "."]),
    (&["gateway", "add"], &["gateway", "add", "alpha"]),
    (&["gateway", "add", "Bad Name"], &["gateway", "add", "beta"]),
    (
        &["gateway", "add", "x", "--engine-port", "2376"],
        &[
            "gateway",
            "add",
            "gamma",
            "--engine-port",
            "2376",
            "--engine-certs",
            "certs",
        ],
    ),
    (
        &["gateway", "add", "x", "--platform", "linux/riscv"],
        &["gateway", "add", "delta", "--platform", "linux/arm64"],
    ),
    (&["gateway", "rm"], &["gateway", "rm", "alpha"]),
    (
        &["workspace", "add", "only"],
        &["workspace", "add", "only", "proj"],
    ),
    (&["workspace", "rm"], &["workspace", "rm", "only"]),
    (
        &["role", "add", "x"],
        &["role", "add", "x", "https://example.com/x.git"],
    ),
    (&["role", "rm"], &["role", "rm", "x"]),
    (
        &["role", "ls", "--json", "extra"],
        &["role", "ls", "--json"],
    ),
    (&["account", "default"], &["account", "default", "ghost"]),
    (
        &["account", "bind", "a"],
        &["account", "bind", "a", "--role", "r"],
    ),
    (
        &["account", "bind", "a", "--role", "r", "--workspace", "w"],
        &["account", "bind", "a", "--workspace", "w"],
    ),
];

#[tokio::test]
async fn malformed_invocations_exit_usage_with_a_stderr_message_and_change_nothing_while_their_valid_counterparts_do_not()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        for (bad, good) in CASES {
            let before = harness.config_bytes()?;
            let ran = harness.run(bad).await?;
            assert_eq!(ran.exit, ExitClass::Usage, "{bad:?}");
            assert!(ran.stdout.is_empty(), "{bad:?}: {}", ran.stdout);
            assert!(!ran.stderr.is_empty(), "{bad:?}");
            assert_eq!(harness.config_bytes()?, before, "{bad:?}");
            assert!(world.gateway.sandboxes().is_empty(), "{bad:?}");
            assert!(world.gateway.providers().is_empty(), "{bad:?}");
            assert!(world.engine.builds().is_empty(), "{bad:?}");
            let valid = harness.run(good).await?;
            assert_ne!(valid.exit, ExitClass::Usage, "{good:?}: {}", valid.stderr);
        }
        Ok(())
    })
    .await
}
