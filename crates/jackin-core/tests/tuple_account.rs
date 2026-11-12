// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{
    Account, AccountName, AccountQuery, Agent, CatalogError, Config, ConfigEdit, ConfigError,
    CredentialSource, EnvName, HostPath, Provider, RoleName, RoleSource, SelectionError,
    SelectionRule, TUPLES, WorkspaceName,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn accounts(list: &[(&str, &str)]) -> String {
    let mut text = String::new();
    for (name, provider) in list {
        text.push_str(&format!("[accounts.{name}]\nprovider = \"{provider}\"\ncredential = {{ source = \"env\", var = \"K\" }}\n"));
    }
    text
}

fn choose_for(
    config: &Config,
    flag: Option<&str>,
    workspace: &str,
    role: &str,
    agent: Agent,
    provider: Provider,
) -> Result<Result<(String, SelectionRule), SelectionError>, Box<dyn std::error::Error>> {
    let flag = flag.map(AccountName::parse).transpose()?;
    let (workspace, role) = (WorkspaceName::parse(workspace)?, RoleName::parse(role)?);
    let query = AccountQuery {
        flag: flag.as_ref(),
        workspace: &workspace,
        role: &role,
        agent,
        provider,
    };
    Ok(config
        .select_account(&query)
        .map(|c| (c.account.to_string(), c.via)))
}

fn choose(
    config: &Config,
    flag: Option<&str>,
    workspace: &str,
    role: &str,
) -> Result<Result<(String, SelectionRule), SelectionError>, Box<dyn std::error::Error>> {
    choose_for(config, flag, workspace, role, Agent::Omp, Provider::Kimi)
}

fn env_account(name: &str, provider: Provider) -> Result<Account, Box<dyn std::error::Error>> {
    Ok(Account::new(
        AccountName::parse(name)?,
        provider,
        CredentialSource::Env {
            var: EnvName::parse("K")?,
        },
    )?)
}

fn add_role(config: &mut Config, name: &str) -> Result<RoleName, Box<dyn std::error::Error>> {
    let name = RoleName::parse(name)?;
    config.apply(ConfigEdit::AddRole {
        name: name.clone(),
        source: RoleSource::Path(HostPath::parse(Path::new("/r"))?),
    })?;
    Ok(name)
}

#[test]
fn every_catalog_tuple_binds_by_agent_and_selects_the_matching_provider() -> TestResult {
    for tuple in TUPLES {
        let suffix = tuple.id.to_lowercase();
        let account_name = format!("acct-{suffix}");
        let role_name = format!("role-{suffix}");
        let mut config = Config::empty();
        config.apply(ConfigEdit::AddAccount(env_account(
            &account_name,
            tuple.provider,
        )?))?;
        let role = add_role(&mut config, &role_name)?;
        config.apply(ConfigEdit::BindRoleAccount {
            role: role.clone(),
            agent: tuple.agent,
            account: AccountName::parse(&account_name)?,
        })?;
        assert_eq!(
            config
                .role(&role)
                .and_then(|entry| entry.bindings.get(&tuple.agent))
                .map(AccountName::as_str),
            Some(account_name.as_str()),
            "{}",
            tuple.id
        );
        let rendered = config.render();
        assert!(
            rendered.contains(&format!(
                "bindings = {{ {} = \"{account_name}\" }}",
                tuple.agent.as_str()
            )),
            "{}: {rendered}",
            tuple.id
        );
        let parsed = Config::parse(&rendered)?;
        let selected = choose_for(
            &parsed,
            None,
            "any",
            &role_name,
            tuple.agent,
            tuple.provider,
        )??;
        assert_eq!(
            selected,
            (account_name, SelectionRule::RoleBinding),
            "{}",
            tuple.id
        );
    }
    Ok(())
}

#[test]
fn role_binding_rejects_unknown_accounts_and_tuples_without_writes() -> TestResult {
    let mut config = Config::empty();
    config.apply(ConfigEdit::AddAccount(env_account("zai", Provider::Zai)?))?;
    let role = add_role(&mut config, "sentinel")?;
    let before = config.clone();
    let unknown = config.apply(ConfigEdit::BindRoleAccount {
        role: role.clone(),
        agent: Agent::Omp,
        account: AccountName::parse("ghost")?,
    });
    assert!(matches!(
        unknown,
        Err(ConfigError::MissingReference {
            kind: "account",
            ..
        })
    ));
    assert_eq!(config, before);
    let mismatched = config.apply(ConfigEdit::BindRoleAccount {
        role: role.clone(),
        agent: Agent::GrokBuild,
        account: AccountName::parse("zai")?,
    });
    assert!(matches!(
        mismatched,
        Err(ConfigError::Catalog(CatalogError::UnknownTuple {
            agent: Agent::GrokBuild,
            provider: Provider::Zai,
        }))
    ));
    assert_eq!(config, before);
    let text = format!(
        "{}[roles.sentinel]\nsource.path = \"/r\"\nbindings.grok = \"zai\"\n",
        accounts(&[("zai", "zai")])
    );
    assert!(matches!(
        Config::parse(&text),
        Err(ConfigError::Catalog(CatalogError::UnknownTuple {
            agent: Agent::GrokBuild,
            provider: Provider::Zai,
        }))
    ));
    let provider_keyed = text.replace("bindings.grok", "bindings.zai");
    assert!(matches!(
        Config::parse(&provider_keyed),
        Err(ConfigError::UnknownField { key }) if key == "roles.sentinel.bindings.zai"
    ));
    Ok(())
}

#[test]
fn other_provider_queries_fall_through_and_unbind_removes_only_its_agent() -> TestResult {
    let fixture = format!(
        "[defaults.accounts]\nkimi = \"default\"\n{}[roles.r1]\nsource.path = \"/r1\"\nbindings.omp = \"rolebound\"\nbindings.claude = \"rolebound\"\nbindings.codex = \"rolebound\"\n",
        accounts(&[("rolebound", "zai"), ("default", "kimi")])
    );
    let mut config = Config::parse(&fixture)?;
    let selected = choose_for(&config, None, "any", "r1", Agent::Codex, Provider::Kimi)??;
    assert_eq!(
        selected,
        ("default".to_owned(), SelectionRule::ProviderDefault)
    );
    let only = Config::parse(&fixture.replace("[defaults.accounts]\nkimi = \"default\"\n", ""))?;
    let selected = choose_for(&only, None, "any", "r1", Agent::Codex, Provider::Kimi)??;
    assert_eq!(selected, ("default".to_owned(), SelectionRule::OnlyAccount));
    config.apply(ConfigEdit::UnbindRoleAccount {
        role: RoleName::parse("r1")?,
        agent: Agent::ClaudeCode,
    })?;
    let role = config.role(&RoleName::parse("r1")?).ok_or("role")?;
    assert!(role.bindings.contains_key(&Agent::Omp));
    assert!(!role.bindings.contains_key(&Agent::ClaudeCode));
    Ok(())
}

#[test]
fn omp_kimi_precedence() -> TestResult {
    let fixture = format!(
        "[defaults.accounts]\nkimi = \"dflt\"\n{}[roles.r1]\nsource.path = \"/r1\"\nbindings.omp = \"rolebound\"\n[roles.r0]\nsource.path = \"/r0\"\n[workspaces.w1]\npath = \"/w1\"\nbindings.kimi = \"wsbound\"\n[workspaces.w0]\npath = \"/w0\"\n",
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
