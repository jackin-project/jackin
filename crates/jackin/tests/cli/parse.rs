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
];

#[tokio::test]
async fn malformed_invocations_exit_usage_with_a_stderr_message_and_change_nothing_while_their_valid_counterparts_do_not()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        for (bad, good) in CASES {
            let ran = harness.run(bad).await?;
            assert_eq!(ran.exit, ExitClass::Usage, "{bad:?}");
            assert!(ran.stdout.is_empty(), "{bad:?}: {}", ran.stdout);
            assert!(!ran.stderr.is_empty(), "{bad:?}");
            assert_eq!(harness.config_bytes()?, None, "{bad:?}");
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
