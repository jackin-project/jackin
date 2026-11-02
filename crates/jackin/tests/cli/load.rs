// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::{AccountName, AttemptId, SandboxLabels, SupervisorState};
use jackin_testkit::{ACCOUNT_ID, Rig, TestResult, guard, launch_id};

use super::support::{Failure, Harness, ScriptedTerminal, Step, failed, host_path};

const DETACH: &[u8] = b"\x10\x11";

fn second_account(rig: &Rig) -> String {
    format!(
        "{}\n[defaults.accounts]\ncodex = \"work\"\n\n[accounts.second]\nprovider = \"codex\"\ncredential = {{ source = \"codex-login\", home = \"{}\", chatgpt_account_id = \"{ACCOUNT_ID}\" }}\n",
        rig.config.render(),
        rig.home.as_path().display()
    )
}

fn configured(rig: &Rig) -> Result<Harness, Failure> {
    let mut harness = Harness::new(&rig.world);
    harness.cwd = rig.repo.root().clone();
    harness.write_config(&second_account(rig))?;
    Ok(harness)
}

fn detach_after_defaults() -> Result<ScriptedTerminal, Failure> {
    let steps = vec![Step::Line(String::new()), Step::Bytes(DETACH.to_vec())];
    Ok(ScriptedTerminal::new(steps)?.0)
}

#[tokio::test]
async fn load_on_a_terminal_launches_one_labelled_sandbox_attaches_and_restarts_on_the_same_launch_id()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let harness = configured(&rig)?;
        let before = harness.config_bytes()?;
        let id = launch_id(1).to_string();
        let ran = harness
            .command(&["load", "--launch-id", &id])
            .terminal(detach_after_defaults()?)
            .run()
            .await?;
        ran.ok()?;
        let boxes = rig.world.gateway.sandboxes();
        assert_eq!(boxes.len(), 1);
        let handle = &boxes[0];
        let labels = SandboxLabels::from_labels(&handle.labels())?;
        assert_eq!(&labels.planned, rig.plan(1)?.draft().labels());
        assert_eq!(labels.planned.launch, launch_id(1));
        assert_eq!(
            handle.supervisor(),
            Some(SupervisorState::Running {
                attempt: AttemptId::FIRST
            })
        );
        assert_eq!(harness.config_bytes()?, before);
        assert!(!rig.gateway().providers().is_empty() && !rig.gateway().profiles().is_empty());

        handle.print_main(b"hello from codex");
        handle.exit_codex(0);
        let again = harness
            .command(&["load", "--launch-id", &id])
            .terminal(detach_after_defaults()?)
            .run()
            .await?;
        again.ok()?;
        assert_eq!(rig.world.gateway.sandboxes().len(), 1);
        assert!(
            again.stdout.contains("hello from codex"),
            "{}",
            again.stdout
        );
        let second = AttemptId::FIRST.next().ok_or("no second attempt")?;
        assert_eq!(
            handle.supervisor(),
            Some(SupervisorState::Running { attempt: second })
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn load_without_a_terminal_prints_the_sandbox_and_launch_id_and_uses_the_cwd_account_and_rebuild()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut harness = configured(&rig)?;
        let first = harness
            .run(&["load", "--launch-id", &launch_id(1).to_string()])
            .await?;
        first.ok()?;
        let name = rig.sandbox_name(1)?;
        assert_eq!(first.stdout, format!("{name}\t{}\n", launch_id(1)));

        let parent = rig.repo.root().as_path().parent().ok_or("no parent")?;
        let dir = rig.repo.root().as_path().file_name().ok_or("no name")?;
        harness.cwd = host_path(parent)?;
        let builds = rig.world.engine.builds().len();
        let relative = dir.to_string_lossy().into_owned();
        let args = [
            "load",
            "sentinel",
            &relative,
            "--account",
            "second",
            "--rebuild",
            "--launch-id",
            &launch_id(2).to_string(),
        ];
        harness.run(&args).await?.ok()?;
        let handle = rig.handle(2).await?;
        let labels = SandboxLabels::from_labels(&handle.labels())?;
        assert_eq!(labels.planned.account, AccountName::parse("second")?);
        assert!(rig.world.engine.builds().len() > builds);
        assert_eq!(rig.world.gateway.sandboxes().len(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_path_without_a_workspace_or_an_unknown_role_fails_and_creates_nothing() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut harness = configured(&rig)?;
        std::fs::create_dir(rig.home.as_path().join("elsewhere"))?;
        harness.cwd = rig.home.clone();
        let ran = harness.run(&["load", "elsewhere"]).await?;
        failed(&ran, ExitClass::Failure, &["jackin workspace add"])?;

        harness.cwd = rig.repo.root().clone();
        let ran = harness.run(&["load", "ghostrole"]).await?;
        failed(&ran, ExitClass::Failure, &["ghostrole"])?;
        assert!(rig.world.gateway.sandboxes().is_empty());
        assert!(rig.world.gateway.providers().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unknown_role_is_named_as_a_plain_word() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut harness = configured(&rig)?;
        harness.cwd = rig.repo.root().clone();
        let ran = harness.run(&["load", "ghostrole"]).await?;
        failed(&ran, ExitClass::Failure, &[])?;
        assert_eq!(ran.stderr, "error: unknown role ghostrole\n");
        assert!(rig.world.gateway.sandboxes().is_empty());
        assert!(rig.world.gateway.providers().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}
