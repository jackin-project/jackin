// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, AccountQuery, Agent, Config, Provider, RoleName, SelectionError, SelectionRule,
    WorkspaceName,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn accounts(list: &[(&str, &str)]) -> String {
    let mut text = String::new();
    for (name, provider) in list {
        text.push_str(&format!("[accounts.{name}]\nprovider = \"{provider}\"\ncredential = {{ source = \"env\", var = \"K\" }}\n"));
    }
    text
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
        agent: Agent::Omp,
        provider: Provider::Kimi,
    };
    Ok(config
        .select_account(&query)
        .map(|c| (c.account.to_string(), c.via)))
}

#[test]
fn omp_kimi_precedence() -> TestResult {
    let fixture = format!(
        "[defaults.accounts]\nkimi = \"dflt\"\n{}[roles.r1]\nsource.path = \"/r1\"\nbindings.kimi = \"rolebound\"\n[roles.r0]\nsource.path = \"/r0\"\n[workspaces.w1]\npath = \"/w1\"\nbindings.kimi = \"wsbound\"\n[workspaces.w0]\npath = \"/w0\"\n",
        accounts(&[
            ("flagged", "kimi"),
            ("wsbound", "kimi"),
            ("rolebound", "kimi"),
            ("dflt", "kimi"),
            ("other", "zai")
        ])
    );
    let config = Config::parse(&fixture)?;
    for (flag, ws, role, want, rule) in [
        (Some("flagged"), "w1", "r1", "flagged", SelectionRule::Flag),
        (None, "w1", "r1", "wsbound", SelectionRule::WorkspaceBinding),
        (None, "w0", "r1", "rolebound", SelectionRule::RoleBinding),
        (None, "w0", "r0", "dflt", SelectionRule::ProviderDefault),
    ] {
        assert_eq!(
            choose(&config, flag, ws, role)?.ok(),
            Some((want.to_owned(), rule)),
            "{rule:?}"
        );
    }
    let sole = Config::parse(&accounts(&[("solo", "kimi"), ("z1", "zai")]))?;
    assert_eq!(
        choose(&sole, None, "any", "any")?.ok(),
        Some(("solo".to_owned(), SelectionRule::OnlyAccount))
    );
    let err = choose(&config, Some("other"), "w1", "r1")?
        .err()
        .ok_or("err")?;
    assert!(
        matches!(
            &err,
            SelectionError::FlagProviderMismatch {
                expected: Provider::Kimi,
                ..
            }
        ),
        "{err}"
    );
    Ok(())
}
