// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin::ExitClass;
use jackin_core::{EnvFile, sandbox_layout};
use jackin_testkit::{Rig, RoleFixture, TestResult, guard, launch_id};

use super::support::{Failure, Harness, ScriptedTerminal, Step, failed};

const DETACH: &[u8] = b"\x10\x11";

async fn rigged() -> Result<(Rig, Harness), Failure> {
    let role = RoleFixture::new("sentinel")?
        .env("GREETING", Some("hello"), Some("Greeting"))
        .env("TOKENISH", None, None);
    let rig = Rig::builder().role(role).build().await?;
    let mut harness = Harness::new(&rig.world);
    harness.cwd = rig.repo.root().clone();
    harness.write_config(&rig.config.render())?;
    Ok((rig, harness))
}

async fn values(rig: &Rig, id: u8) -> Result<BTreeMap<String, String>, Failure> {
    let handle = rig.handle(id).await?;
    let bytes = handle
        .file(&sandbox_layout::env_file(&launch_id(id)))
        .ok_or("no env file")?;
    let file = EnvFile::parse(&String::from_utf8(bytes)?)?;
    Ok(file
        .values()
        .iter()
        .map(|(name, value)| (name.to_string(), value.clone()))
        .collect())
}

fn nothing_created(rig: &Rig) -> bool {
    rig.world.gateway.sandboxes().is_empty()
        && rig.world.gateway.providers().is_empty()
        && rig.world.engine.builds().is_empty()
}

fn lines(answers: &[&str]) -> Result<ScriptedTerminal, Failure> {
    let mut steps: Vec<Step> = answers
        .iter()
        .map(|text| Step::Line((*text).to_owned()))
        .collect();
    steps.push(Step::Bytes(DETACH.to_vec()));
    Ok(ScriptedTerminal::new(steps)?.0)
}

#[tokio::test]
async fn without_a_terminal_defaults_apply_and_the_env_flag_wins() -> TestResult {
    guard(async {
        let (rig, harness) = rigged().await?;
        let id = launch_id(1).to_string();
        harness
            .run(&["load", "--launch-id", &id, "--env", "TOKENISH=t1"])
            .await?
            .ok()?;
        let seen = values(&rig, 1).await?;
        assert_eq!(seen.get("GREETING").map(String::as_str), Some("hello"));
        assert_eq!(seen.get("TOKENISH").map(String::as_str), Some("t1"));

        let id = launch_id(2).to_string();
        let args = [
            "load",
            "--launch-id",
            &id,
            "--env",
            "TOKENISH=t2",
            "--env",
            "GREETING=hola",
        ];
        harness.run(&args).await?.ok()?;
        assert_eq!(
            values(&rig, 2).await?.get("GREETING").map(String::as_str),
            Some("hola")
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn on_a_terminal_each_entry_is_asked_with_its_prompt_and_default_and_flags_skip_the_question()
-> TestResult {
    guard(async {
        let (rig, harness) = rigged().await?;
        let id = launch_id(1).to_string();
        let ran = harness
            .command(&["load", "--launch-id", &id])
            .terminal(lines(&["", "typed-value"])?)
            .run()
            .await?;
        ran.ok()?;
        assert!(
            ran.stderr.contains("Greeting") && ran.stderr.contains("hello"),
            "{}",
            ran.stderr
        );
        let seen = values(&rig, 1).await?;
        assert_eq!(seen.get("GREETING").map(String::as_str), Some("hello"));
        assert_eq!(
            seen.get("TOKENISH").map(String::as_str),
            Some("typed-value")
        );

        let id = launch_id(2).to_string();
        let ran = harness
            .command(&["load", "--launch-id", &id, "--env", "TOKENISH=given"])
            .terminal(lines(&["hola"])?)
            .run()
            .await?;
        ran.ok()?;
        assert!(!ran.stderr.contains("TOKENISH"), "{}", ran.stderr);
        let seen = values(&rig, 2).await?;
        assert_eq!(seen.get("GREETING").map(String::as_str), Some("hola"));
        assert_eq!(seen.get("TOKENISH").map(String::as_str), Some("given"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_missing_value_undeclared_name_or_closed_input_stops_before_anything_is_created()
-> TestResult {
    guard(async {
        let (rig, harness) = rigged().await?;
        let refused = harness.run(&["load"]).await?;
        failed(&refused, ExitClass::Refused, &["TOKENISH"])?;
        let undeclared = harness.run(&["load", "--env", "UNDECLARED=1"]).await?;
        failed(&undeclared, ExitClass::Failure, &["UNDECLARED"])?;
        let (closed, _) = ScriptedTerminal::new(vec![Step::Line(String::new()), Step::Close])?;
        let ran = harness.command(&["load"]).terminal(closed).run().await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stdout.is_empty());
        assert!(nothing_created(&rig));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_prompt_and_a_default_from_the_role_reach_the_terminal_as_printable_text() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?
            .env(
                "GREETING",
                Some("hello\x1b[2Jthere\x1b]0;owned\x07"),
                Some("\x1b]52;c;ZXZpbA==\x07Deploy\x1b[31m key\u{9b}2J"),
            )
            .env("TOKENISH", None, None);
        let rig = Rig::builder().role(role).build().await?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.write_config(&rig.config.render())?;
        let ran = harness
            .command(&["load"])
            .terminal(lines(&["", "typed"])?)
            .run()
            .await?;
        ran.ok()?;
        assert!(
            ran.stderr.contains("Deploy key2J [hellothere]: "),
            "{:?}",
            ran.stderr
        );
        let controls: Vec<char> = ran
            .stderr
            .chars()
            .filter(|c| c.is_control() && !matches!(c, '\n' | '\t'))
            .collect();
        assert!(controls.is_empty(), "{controls:?} in {:?}", ran.stderr);
        Ok(())
    })
    .await
}
