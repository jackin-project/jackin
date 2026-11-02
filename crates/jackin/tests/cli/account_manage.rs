// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::{AccountLs, ExitClass};
use jackin_core::{
    AccountName, Agent, Config, ConfigEdit, Provider, ProviderName, RoleName, WorkspaceName,
};
use jackin_testkit::{Rig, TestResult, World, guard};
use tokio_util::sync::CancellationToken;

use super::support::{Failure, FixedConnector, GATEWAY_CONFIG, Harness, failed};

const CONFIG: &str = "[defaults.accounts]
codex = \"one\"

[accounts.one]
provider = \"codex\"
credential = { source = \"env\", var = \"HOST_SECRET_VAR\" }

[accounts.two]
provider = \"codex\"
credential = { source = \"op\", reference = \"op://vault-name/item-name/field\" }

[accounts.three]
provider = \"claude\"
credential = { source = \"file\", path = \"/secret/dir/key.txt\" }

[accounts.four]
provider = \"codex\"
credential = { source = \"codex-login\", home = \"/login/home\", chatgpt_account_id = \"3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f\" }

[roles.sentinel]
source = { path = \"/roles/sentinel\" }

[workspaces.alpha]
path = \"/work/alpha\"
";

fn seeded(world: &World) -> Result<Harness, Failure> {
    let harness = Harness::new(world);
    harness.write_config(CONFIG)?;
    Ok(harness)
}

fn expect(edit: impl FnOnce(&mut Config) -> Result<(), Failure>) -> Result<String, Failure> {
    let mut config = Config::parse(CONFIG)?;
    edit(&mut config)?;
    Ok(config.render())
}

async fn rendered(harness: &Harness) -> Result<String, Failure> {
    Ok(harness.stored().await?.render())
}

#[tokio::test]
async fn default_and_bind_change_exactly_the_named_entry() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = seeded(&world)?;

        harness.run(&["account", "default", "two"]).await?.ok()?;
        let after_default = expect(|config| {
            config.apply(ConfigEdit::SetDefaultAccount {
                account: AccountName::parse("two")?,
            })?;
            Ok(())
        })?;
        assert_eq!(rendered(&harness).await?, after_default);

        harness
            .run(&["account", "bind", "two", "--role", "sentinel"])
            .await?
            .ok()?;
        let config = harness.stored().await?;
        let bound = config
            .role(&RoleName::parse("sentinel")?)
            .ok_or("no role")?;
        assert_eq!(
            bound.bindings.get(&Agent::Codex).map(AccountName::as_str),
            Some("two")
        );
        let alpha = WorkspaceName::parse("alpha")?;
        assert!(
            config
                .workspace(&alpha)
                .ok_or("no workspace")?
                .bindings
                .is_empty()
        );
        assert_eq!(
            config
                .default_account(Provider::Codex)
                .map(AccountName::as_str),
            Some("two")
        );

        harness
            .run(&["account", "bind", "three", "--workspace", "alpha"])
            .await?
            .ok()?;
        let config = harness.stored().await?;
        let found = config.workspace(&alpha).ok_or("no workspace")?;
        assert_eq!(
            found
                .bindings
                .get(&Provider::Claude)
                .map(AccountName::as_str),
            Some("three")
        );
        let role = config
            .role(&RoleName::parse("sentinel")?)
            .ok_or("no role")?;
        assert_eq!(role.bindings.len(), 1);

        let roles = harness.run(&["role", "ls", "--json"]).await?;
        assert_eq!(
            roles.stdout,
            concat!(
                "{\"schema\":1,\"roles\":[{\"name\":\"sentinel\",",
                "\"source\":{\"kind\":\"path\",\"path\":\"/roles/sentinel\"},",
                "\"bindings\":{\"codex\":\"two\"}}]}\n"
            )
        );
        let workspaces = harness.run(&["workspace", "ls", "--json"]).await?;
        assert_eq!(
            workspaces.stdout,
            concat!(
                "{\"schema\":1,\"workspaces\":[{\"name\":\"alpha\",\"path\":\"/work/alpha\",",
                "\"docker\":false,\"default_role\":null,\"bindings\":{\"claude\":\"three\"}}]}\n"
            )
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn account_ls_json_has_a_pinned_shape_and_no_credential_material() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = seeded(&world)?;
        let ran = harness.run(&["account", "ls", "--json"]).await?;
        ran.ok()?;
        assert_eq!(
            ran.stdout,
            concat!(
                "{\"schema\":1,\"accounts\":[",
                "{\"name\":\"four\",\"provider\":\"codex\",\"credential\":\"codex-login\",\"default\":false},",
                "{\"name\":\"one\",\"provider\":\"codex\",\"credential\":\"env\",\"default\":true},",
                "{\"name\":\"three\",\"provider\":\"claude\",\"credential\":\"file\",\"default\":false},",
                "{\"name\":\"two\",\"provider\":\"codex\",\"credential\":\"op\",\"default\":false}]}\n"
            )
        );
        let typed: AccountLs = ran.json()?;
        assert_eq!(typed.accounts.len(), 4);
        for secret in ["HOST_SECRET_VAR", "vault-name", "/secret/dir", "/login/home"] {
            assert!(!ran.stdout.contains(secret), "{secret}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn binding_an_unknown_or_mismatched_account_fails_without_a_write() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = seeded(&world)?;
        let before = harness.config_bytes()?;
        let cases: [(&[&str], &str); 4] = [
            (&["account", "bind", "ghost", "--role", "sentinel"], "ghost"),
            (
                &["account", "bind", "ghost", "--workspace", "alpha"],
                "ghost",
            ),
            (&["account", "bind", "three", "--role", "sentinel"], "three"),
            (&["account", "default", "ghost"], "ghost"),
        ];
        for (args, named) in cases {
            failed(&harness.run(args).await?, ExitClass::Failure, &[named])?;
            assert_eq!(harness.config_bytes()?, before, "{args:?}");
        }
        Ok(())
    })
    .await
}

async fn synced(rig: &Rig) -> Result<(Harness, ProviderName), Failure> {
    let harness = Harness::new(&rig.world);
    harness.write_config(&rig.config.render())?;
    let work = AccountName::parse("work")?;
    let account = rig.config.account(&work).ok_or("no work account")?;
    rig.world
        .session
        .sync_account(account, &CancellationToken::new())
        .await?;
    let provider = ProviderName::for_account(&work, Agent::Codex);
    assert!(rig.gateway().provider(&provider).is_some());
    Ok((harness, provider))
}

#[tokio::test]
async fn rm_of_a_bound_account_lists_its_references_and_keeps_the_provider() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let (harness, provider) = synced(&rig).await?;
        harness
            .run(&["account", "bind", "work", "--workspace", "platform-backend"])
            .await?
            .ok()?;
        let before = harness.config_bytes()?;
        let ran = harness.run(&["account", "rm", "work"]).await?;
        failed(&ran, ExitClass::Failure, &["work", "platform-backend"])?;
        assert_eq!(harness.config_bytes()?, before);
        assert!(rig.gateway().provider(&provider).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn rm_checks_the_gateway_before_the_config_and_removes_both_when_nothing_uses_the_account()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let (harness, provider) = synced(&rig).await?;
        rig.launch(1).await?;
        let before = harness.config_bytes()?;
        let ran = harness.run(&["account", "rm", "work"]).await?;
        failed(&ran, ExitClass::Failure, &["attached"])?;
        assert_eq!(harness.config_bytes()?, before);
        assert!(
            harness
                .stored()
                .await?
                .account(&AccountName::parse("work")?)
                .is_some()
        );
        assert!(rig.gateway().provider(&provider).is_some());

        let name = rig.sandbox_name(1)?;
        rig.world.session.gateway().delete_sandbox(&name).await?;
        harness.run(&["account", "rm", "work"]).await?.ok()?;
        assert!(
            harness
                .stored()
                .await?
                .account(&AccountName::parse("work")?)
                .is_none()
        );
        assert!(rig.gateway().provider(&provider).is_none());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn rm_of_an_account_that_is_not_a_codex_login_never_connects_and_succeeds_with_the_gateway_down()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let mut harness = Harness::new(&world);
        harness.connector = FixedConnector::empty();
        let text = format!("{GATEWAY_CONFIG}\n{CONFIG}");
        harness.write_config(&text)?;
        harness.run(&["account", "default", "two"]).await?.ok()?;
        let mut expected = Config::parse(&text)?;
        expected.apply(ConfigEdit::SetDefaultAccount {
            account: AccountName::parse("two")?,
        })?;
        expected.apply(ConfigEdit::RemoveAccount(AccountName::parse("one")?))?;
        harness.run(&["account", "rm", "one"]).await?.ok()?;
        assert_eq!(rendered(&harness).await?, expected.render());

        let before = harness.config_bytes()?;
        let ran = harness.run(&["account", "rm", "four"]).await?;
        failed(&ran, ExitClass::Failure, &["fake-gateway", "unreachable"])?;
        assert_eq!(harness.config_bytes()?, before);
        Ok(())
    })
    .await
}
