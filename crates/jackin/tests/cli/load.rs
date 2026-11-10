// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::{AccountName, AttemptId, SandboxLabels, SupervisorState};
use jackin_testkit::{Rig, TestResult, guard, launch_id};

use super::support::{Failure, Harness, ScriptedTerminal, Step, failed, host_path};

const DETACH: &[u8] = b"\x10\x11";

fn second_account(rig: &Rig) -> String {
    format!(
        "{}\n[defaults.accounts]\nzai = \"work\"\n\n[accounts.second]\nprovider = \"zai\"\ncredential = {{ source = \"file\", path = \"{}/zai.txt\" }}\n",
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
        assert_eq!(
            labels.planned.agents.agents_label(),
            jackin_core::AgentInstances::single(jackin_core::AgentInstance::new(
                jackin_core::Agent::Codex,
                jackin_core::Provider::Zai,
                AccountName::parse("second")?,
                "glm-5.3".to_owned(),
            ))
            .agents_label()
        );
        assert!(rig.world.engine.builds().len() > builds);
        assert_eq!(rig.world.gateway.sandboxes().len(), 2);
        Ok(())
    })
    .await
}

fn multi_account_config(rig: &Rig) -> String {
    format!(
        "{}\n[defaults.accounts]\nzai = \"work\"\n\n[accounts.second]\nprovider = \"zai\"\ncredential = {{ source = \"file\", path = \"{}/zai.txt\" }}\n\n[accounts.zaiacct]\nprovider = \"zai\"\ncredential = {{ source = \"file\", path = \"{}/zai.txt\" }}\n\n[accounts.codexacct]\nprovider = \"codex\"\ncredential = {{ source = \"codex-login\", home = \"{}\", chatgpt_account_id = \"3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f\" }}\n",
        rig.config.render(),
        rig.home.as_path().display(),
        rig.home.as_path().display(),
        rig.home.as_path().display()
    )
}

#[tokio::test]
async fn account_for_and_provider_scoped_global_account_resolve_per_instance() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        std::fs::write(rig.home.as_path().join("zai.txt"), "test-key")?;
        harness.write_config(&multi_account_config(&rig))?;
        let base = [
            "load",
            "--agent",
            "codex",
            "--agent",
            "omp",
            "--provider-for",
            "omp=zai",
        ];
        let mut args: Vec<String> = base.iter().map(|s| (*s).to_owned()).collect();
        args.extend(
            [
                "--account-for",
                "codex=second",
                "--account-for",
                "omp=zaiacct",
                "--launch-id",
            ]
            .iter()
            .map(|s| (*s).to_owned()),
        );
        args.push(launch_id(3).to_string());
        harness
            .run(&args.iter().map(String::as_str).collect::<Vec<_>>())
            .await?
            .ok()?;
        let boxes = rig.world.gateway.sandboxes();
        assert_eq!(boxes.len(), 1);
        let text = String::from_utf8(
            boxes[0]
                .file(&jackin_core::SandboxPath::from_static(
                    "/sandbox/.jackin/agents.json",
                ))
                .ok_or("agents.json")?,
        )?;
        assert!(text.contains("\"account\":\"second\""), "{text}");
        assert!(text.contains("\"account\":\"zaiacct\""), "{text}");

        let mut args2: Vec<String> = base.iter().map(|s| (*s).to_owned()).collect();
        args2.extend(
            ["--account", "second", "--launch-id"]
                .iter()
                .map(|s| (*s).to_owned()),
        );
        args2.push(launch_id(4).to_string());
        harness
            .run(&args2.iter().map(String::as_str).collect::<Vec<_>>())
            .await?
            .ok()?;
        assert_eq!(rig.world.gateway.sandboxes().len(), 2);

        let mut args3: Vec<String> = base.iter().map(|s| (*s).to_owned()).collect();
        args3.extend(
            ["--account-for", "codex=codexacct", "--launch-id"]
                .iter()
                .map(|s| (*s).to_owned()),
        );
        args3.push(launch_id(5).to_string());
        let ran = harness
            .run(&args3.iter().map(String::as_str).collect::<Vec<_>>())
            .await?;
        failed(&ran, ExitClass::Usage, &["does not belong"])?;
        assert_eq!(rig.world.gateway.sandboxes().len(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_single_instance_global_account_with_the_wrong_provider_is_rejected_before_effects()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        std::fs::write(rig.home.as_path().join("zai.txt"), "test-key")?;
        harness.write_config(&multi_account_config(&rig))?;
        let ran = harness.run(&["load", "--account", "codexacct"]).await?;
        failed(&ran, ExitClass::Usage, &[])?;
        assert_eq!(
            ran.stderr,
            "error: account codexacct does not belong to the zai provider\n"
        );
        assert!(rig.world.engine.builds().is_empty());
        assert!(rig.world.gateway.providers().is_empty());
        assert!(rig.world.gateway.sandboxes().is_empty());
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

async fn unavailable_tuple_refusal(agent: &str, provider: &str) -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut harness = configured(&rig)?;
        harness.cwd = rig.repo.root().clone();
        let ran = harness
            .run(&[
                "load",
                "--agent",
                agent,
                "--provider",
                provider,
                "--launch-id",
                &launch_id(9).to_string(),
            ])
            .await?;
        failed(&ran, ExitClass::Usage, &[])?;
        assert_eq!(
            ran.stderr,
            format!("error: tuple {agent} with provider {provider} is not supported\n")
        );
        assert!(rig.world.gateway.sandboxes().is_empty());
        assert!(rig.world.gateway.providers().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}

macro_rules! unavailable_tuple_tests {
    ($($name:ident => ($agent:literal, $provider:literal)),+ $(,)?) => {
        $(
            #[tokio::test]
            async fn $name() -> TestResult {
                unavailable_tuple_refusal($agent, $provider).await
            }
        )+
    };
}

unavailable_tuple_tests! {
    t01_load_refusal => ("codex", "codex"),
    t10_load_refusal => ("omp", "kimi"),
    t11_load_refusal => ("omp", "minimax"),
    t12_load_refusal => ("omp", "mimo"),
    t16_load_refusal => ("claude", "mimo"),
    t20_load_refusal => ("codex", "mimo"),
}
