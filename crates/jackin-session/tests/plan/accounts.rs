// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, AccountSelection, MissingBinding, Provider, ProviderName, SelectionError,
    SelectionRule,
};
use jackin_session::PlanError;
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, built_image, guard, zai_account};

fn accounts(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| zai_account(name))
        .collect::<Vec<_>>()
        .join("\n")
}

struct Row {
    accounts: &'static [&'static str],
    defaults: &'static str,
    workspace: &'static str,
    role: &'static str,
    flag: Option<&'static str>,
    expected: &'static str,
    via: SelectionRule,
}

const ROWS: [Row; 5] = [
    Row {
        accounts: &["a-flag", "a-ws", "a-role", "a-def"],
        defaults: "zai = \"a-def\"\n",
        workspace: "zai = \"a-ws\"",
        role: "codex = \"a-role\"",
        flag: Some("a-flag"),
        expected: "a-flag",
        via: SelectionRule::Flag,
    },
    Row {
        accounts: &["a-flag", "a-ws", "a-role", "a-def"],
        defaults: "zai = \"a-def\"\n",
        workspace: "zai = \"a-ws\"",
        role: "codex = \"a-role\"",
        flag: None,
        expected: "a-ws",
        via: SelectionRule::WorkspaceBinding,
    },
    Row {
        accounts: &["a-role", "a-def"],
        defaults: "zai = \"a-def\"\n",
        workspace: "",
        role: "codex = \"a-role\"",
        flag: None,
        expected: "a-role",
        via: SelectionRule::RoleBinding,
    },
    Row {
        accounts: &["a-def", "a-other"],
        defaults: "zai = \"a-def\"\n",
        workspace: "",
        role: "",
        flag: None,
        expected: "a-def",
        via: SelectionRule::ProviderDefault,
    },
    Row {
        accounts: &["a-only"],
        defaults: "",
        workspace: "",
        role: "",
        flag: None,
        expected: "a-only",
        via: SelectionRule::OnlyAccount,
    },
];

async fn spec(
    row_accounts: &[&str],
    defaults: &str,
    workspace: &str,
    role: &str,
) -> Result<super::support::Setup, Box<dyn std::error::Error>> {
    let mut spec = Spec::new(RoleFixture::new("sentinel")?);
    spec.accounts = accounts(row_accounts);
    spec.defaults = defaults.to_owned();
    spec.workspace_bindings = workspace.to_owned();
    spec.role_bindings = role.to_owned();
    spec.build().await
}

#[tokio::test]
async fn the_plan_selects_the_account_by_the_d26_order_and_names_the_missing_binding() -> TestResult
{
    guard(async {
        for row in ROWS {
            let setup = spec(row.accounts, row.defaults, row.workspace, row.role).await?;
            let flag = row.flag.map(AccountName::parse).transpose()?;
            let plan = setup.plan(setup.request(1)?.account(flag))?;
            let account = AccountName::parse(row.expected)?;
            assert_eq!(
                plan.account(),
                &AccountSelection { account: account.clone(), via: row.via }
            );
            assert_eq!(plan.draft().labels().agents.lead().account, account);
            let created = plan.draft().complete(&built_image(&setup.role, '5')?, jackin_core::DockerLabel::Off);
            assert_eq!(
                created.providers(),
                [ProviderName::parse(&format!("jackin-{}-codex", row.expected))?]
            );
        }

        let several = spec(&["a-one", "a-two"], "", "", "").await?;
        let result = several.plan(several.request(1)?);
        let expected = [AccountName::parse("a-one")?, AccountName::parse("a-two")?];
        assert!(
            matches!(
                &result,
                Err(PlanError::Selection(SelectionError::NoAccount {
                    missing: MissingBinding::SeveralAccountsNoDefault { provider: Provider::Zai, accounts }
                })) if *accounts == expected
            ),
            "{:?}",
            result.err()
        );

        let none = spec(&[], "", "", "").await?;
        let result = none.plan(none.request(1)?);
        assert!(
            matches!(
                &result,
                Err(PlanError::Selection(SelectionError::NoAccount {
                    missing: MissingBinding::NoAccountForProvider { provider: Provider::Zai }
                }))
            ),
            "{:?}",
            result.err()
        );

        let unknown = several.plan(several.request(1)?.account(Some(AccountName::parse("ghost")?)));
        assert!(matches!(
            unknown,
            Err(PlanError::Selection(SelectionError::UnknownFlagAccount(_)))
        ));
        Ok(())
    })
    .await
}
