// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::{AccountLs, ExitClass};
use jackin_core::{AccountName, Agent, Config, Provider, ProviderName, RoleName, WorkspaceName};
use jackin_testkit::{Rig, TestResult, World, guard};
use tokio_util::sync::CancellationToken;

use super::support::{Failure, Harness, failed, keys, names};

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
            config.apply(jackin_core::ConfigEdit::SetDefaultAccount {
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
        let value = ran.value()?;
        assert_eq!(keys(&value)?, names(&["schema", "accounts"]));
        assert_eq!(value["schema"], 1);
        let rows = value["accounts"].as_array().ok_or("no rows")?;
        assert_eq!(rows.len(), 3);
        for row in rows {
            assert_eq!(
                keys(row)?,
                names(&["name", "provider", "credential", "default"])
            );
        }
        let typed: AccountLs = ran.json()?;
        let kinds: Vec<&str> = typed
            .accounts
            .iter()
            .map(|row| row.credential.as_str())
            .collect();
        assert_eq!(kinds, ["env", "file", "op"]);
        assert!(typed.accounts[0].default && !typed.accounts[2].default);
        for secret in ["HOST_SECRET_VAR", "vault-name", "/secret/dir"] {
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
