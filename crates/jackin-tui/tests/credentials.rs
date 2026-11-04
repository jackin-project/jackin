// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, ConfigEdit, CredentialSource, EnvName, HostPath, OpReference, Provider,
};
use jackin_testkit::{
    ACCESS, ACCOUNT_ID, ConsoleRig, ProcessReply, REFRESH, Rig, ScriptedProgram, TestResult, guard,
};

const ENV_VALUE: &str = "ENV-SECRET-5b1d02";
const OP_VALUE: &str = "OP-SECRET-91ac3e";
const FILE_VALUE: &str = "FILE-SECRET-e47f60";
const OP_REFERENCE: &str = "op://Dev/OpenAI/credential";

fn fakes() -> [&'static str; 6] {
    [ACCESS, REFRESH, ACCOUNT_ID, ENV_VALUE, OP_VALUE, FILE_VALUE]
}

fn leaks(surface: &str, text: &str) -> Vec<String> {
    fakes()
        .into_iter()
        .filter(|fake| text.contains(fake))
        .map(|fake| format!("{fake} appears in {surface}"))
        .collect()
}

#[tokio::test]
async fn no_account_kind_puts_a_credential_value_into_a_projected_field_or_a_screen() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        rig.world.seed_all(&rig.config).await?;
        let key_file = rig.home.as_path().join("api.key");
        std::fs::write(&key_file, FILE_VALUE)?;
        rig.world.runner.scripted().expect(
            ScriptedProgram::Op,
            &["read", OP_REFERENCE],
            ProcessReply::exited(0, OP_VALUE.as_bytes(), b""),
        );
        let extra = [
            (
                "envy",
                CredentialSource::Env {
                    var: EnvName::parse("ENVY_KEY")?,
                },
            ),
            (
                "vault",
                CredentialSource::OnePassword {
                    reference: OpReference::parse(OP_REFERENCE)?,
                    account: None,
                },
            ),
            (
                "keyfile",
                CredentialSource::File {
                    path: HostPath::parse(&key_file)?,
                },
            ),
        ];
        for (name, source) in extra {
            let account = Account::new(AccountName::parse(name)?, Provider::Codex, source)?;
            rig.world
                .config
                .edit(vec![ConfigEdit::AddAccount(account)])
                .await?;
        }
        let mut console = ConsoleRig::builder(&rig.world)
            .env("ENVY_KEY", ENV_VALUE)
            .build()
            .await?;
        console.skip_intro();

        let registry = &console.console().app().world.accounts;
        assert_eq!(registry.accounts.len(), 4);
        let mut found = Vec::new();
        for account in &registry.accounts {
            found.extend(leaks(
                &format!("account {}", account.display_name),
                &format!("{account:#?}"),
            ));
        }
        found.extend(leaks("the registry", &format!("{registry:#?}")));
        console.keys("u")?;
        let usage = console.screen();
        for name in ["work", "envy", "vault", "keyfile"] {
            assert!(usage.contains(name), "the usage page lists {name}: {usage}");
        }
        found.extend(leaks("the usage page", &usage));
        for keys in ["esc a", "esc"] {
            console.keys(keys)?;
            found.extend(leaks(
                &format!("the screen after {keys}"),
                &console.screen(),
            ));
        }
        assert!(found.is_empty(), "{found:#?}");
        Ok(())
    })
    .await
}
