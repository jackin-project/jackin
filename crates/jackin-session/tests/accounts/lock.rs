// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::Path;
use std::time::Duration;

use jackin_core::{
    Account, AccountName, Agent, ChatgptAccountId, ConfigEdit, CredentialSource, FileLock,
    HostPath, Provider, ProviderName, Secret,
};
use jackin_openshell::{Gateway, ProviderMaterial, ProviderProfile, ProviderSpec};
use jackin_session::LaunchOutcome;
use jackin_testkit::{ACCOUNT_ID, REFRESH, Rig, TestResult, guard_within};
use tokio_util::sync::CancellationToken;

const ROTATED: &str = "REFRESH-ROTATED-1d5e90";
const WITHIN: Duration = Duration::from_secs(20);

fn polling(rig: &Rig) -> bool {
    rig.world
        .clock
        .slept()
        .contains(&Duration::from_millis(100))
}

async fn hold(rig: &Rig, account: &str) -> Result<FileLock, Box<dyn Error>> {
    rig.world.fs.create_dir_all(rig.world.cache_dir()).await?;
    let lock = HostPath::resolve(
        Path::new(&format!("account-{account}.lock")),
        rig.world.cache_dir(),
    )?;
    Ok(rig.world.fs.lock(&lock).await?)
}

async fn peer_put(rig: &Rig, refresh: &str) -> TestResult {
    let spec = Agent::Codex.spec();
    let profile = ProviderProfile::parse(&spec.codex.openshell.chatgpt_profile)?;
    rig.gateway().ensure_profile(&profile).await?;
    rig.gateway()
        .put_provider(&ProviderSpec {
            name: ProviderName::parse("jackin-work-codex")?,
            profile: profile.id,
            keys: spec.codex.openshell.keys,
            material: ProviderMaterial::ChatgptLogin {
                access_token: Secret::new("ACCESS-PEER".to_owned()),
                refresh_token: Secret::new(refresh.to_owned()),
                client_id: "app_EMoamEEZ73f0CkXaXp7hrann".to_owned(),
                chatgpt_account_id: ChatgptAccountId::parse(ACCOUNT_ID)?,
            },
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn a_launch_waits_for_a_peer_that_syncs_the_account_and_keeps_the_rotated_token() -> TestResult
{
    guard_within(WITHIN, async {
        let rig = Rig::new().await?;
        let held = hold(&rig, "work").await?;
        let peer = async {
            while !polling(&rig) && rig.gateway().providers().is_empty() {
                tokio::task::yield_now().await;
            }
            assert!(rig.gateway().providers().is_empty());
            peer_put(&rig, REFRESH).await?;
            peer_put(&rig, ROTATED).await?;
            drop(held);
            Ok::<(), Box<dyn Error>>(())
        };
        let (outcome, peer) = tokio::join!(rig.launch(1), peer);
        peer?;
        assert!(matches!(outcome?, LaunchOutcome::Ready(_)));
        let record = rig
            .gateway()
            .provider(&ProviderName::parse("jackin-work-codex")?)
            .ok_or("no provider record")?;
        assert_eq!(record.refresh_token, ROTATED);
        drop(hold(&rig, "work").await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_lock_is_per_account_and_a_cancelled_wait_ends_the_launch() -> TestResult {
    guard_within(WITHIN, async {
        let rig = Rig::new().await?;
        let held = hold(&rig, "work").await?;
        let personal = Account::new(
            AccountName::parse("personal")?,
            Provider::Codex,
            CredentialSource::CodexLogin {
                home: rig.home.clone(),
                chatgpt_account_id: ChatgptAccountId::parse(ACCOUNT_ID)?,
            },
        )?;
        rig.world
            .config
            .edit(vec![ConfigEdit::AddAccount(personal.clone())])
            .await?;
        rig.world
            .session
            .sync_account(&personal, &CancellationToken::new())
            .await?;

        let token = CancellationToken::new();
        let canceller = async {
            while !polling(&rig) {
                tokio::task::yield_now().await;
            }
            token.cancel();
        };
        let (outcome, ()) = tokio::join!(rig.launch_with(1, token.clone()), canceller);
        assert!(matches!(outcome?, Ok(LaunchOutcome::Cancelled(_))));
        assert!(
            !rig.gateway()
                .providers()
                .contains(&ProviderName::parse("jackin-work-codex")?)
        );
        drop(held);
        Ok(())
    })
    .await
}
