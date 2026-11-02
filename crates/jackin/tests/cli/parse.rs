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
    (&["account", "rm"], &["account", "rm", "ghost"]),
    (
        &[
            "account",
            "add",
            "nosuch",
            "--name",
            "a",
            "--credential",
            "env:K",
        ],
        &[
            "account",
            "add",
            "claude",
            "--name",
            "a",
            "--credential",
            "env:K",
        ],
    ),
    (
        &["account", "add", "claude", "--name", "a"],
        &[
            "account",
            "add",
            "claude",
            "--name",
            "b",
            "--credential",
            "env:K",
        ],
    ),
    (
        &[
            "account",
            "add",
            "codex",
            "--name",
            "a",
            "--chatgpt-from",
            "h",
            "--credential",
            "env:K",
        ],
        &[
            "account",
            "add",
            "claude",
            "--name",
            "c",
            "--credential",
            "env:K",
        ],
    ),
    (
        &[
            "account",
            "add",
            "claude",
            "--name",
            "a",
            "--credential",
            "plain",
        ],
        &[
            "account",
            "add",
            "claude",
            "--name",
            "d",
            "--credential",
            "op://v/i/f",
        ],
    ),
    (
        &[
            "account",
            "add",
            "claude",
            "--name",
            "Bad Name",
            "--credential",
            "env:K",
        ],
        &[
            "account",
            "add",
            "claude",
            "--name",
            "e",
            "--credential",
            "file:key.txt",
        ],
    ),
    (&["build", "--nope"], &["build", "--json"]),
    (
        &["load", "--launch-id", "bad id"],
        &["load", "--launch-id", "good-id"],
    ),
    (&["load", "--env", "NOEQUALS"], &["load", "--env", "A=1"]),
    (&["load", "--env", "1BAD=x"], &["load", "--env", "GOOD=x"]),
    (&["load", "a", ".", "extra"], &["load", "a", "."]),
    (&["attach"], &["attach", "jackin-x-0001"]),
    (
        &["attach", "jackin-x-0001", "--nope"],
        &["attach", "jackin-x-0001", "--restart"],
    ),
    (&["doctor", "extra"], &["doctor"]),
    (&["ls", "extra"], &["ls", "--json"]),
    (&["inspect"], &["inspect", "jackin-x-0001"]),
    (
        &["inspect", "Bad Name"],
        &["inspect", "jackin-x-0002", "--json"],
    ),
    (&["build", "a", "b"], &["build", "a"]),
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
