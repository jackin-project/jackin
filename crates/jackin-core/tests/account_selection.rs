// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, AccountQuery, Agent, Config, MissingBinding, Provider, RoleName, SelectionError,
    SelectionRule, WorkspaceName,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn accounts(list: &[(&str, &str)]) -> String {
    let mut text = String::new();
    for (name, provider) in list {
        text.push_str(&format!(
            "[accounts.{name}]\nprovider = \"{provider}\"\ncredential = {{ source = \"env\", var = \"K\" }}\n"
        ));
    }
    text
}

fn selection_fixture() -> String {
    format!(
        "[defaults.accounts]\ncodex = \"dflt\"\n{}[roles.r1]\nsource.path = \"/r1\"\nbindings.codex = \"rolebound\"\n[roles.r0]\nsource.path = \"/r0\"\n[workspaces.w1]\npath = \"/w1\"\nbindings.codex = \"wsbound\"\n[workspaces.w0]\npath = \"/w0\"\n",
        accounts(&[
            ("flagged", "codex"),
            ("wsbound", "codex"),
            ("rolebound", "codex"),
            ("dflt", "codex"),
            ("claudeone", "claude"),
        ])
    )
}

fn choose(
    config: &Config,
    flag: Option<&str>,
    workspace: &str,
    role: &str,
) -> Result<Result<(String, SelectionRule), SelectionError>, Box<dyn std::error::Error>> {
    let flag = flag.map(AccountName::parse).transpose()?;
    let (workspace, role) = (WorkspaceName::parse(workspace)?, RoleName::parse(role)?);
    let query = AccountQuery {
        flag: flag.as_ref(),
        workspace: &workspace,
        role: &role,
        agent: Agent::Codex,
        provider: Provider::Codex,
    };
    Ok(config
        .select_account(&query)
        .map(|chosen| (chosen.account.to_string(), chosen.via)))
}

#[test]
fn selection_follows_flag_workspace_role_default_then_only() -> TestResult {
    let config = Config::parse(&selection_fixture())?;
    for (flag, workspace, role, account, rule) in [
        (Some("flagged"), "w1", "r1", "flagged", SelectionRule::Flag),
        (None, "w1", "r1", "wsbound", SelectionRule::WorkspaceBinding),
        (None, "w0", "r1", "rolebound", SelectionRule::RoleBinding),
        (None, "w0", "r0", "dflt", SelectionRule::ProviderDefault),
    ] {
        let first = choose(&config, flag, workspace, role)?;
        assert_eq!(
            first.as_ref().ok(),
            Some(&(account.to_owned(), rule)),
            "{rule:?}"
        );
        assert_eq!(first.ok(), choose(&config, flag, workspace, role)?.ok());
    }
    let one = Config::parse(&accounts(&[("solo", "codex"), ("c1", "claude")]))?;
    assert_eq!(
        choose(&one, None, "any", "any")?.ok(),
        Some(("solo".to_owned(), SelectionRule::OnlyAccount))
    );
    let one_default = Config::parse(&format!(
        "[defaults.accounts]\ncodex = \"solo\"\n{}",
        accounts(&[("solo", "codex")])
    ))?;
    assert_eq!(
        choose(&one_default, None, "any", "any")?.ok(),
        Some(("solo".to_owned(), SelectionRule::ProviderDefault))
    );
    Ok(())
}

#[test]
fn selection_errors_name_the_missing_binding_and_the_bad_flag() -> TestResult {
    let many = Config::parse(&accounts(&[
        ("bravo", "codex"),
        ("alpha", "codex"),
        ("c1", "claude"),
    ]))?;
    let error = choose(&many, None, "w", "r")?.err().ok_or("many")?;
    assert!(matches!(
        &error,
        SelectionError::NoAccount {
            missing: MissingBinding::SeveralAccountsNoDefault { provider: Provider::Codex, accounts }
        } if accounts.iter().map(ToString::to_string).collect::<Vec<_>>() == ["alpha", "bravo"]
    ));
    let shown = error.to_string();
    for part in ["codex", "alpha", "bravo"] {
        assert!(shown.contains(part), "{shown}");
    }
    assert!(!shown.contains("Codex"), "{shown}");
    let none = Config::parse(&accounts(&[("c1", "claude")]))?;
    let error = choose(&none, None, "w", "r")?.err().ok_or("none")?;
    assert!(matches!(
        &error,
        SelectionError::NoAccount {
            missing: MissingBinding::NoAccountForProvider {
                provider: Provider::Codex
            }
        }
    ));
    assert!(error.to_string().contains("codex"));
    let config = Config::parse(&selection_fixture())?;
    let error = choose(&config, Some("ghost"), "w1", "r1")?
        .err()
        .ok_or("ghost")?;
    assert!(matches!(&error, SelectionError::UnknownFlagAccount(name) if name.as_str() == "ghost"));
    let error = choose(&config, Some("claudeone"), "w1", "r1")?
        .err()
        .ok_or("claude")?;
    assert!(matches!(
        &error,
        SelectionError::FlagProviderMismatch { account, expected: Provider::Codex } if account.as_str() == "claudeone"
    ));
    Ok(())
}
