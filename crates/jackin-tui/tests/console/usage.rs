// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Account, AccountName, ConfigEdit, CredentialSource, EnvName, Provider};
use jackin_testkit::{ConsoleRig, Rig, TestResult, guard};

#[tokio::test]
async fn usage_page_lists_accounts_with_no_windows() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.world.seed_all(&rig.config).await?;
        let other = Account::new(
            AccountName::parse("spare")?,
            Provider::Codex,
            CredentialSource::Env {
                var: EnvName::parse("SPARE_KEY")?,
            },
        )?;
        rig.world
            .config
            .edit(
                vec![ConfigEdit::AddAccount(other)],
                &tokio_util::sync::CancellationToken::new(),
            )
            .await?;
        let mut console = ConsoleRig::new(&rig.world).await?;
        console.skip_intro();
        let world = &console.console().app().world;
        assert_eq!(world.accounts.accounts.len(), 2);
        assert!(
            world
                .accounts
                .accounts
                .iter()
                .all(|a| a.usage.windows.is_empty()),
            "no usage window is invented"
        );
        console.keys("u")?;
        let screen = console.screen();
        assert!(screen.contains("work"), "{screen}");
        assert!(screen.contains("spare"), "{screen}");
        Ok(())
    })
    .await
}
