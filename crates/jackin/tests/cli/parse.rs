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
    (&["usage", "--nope"], &["usage"]),
    (&["usage", "extra"], &["usage"]),
    (&["usage", "--refresh"], &["usage"]),
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
    (&["shell"], &["shell", "jackin-x-0001"]),
    (&["export"], &["export", "jackin-x-0001"]),
    (&["stop"], &["stop", "jackin-x-0001"]),
    (&["start", "Bad Name"], &["start", "jackin-x-0001"]),
    (&["purge"], &["purge", "jackin-x-0001"]),
    (
        &["purge", "jackin-x-0001", "--nope"],
        &["purge", "jackin-x-0001", "--force"],
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
        let mut harness = Harness::new(&world);
        harness.env.push(("K".to_owned(), "fake-value".to_owned()));
        std::fs::write(world.home().as_path().join("key.txt"), "fake-key")?;
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

const FAKE: &str = "sk-FAKE-VALUE-123";

async fn never_echoed(cases: &[(&[&str], &str)], canary: &str) -> TestResult {
    let world = World::builder().build()?;
    let harness = Harness::new(&world);
    let mut wrong = Vec::new();
    for (args, expected) in cases {
        let before = harness.config_bytes()?;
        let ran = harness.run(args).await?;
        let echoed = ran.stderr.to_lowercase().contains(&canary.to_lowercase());
        if ran.exit != ExitClass::Usage
            || !ran.stdout.is_empty()
            || !ran.stderr.contains(expected)
            || echoed
            || harness.config_bytes()? != before
            || !world.gateway.providers().is_empty()
        {
            wrong.push(format!("{args:?} -> {:?}: {}", ran.exit, ran.stderr));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    Ok(())
}

#[tokio::test]
async fn a_rejected_credential_is_never_echoed_and_the_error_names_the_expected_forms() -> TestResult
{
    guard(async {
        let variable = format!("env:{FAKE}");
        let reference = format!("op://{FAKE}");
        let forms = [FAKE, &variable, &reference];
        for form in forms {
            let args = [
                "account",
                "add",
                "codex",
                "--name",
                "x",
                "--credential",
                form,
            ];
            never_echoed(
                &[(&args, "env:NAME, an op:// reference, file:PATH, or omp-host")],
                "sk-",
            )
            .await?;
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_rejected_env_assignment_is_never_echoed_and_the_error_names_the_expected_form()
-> TestResult {
    guard(async {
        let bad_name = format!("1{FAKE}=x");
        let bad_value = format!("1BAD={FAKE}");
        let forms = [FAKE, &bad_name, &bad_value];
        for form in forms {
            never_echoed(&[(&["load", "--env", form], "NAME=VALUE")], "sk-").await?;
        }
        Ok(())
    })
    .await
}

const ECHO_CASES: &[(&[&str], &str)] = &[
    (
        &[
            "account",
            "add",
            "sk-FAKE-1",
            "--name",
            "x",
            "--credential",
            "env:A",
        ],
        "codex, cursor",
    ),
    (
        &[
            "account",
            "add",
            "codex",
            "--name",
            "x",
            "--credential",
            "--sk-FAKE-2",
        ],
        "Usage: jackin account add",
    ),
    (
        &[
            "account",
            "add",
            "codex",
            "--name",
            "x",
            "--credential",
            "env:A",
            "sk-FAKE-3",
        ],
        "Usage: jackin account add",
    ),
    (
        &[
            "account",
            "add",
            "codex",
            "--name",
            "sk-FAKE-4",
            "--credential",
            "env:A",
        ],
        "for '--name <NAME>'",
    ),
    (
        &["--gateway", "sk-FAKE-5 x", "doctor"],
        "for '--gateway <GATEWAY>'",
    ),
    (&["sk-FAKE-6"], "unrecognized subcommand"),
    (
        &["gateway", "add", "gw", "--platform", "sk-FAKE-7"],
        "expected one of: linux/arm64, linux/amd64",
    ),
    (&["--sk-FAKE-8", "doctor"], "Usage: jackin"),
    (&["-sk-FAKE-9", "doctor"], "Usage: jackin"),
    (&["usage", "sk-FAKE-10"], "Usage: jackin usage"),
    (
        &["load", "--launch-id", "sk-FAKE-11 x"],
        "for '--launch-id <LAUNCH_ID>'",
    ),
    (
        &["account", "add", "codex", "--name=sk-FAKE-12"],
        "for '--name <NAME>'",
    ),
    (
        &[
            "account",
            "bind",
            "sk-fake-13",
            "--role",
            "r",
            "--workspace",
            "w",
        ],
        "conflicting arguments",
    ),
    (
        &["workspace", "add", "sk-FAKE-14"],
        "missing required arguments",
    ),
];

#[tokio::test]
async fn no_parse_error_prints_a_value_the_user_typed_and_each_names_the_expected_form()
-> TestResult {
    guard(never_echoed(ECHO_CASES, "sk-FAKE")).await
}
