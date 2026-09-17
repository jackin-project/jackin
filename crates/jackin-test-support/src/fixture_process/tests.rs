//! Self-tests for the [`FakeProcessHarness`](super::FakeProcessHarness).

use super::{FakeProcessHarness, Invocation, ProcessScript};
use std::io::Result;

#[expect(
    clippy::disallowed_methods,
    reason = "self-test drives the fake binary on the test thread; never a render/runtime path"
)]
fn run(args: &[&str], fake: &super::FakeBinary) -> Result<(String, String, i32)> {
    let output = fake.command().args(args).output()?;
    Ok((
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(-1),
    ))
}

#[test]
fn dispatches_exact_args_to_scripted_outputs() -> Result<()> {
    let harness = FakeProcessHarness::new()?;
    let fake = harness.binary(
        "cli",
        &ProcessScript::new()
            .on_exact(["--version"], "cli 1.2.3\n", "", 0)
            .on_exact(["bad"], "", "boom\n", 3)
            .with_default("default-out", "default-err", 7),
    )?;

    assert_eq!(
        run(&["--version"], &fake)?,
        ("cli 1.2.3\n".to_owned(), String::new(), 0)
    );
    assert_eq!(
        run(&["bad"], &fake)?,
        (String::new(), "boom\n".to_owned(), 3)
    );
    assert_eq!(
        run(&["other"], &fake)?,
        ("default-out".to_owned(), "default-err".to_owned(), 7)
    );
    Ok(())
}

#[test]
fn prefix_entries_match_subcommands_with_trailing_flags() -> Result<()> {
    let harness = FakeProcessHarness::new()?;
    let fake = harness.binary(
        "accounts",
        &ProcessScript::new()
            .on_prefix(["account", "list"], "a1\na2\n", "", 0)
            .with_default("", "unknown", 1),
    )?;

    assert_eq!(run(&["account", "list"], &fake)?.0, "a1\na2\n".to_owned());
    assert_eq!(
        run(&["account", "list", "--json"], &fake)?.0,
        "a1\na2\n".to_owned(),
        "trailing flags still match the prefix"
    );
    let (_, unmatched_err, unmatched_code) = run(&["account", "show"], &fake)?;
    assert_eq!((unmatched_err, unmatched_code), ("unknown".to_owned(), 1));
    Ok(())
}

#[test]
fn version_and_help_stubs_cover_common_spellings() -> Result<()> {
    let harness = FakeProcessHarness::new()?;
    let fake = harness.binary("tool", &ProcessScript::version_stub("tool", "9.9"))?;
    for spelling in ["--version", "version", "-V"] {
        assert_eq!(run(&[spelling], &fake)?.0, "tool 9.9\n".to_owned());
    }

    let fake_help = harness.binary("tool-help", &ProcessScript::help_stub("usage: tool\n"))?;
    for spelling in ["--help", "help", "-h"] {
        assert_eq!(run(&[spelling], &fake_help)?.0, "usage: tool\n".to_owned());
    }
    Ok(())
}

#[test]
fn shell_metacharacters_in_bodies_are_emitted_literally() -> Result<()> {
    let tricky = "it's a $VAR `trap` \"quoted\" $(evil)\nline2\n";
    let harness = FakeProcessHarness::new()?;
    let fake = harness.binary(
        "tricky",
        &ProcessScript::new().on_exact(["go"], tricky, tricky, 0),
    )?;
    let (stdout, stderr, code) = run(&["go"], &fake)?;
    assert_eq!(stdout, tricky, "stdout must not be shell-expanded");
    assert_eq!(stderr, tricky, "stderr must not be shell-expanded");
    assert_eq!(code, 0);
    Ok(())
}

#[test]
fn special_characters_in_args_match_literally() -> Result<()> {
    let harness = FakeProcessHarness::new()?;
    let fake = harness.binary(
        "glob",
        &ProcessScript::new()
            .on_exact(["show", "a*b [x]?"], "literal\n", "", 0)
            .on_exact(["show", "it's\\here $HOME"], "quoted\n", "", 0)
            .with_default("", "", 9),
    )?;
    assert_eq!(run(&["show", "a*b [x]?"], &fake)?.0, "literal\n");
    assert_eq!(
        run(&["show", "aab x"], &fake)?.2,
        9,
        "glob chars must not act as wildcards"
    );
    assert_eq!(
        run(&["show", "it's\\here $HOME"], &fake)?.0,
        "quoted\n",
        "quotes, backslashes, and $ match literally without expansion"
    );
    assert_eq!(
        run(&["show"], &fake)?.2,
        9,
        "arity mismatch falls through to the default"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "self-test drives the fake binary on the test thread; never a render/runtime path"
)]
fn records_argv_and_env_per_invocation_in_order() -> Result<()> {
    let harness = FakeProcessHarness::new()?;
    let fake = harness.binary("rec", &ProcessScript::new().with_default("", "", 0))?;
    assert!(
        fake.invocations()?.is_empty(),
        "no spawns yet means no invocations"
    );

    fake.command()
        .args(["account", "list"])
        .env("FAKE_MARKER", "marker-value")
        .output()?;
    fake.command().arg("--version").output()?;

    let invocations = fake.invocations()?;
    assert_eq!(invocations.len(), 2);
    assert_eq!(
        invocations[0].args(),
        ["account".to_owned(), "list".to_owned()]
    );
    assert_eq!(invocations[0].env_get("FAKE_MARKER"), Some("marker-value"));
    assert!(
        invocations[0].env_get("PATH").is_some(),
        "inherited env is captured"
    );
    assert_eq!(invocations[1].args(), ["--version".to_owned()]);
    assert!(
        invocations[0].argv[0].ends_with("rec.sh"),
        "argv[0] is the script path: {:?}",
        invocations[0].argv[0]
    );
    Ok(())
}

#[test]
fn assert_no_secrets_passes_on_clean_invocation() {
    let invocation = Invocation {
        argv: vec!["fake".to_owned(), "account".to_owned()],
        env: vec![("TOKEN".to_owned(), "test-token-123".to_owned())],
    };
    invocation.assert_no_secrets(&["real-access-token", ""]);
}

#[test]
#[should_panic(expected = "spawned argv leaked forbidden value #0")]
fn assert_no_secrets_panics_on_argv_leak() {
    let invocation = Invocation {
        argv: vec!["fake".to_owned(), "--token=hunter2".to_owned()],
        env: Vec::new(),
    };
    invocation.assert_no_secrets(&["hunter2"]);
}

#[test]
#[should_panic(expected = "spawned env leaked forbidden value #0 in API_KEY")]
fn assert_no_secrets_panics_on_env_leak() {
    let invocation = Invocation {
        argv: vec!["fake".to_owned()],
        env: vec![("API_KEY".to_owned(), "hunter2".to_owned())],
    };
    invocation.assert_no_secrets(&["hunter2"]);
}

#[test]
#[should_panic(expected = "plain file name")]
fn binary_name_with_separator_is_rejected() {
    // If harness setup fails the expected panic never fires and this fails.
    if let Ok(harness) = FakeProcessHarness::new() {
        let _binary = harness.binary("../escape", &ProcessScript::new());
    }
}
