// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{
    Account, AccountName, AccountQuery, Agent, ChatgptAccountId, Config, ConfigEdit, ConfigError,
    ConfigReference, CredentialSource, EngineConfig, EnvName, GatewayEntry, GatewayName, HostPath,
    Platform, Provider, RoleName, RoleSource, SelectionRule, Workspace, WorkspaceName,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

const BASE: &str = r#"default_gateway = "gw-a"

[gateways.gw-a]
openshell_workspace = "default"
builder = "orbstack"
platform = "linux/arm64"

[gateways.gw-b]
openshell_workspace = "alt"
builder = "default"
platform = "linux/amd64"

[defaults.accounts]
codex = "work"
claude = "personal"

[accounts.work]
provider = "codex"
credential = { source = "env", var = "K1" }

[accounts.ci]
provider = "codex"
credential = { source = "op", reference = "op://Dev/OpenAI/credential" }

[accounts.personal]
provider = "claude"
credential = { source = "env", var = "K2" }

[accounts.spare]
provider = "claude"
credential = { source = "file", path = "/keys/claude" }

[roles.sentinel]
source = { path = "/r/sentinel" }
bindings = { codex = "work" }

[roles.smith]
source = { git = "https://example.com/smith.git", rev = "v1" }

[workspaces.proj]
path = "/p/proj"
docker = false
default_role = "sentinel"
bindings = { codex = "work", claude = "personal" }

[workspaces.other]
path = "/p/other"
docker = false
"#;

fn hp(text: &str) -> Fallible<HostPath> {
    Ok(HostPath::parse(Path::new(text))?)
}

fn acct(name: &str) -> Fallible<AccountName> {
    Ok(AccountName::parse(name)?)
}

fn gw(name: &str) -> Fallible<GatewayName> {
    Ok(GatewayName::parse(name)?)
}

fn role(name: &str) -> Fallible<RoleName> {
    Ok(RoleName::parse(name)?)
}

fn ws(name: &str) -> Fallible<WorkspaceName> {
    Ok(WorkspaceName::parse(name)?)
}

fn model(text: &str) -> Fallible<toml::Table> {
    Ok(text.parse::<toml::Table>()?)
}

fn prune(table: toml::Table) -> toml::Table {
    table
        .into_iter()
        .filter_map(|(key, value)| match value {
            toml::Value::Table(inner) => {
                let inner = prune(inner);
                (!inner.is_empty()).then_some((key, toml::Value::Table(inner)))
            }
            other => Some((key, other)),
        })
        .collect()
}

fn value(text: &str) -> Fallible<toml::Value> {
    model(&format!("v = {text}"))?
        .remove("v")
        .ok_or_else(|| "value".into())
}

fn put(table: &mut toml::Table, path: &[&str], leaf: toml::Value) {
    match path {
        [last] => {
            table.insert((*last).to_owned(), leaf);
        }
        [head, rest @ ..] => {
            let entry = table
                .entry((*head).to_owned())
                .or_insert_with(|| toml::Value::Table(toml::Table::new()));
            if let toml::Value::Table(inner) = entry {
                put(inner, rest, leaf);
            }
        }
        [] => {}
    }
}

fn del(table: &mut toml::Table, path: &[&str]) {
    match path {
        [last] => {
            table.remove(*last);
        }
        [head, rest @ ..] => {
            if let Some(toml::Value::Table(inner)) = table.get_mut(*head) {
                del(inner, rest);
            }
        }
        [] => {}
    }
}

fn env_account(name: &str, provider: Provider, var: &str) -> Fallible<Account> {
    Ok(Account::new(
        acct(name)?,
        provider,
        CredentialSource::Env {
            var: EnvName::parse(var)?,
        },
    )?)
}

type Mutation = Box<dyn Fn(&mut toml::Table) -> Fallible<()>>;

struct Row {
    label: &'static str,
    edit: ConfigEdit,
    mutate: Mutation,
}

fn set(path: &'static [&'static str], text: &'static str) -> Mutation {
    Box::new(move |table| {
        put(table, path, value(text)?);
        Ok(())
    })
}

fn unset(paths: &'static [&'static [&'static str]]) -> Mutation {
    Box::new(move |table| {
        for path in paths {
            del(table, path);
        }
        Ok(())
    })
}

fn row(label: &'static str, edit: ConfigEdit, mutate: Mutation) -> Row {
    Row {
        label,
        edit,
        mutate,
    }
}

fn gateway_rows() -> Fallible<Vec<Row>> {
    let with_engine = GatewayEntry {
        openshell_workspace: "gc".parse()?,
        builder: "default".parse()?,
        platform: Platform::LinuxAmd64,
        docker_engine: Some(EngineConfig {
            port: std::num::NonZeroU16::new(18476).ok_or("port")?,
            client_certs: hp("/certs")?,
        }),
    };
    Ok(vec![
        row(
            "add gateway",
            ConfigEdit::AddGateway {
                name: gw("gw-c")?,
                entry: with_engine,
            },
            set(
                &["gateways", "gw-c"],
                "{ openshell_workspace = \"gc\", builder = \"default\", platform = \"linux/amd64\", docker_engine = { port = 18476, client_certs = \"/certs\" } }",
            ),
        ),
        row(
            "remove gateway",
            ConfigEdit::RemoveGateway(gw("gw-b")?),
            unset(&[&["gateways", "gw-b"]]),
        ),
        row(
            "remove the default gateway",
            ConfigEdit::RemoveGateway(gw("gw-a")?),
            unset(&[&["gateways", "gw-a"], &["default_gateway"]]),
        ),
        row(
            "set default gateway",
            ConfigEdit::SetDefaultGateway(Some(gw("gw-b")?)),
            set(&["default_gateway"], "\"gw-b\""),
        ),
        row(
            "clear default gateway",
            ConfigEdit::SetDefaultGateway(None),
            unset(&[&["default_gateway"]]),
        ),
    ])
}

fn account_rows() -> Fallible<Vec<Row>> {
    Ok(vec![
        row(
            "add account",
            ConfigEdit::AddAccount(env_account("extra", Provider::Grok, "XAI_KEY")?),
            set(
                &["accounts", "extra"],
                "{ provider = \"grok\", credential = { source = \"env\", var = \"XAI_KEY\" } }",
            ),
        ),
        row(
            "remove account",
            ConfigEdit::RemoveAccount(acct("ci")?),
            unset(&[&["accounts", "ci"]]),
        ),
        row(
            "replace an account credential",
            ConfigEdit::SetAccountCredential {
                account: acct("work")?,
                credential: CredentialSource::File {
                    path: hp("/keys/work")?,
                },
            },
            set(
                &["accounts", "work", "credential"],
                "{ source = \"file\", path = \"/keys/work\" }",
            ),
        ),
        row(
            "set default account",
            ConfigEdit::SetDefaultAccount {
                account: acct("ci")?,
            },
            set(&["defaults", "accounts", "codex"], "\"ci\""),
        ),
        row(
            "clear default account",
            ConfigEdit::ClearDefaultAccount {
                provider: Provider::Claude,
            },
            unset(&[&["defaults", "accounts", "claude"]]),
        ),
        row(
            "bind a workspace",
            ConfigEdit::BindWorkspaceAccount {
                workspace: ws("other")?,
                account: acct("ci")?,
            },
            set(&["workspaces", "other", "bindings", "codex"], "\"ci\""),
        ),
        row(
            "rebind a workspace",
            ConfigEdit::BindWorkspaceAccount {
                workspace: ws("proj")?,
                account: acct("ci")?,
            },
            set(&["workspaces", "proj", "bindings", "codex"], "\"ci\""),
        ),
        row(
            "unbind a workspace",
            ConfigEdit::UnbindWorkspaceAccount {
                workspace: ws("proj")?,
                provider: Provider::Claude,
            },
            unset(&[&["workspaces", "proj", "bindings", "claude"]]),
        ),
    ])
}

fn entry_rows() -> Fallible<Vec<Row>> {
    let new_workspace = Workspace {
        path: hp("/p/ws3")?,
        docker: true,
        default_role: Some(role("smith")?),
        bindings: [(Provider::Claude, acct("spare")?)].into(),
    };
    Ok(vec![
        row(
            "bind a role",
            ConfigEdit::BindRoleAccount {
                role: role("smith")?,
                agent: Agent::Codex,
                account: acct("ci")?,
            },
            set(&["roles", "smith", "bindings", "codex"], "\"ci\""),
        ),
        row(
            "unbind a role",
            ConfigEdit::UnbindRoleAccount {
                role: role("sentinel")?,
                agent: Agent::Codex,
            },
            unset(&[&["roles", "sentinel", "bindings", "codex"]]),
        ),
        row(
            "add role",
            ConfigEdit::AddRole {
                name: role("new")?,
                source: RoleSource::Path(hp("/r/new")?),
            },
            set(&["roles", "new"], "{ source = { path = \"/r/new\" } }"),
        ),
        row(
            "remove role",
            ConfigEdit::RemoveRole(role("smith")?),
            unset(&[&["roles", "smith"]]),
        ),
        row(
            "add workspace",
            ConfigEdit::AddWorkspace {
                name: ws("ws3")?,
                workspace: new_workspace,
            },
            set(
                &["workspaces", "ws3"],
                "{ path = \"/p/ws3\", docker = true, default_role = \"smith\", bindings = { claude = \"spare\" } }",
            ),
        ),
        row(
            "remove workspace",
            ConfigEdit::RemoveWorkspace(ws("other")?),
            unset(&[&["workspaces", "other"]]),
        ),
        row(
            "set docker",
            ConfigEdit::SetWorkspaceDocker {
                workspace: ws("other")?,
                docker: true,
            },
            set(&["workspaces", "other", "docker"], "true"),
        ),
        row(
            "set default role",
            ConfigEdit::SetWorkspaceDefaultRole {
                workspace: ws("other")?,
                role: Some(role("smith")?),
            },
            set(&["workspaces", "other", "default_role"], "\"smith\""),
        ),
        row(
            "clear default role",
            ConfigEdit::SetWorkspaceDefaultRole {
                workspace: ws("proj")?,
                role: None,
            },
            unset(&[&["workspaces", "proj", "default_role"]]),
        ),
    ])
}

#[test]
fn every_edit_changes_only_what_it_names() -> TestResult {
    let mut rows = gateway_rows()?;
    rows.extend(account_rows()?);
    rows.extend(entry_rows()?);
    for Row {
        label,
        edit,
        mutate,
    } in rows
    {
        let mut config = Config::parse(BASE)?;
        config.apply(edit)?;
        let mut expected = model(BASE)?;
        mutate(&mut expected)?;
        let rendered = config.render();
        assert_eq!(prune(model(&rendered)?), prune(expected), "{label}");
        assert_eq!(Config::parse(&rendered)?, config, "{label}");
    }
    Ok(())
}

fn label(error: &ConfigError) -> String {
    match error {
        ConfigError::Syntax { .. } => "syntax".to_owned(),
        ConfigError::UnknownField { key } => format!("unknown:{key}"),
        ConfigError::MissingField { key } => format!("missing:{key}"),
        ConfigError::InvalidValue { key, .. } => format!("invalid:{key}"),
        ConfigError::DuplicateName { kind, name } => format!("duplicate:{kind}:{name}"),
        ConfigError::MissingReference { kind, name } => format!("reference:{kind}:{name}"),
        ConfigError::ProviderMismatch {
            account,
            expected,
            found,
        } => {
            format!(
                "mismatch:{account}:{}:{}",
                expected.as_str(),
                found.as_str()
            )
        }
        ConfigError::CredentialNotAllowed { provider } => {
            format!("credential:{}", provider.as_str())
        }
        ConfigError::AccountInUse { account, by } => {
            format!("account-in-use:{account}:{}", by.len())
        }
        ConfigError::RoleInUse { role, by } => format!("role-in-use:{role}:{}", by.len()),
        ConfigError::NoGateway => "no-gateway".to_owned(),
        ConfigError::AmbiguousGateway { .. } => "ambiguous".to_owned(),
        ConfigError::UnknownGateway(name) => format!("unknown-gateway:{name}"),
        ConfigError::NoWorkspaceForPath(_) => "no-workspace".to_owned(),
        ConfigError::DuplicatePath { path, workspaces } => format!(
            "duplicate-path:{}:{}",
            path.as_path().display(),
            workspaces
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ),
        ConfigError::Fs(_) => "fs".to_owned(),
    }
}

type RejectedRow = (&'static str, ConfigEdit, String);

fn new_workspace(
    bindings: Vec<(Provider, &str)>,
    default_role: Option<&str>,
) -> Fallible<Workspace> {
    Ok(Workspace {
        path: hp("/p/new")?,
        docker: false,
        default_role: default_role.map(role).transpose()?,
        bindings: bindings
            .into_iter()
            .map(|(provider, name)| Ok((provider, acct(name)?)))
            .collect::<Fallible<_>>()?,
    })
}

fn workspace_at(path: &str) -> Fallible<Workspace> {
    Ok(Workspace {
        path: hp(path)?,
        ..new_workspace(vec![], None)?
    })
}

fn duplicate_path_rows() -> Fallible<Vec<RejectedRow>> {
    [
        ("path of proj", "/p/proj", "duplicate-path:/p/proj:new,proj"),
        (
            "path of proj with a trailing slash",
            "/p/proj/",
            "duplicate-path:/p/proj:new,proj",
        ),
        (
            "path of other",
            "/p/other",
            "duplicate-path:/p/other:new,other",
        ),
    ]
    .into_iter()
    .map(|(name, path, expected)| {
        Ok((
            name,
            ConfigEdit::AddWorkspace {
                name: ws("new")?,
                workspace: workspace_at(path)?,
            },
            expected.to_owned(),
        ))
    })
    .collect()
}

fn duplicate_rows() -> Fallible<Vec<RejectedRow>> {
    let entry = GatewayEntry {
        openshell_workspace: "x".parse()?,
        builder: "default".parse()?,
        platform: Platform::LinuxArm64,
        docker_engine: None,
    };
    Ok(vec![
        (
            "duplicate gateway",
            ConfigEdit::AddGateway {
                name: gw("gw-a")?,
                entry,
            },
            "duplicate:gateway:gw-a".to_owned(),
        ),
        (
            "duplicate account",
            ConfigEdit::AddAccount(env_account("work", Provider::Codex, "K")?),
            "duplicate:account:work".to_owned(),
        ),
        (
            "duplicate role",
            ConfigEdit::AddRole {
                name: role("sentinel")?,
                source: RoleSource::Path(hp("/x")?),
            },
            "duplicate:role:sentinel".to_owned(),
        ),
        (
            "duplicate workspace",
            ConfigEdit::AddWorkspace {
                name: ws("proj")?,
                workspace: new_workspace(vec![], None)?,
            },
            "duplicate:workspace:proj".to_owned(),
        ),
    ])
}

fn missing_rows() -> Fallible<Vec<RejectedRow>> {
    Ok(vec![
        (
            "remove ghost gateway",
            ConfigEdit::RemoveGateway(gw("ghost")?),
            "reference:gateway:ghost".to_owned(),
        ),
        (
            "default ghost gateway",
            ConfigEdit::SetDefaultGateway(Some(gw("ghost")?)),
            "reference:gateway:ghost".to_owned(),
        ),
        (
            "remove ghost account",
            ConfigEdit::RemoveAccount(acct("ghost")?),
            "reference:account:ghost".to_owned(),
        ),
        (
            "remove ghost role",
            ConfigEdit::RemoveRole(role("ghost")?),
            "reference:role:ghost".to_owned(),
        ),
        (
            "credential of a ghost account",
            ConfigEdit::SetAccountCredential {
                account: acct("ghost")?,
                credential: CredentialSource::File {
                    path: hp("/keys/ghost")?,
                },
            },
            "reference:account:ghost".to_owned(),
        ),
        (
            "remove ghost workspace",
            ConfigEdit::RemoveWorkspace(ws("ghost")?),
            "reference:workspace:ghost".to_owned(),
        ),
        (
            "default ghost account",
            ConfigEdit::SetDefaultAccount {
                account: acct("ghost")?,
            },
            "reference:account:ghost".to_owned(),
        ),
        (
            "bind ghost workspace",
            ConfigEdit::BindWorkspaceAccount {
                workspace: ws("ghost-ws")?,
                account: acct("work")?,
            },
            "reference:workspace:ghost-ws".to_owned(),
        ),
        (
            "bind ghost account",
            ConfigEdit::BindWorkspaceAccount {
                workspace: ws("proj")?,
                account: acct("ghost")?,
            },
            "reference:account:ghost".to_owned(),
        ),
        (
            "bind ghost role",
            ConfigEdit::BindRoleAccount {
                role: role("ghost-role")?,
                agent: Agent::Codex,
                account: acct("work")?,
            },
            "reference:role:ghost-role".to_owned(),
        ),
        (
            "unbind ghost role",
            ConfigEdit::UnbindRoleAccount {
                role: role("ghost-role")?,
                agent: Agent::Codex,
            },
            "reference:role:ghost-role".to_owned(),
        ),
        (
            "unbind ghost workspace",
            ConfigEdit::UnbindWorkspaceAccount {
                workspace: ws("ghost-ws")?,
                provider: Provider::Codex,
            },
            "reference:workspace:ghost-ws".to_owned(),
        ),
        (
            "docker on ghost workspace",
            ConfigEdit::SetWorkspaceDocker {
                workspace: ws("ghost")?,
                docker: true,
            },
            "reference:workspace:ghost".to_owned(),
        ),
        (
            "ghost default role",
            ConfigEdit::SetWorkspaceDefaultRole {
                workspace: ws("proj")?,
                role: Some(role("ghost")?),
            },
            "reference:role:ghost".to_owned(),
        ),
    ])
}

fn blocked_rows() -> Fallible<Vec<RejectedRow>> {
    Ok(vec![
        (
            "role bound to the wrong provider",
            ConfigEdit::BindRoleAccount {
                role: role("sentinel")?,
                agent: Agent::Codex,
                account: acct("personal")?,
            },
            "mismatch:personal:codex:claude".to_owned(),
        ),
        (
            "a ChatGPT login on a claude account",
            ConfigEdit::SetAccountCredential {
                account: acct("personal")?,
                credential: CredentialSource::CodexLogin {
                    home: hp("/codex")?,
                    chatgpt_account_id: ChatgptAccountId::parse("acct-1")?,
                },
            },
            "credential:claude".to_owned(),
        ),
        (
            "account in use by three",
            ConfigEdit::RemoveAccount(acct("work")?),
            "account-in-use:work:3".to_owned(),
        ),
        (
            "account in use by two",
            ConfigEdit::RemoveAccount(acct("personal")?),
            "account-in-use:personal:2".to_owned(),
        ),
        (
            "role in use",
            ConfigEdit::RemoveRole(role("sentinel")?),
            "role-in-use:sentinel:1".to_owned(),
        ),
        (
            "workspace with a bad second binding",
            ConfigEdit::AddWorkspace {
                name: ws("new")?,
                workspace: new_workspace(
                    vec![(Provider::Claude, "spare"), (Provider::Codex, "personal")],
                    None,
                )?,
            },
            "mismatch:personal:codex:claude".to_owned(),
        ),
        (
            "workspace with a ghost default role",
            ConfigEdit::AddWorkspace {
                name: ws("new")?,
                workspace: new_workspace(vec![], Some("ghost"))?,
            },
            "reference:role:ghost".to_owned(),
        ),
    ])
}

fn rejected_rows() -> Fallible<Vec<RejectedRow>> {
    let mut rows = duplicate_rows()?;
    rows.extend(duplicate_path_rows()?);
    rows.extend(missing_rows()?);
    rows.extend(blocked_rows()?);
    Ok(rows)
}

#[test]
fn rejected_edits_name_the_reason_and_leave_the_config_unchanged() -> TestResult {
    for (name, edit, expected) in rejected_rows()? {
        let before = Config::parse(BASE)?;
        let mut config = before.clone();
        let error = config.apply(edit).err().ok_or(name)?;
        assert_eq!(label(&error), expected, "{name}");
        assert_eq!(config, before, "{name}");
        assert_eq!(config.render(), before.render(), "{name}");
    }
    Ok(())
}

#[test]
fn a_workspace_path_is_free_again_after_its_workspace_is_removed() -> TestResult {
    let mut config = Config::parse(BASE)?;
    config.apply(ConfigEdit::RemoveWorkspace(ws("proj")?))?;
    config.apply(ConfigEdit::AddWorkspace {
        name: ws("new")?,
        workspace: workspace_at("/p/proj")?,
    })?;
    let (name, _) = config.workspace_for(&hp("/p/proj")?)?;
    assert_eq!(name, &ws("new")?);
    assert_eq!(Config::parse(&config.render())?, config);
    Ok(())
}

#[test]
fn account_in_use_lists_every_kind_of_reference() -> TestResult {
    let mut config = Config::parse(BASE)?;
    let Err(ConfigError::AccountInUse { by, .. }) =
        config.apply(ConfigEdit::RemoveAccount(acct("work")?))
    else {
        return Err("expected AccountInUse".into());
    };
    assert_eq!(by.len(), 3);
    assert!(by.contains(&ConfigReference::Default(Provider::Codex)));
    assert!(by.contains(&ConfigReference::WorkspaceBinding(ws("proj")?)));
    assert!(by.contains(&ConfigReference::RoleBinding(role("sentinel")?)));
    let Err(ConfigError::AccountInUse { by, .. }) =
        config.apply(ConfigEdit::RemoveAccount(acct("personal")?))
    else {
        return Err("expected AccountInUse".into());
    };
    assert_eq!(by.len(), 2);
    assert!(by.contains(&ConfigReference::Default(Provider::Claude)));
    assert!(by.contains(&ConfigReference::WorkspaceBinding(ws("proj")?)));
    let Err(ConfigError::RoleInUse { by, .. }) =
        config.apply(ConfigEdit::RemoveRole(role("sentinel")?))
    else {
        return Err("expected RoleInUse".into());
    };
    assert_eq!(by, [ConfigReference::WorkspaceDefaultRole(ws("proj")?)]);
    Ok(())
}

fn selected(config: &Config) -> Fallible<(String, SelectionRule)> {
    let (workspace, role) = (ws("proj")?, role("sentinel")?);
    let query = AccountQuery {
        flag: None,
        workspace: &workspace,
        role: &role,
        agent: Agent::Codex,
    };
    let chosen = config.select_account(&query)?;
    Ok((chosen.account.to_string(), chosen.via))
}

#[test]
fn removing_a_bound_account_with_its_bindings_clears_exactly_the_references_it_listed() -> TestResult
{
    let mut config = Config::parse(BASE)?;
    assert_eq!(
        selected(&config)?,
        ("work".to_owned(), SelectionRule::WorkspaceBinding)
    );
    config.apply(ConfigEdit::RemoveAccountClearingBindings(acct("work")?))?;
    let mut expected = model(BASE)?;
    for path in [
        &["accounts", "work"][..],
        &["defaults", "accounts", "codex"],
        &["workspaces", "proj", "bindings", "codex"],
        &["roles", "sentinel", "bindings", "codex"],
    ] {
        del(&mut expected, path);
    }
    assert_eq!(prune(model(&config.render())?), prune(expected));
    assert_eq!(
        selected(&config)?,
        ("ci".to_owned(), SelectionRule::OnlyAccount)
    );
    let mut strict = Config::parse(BASE)?;
    strict.apply(ConfigEdit::RemoveAccount(acct("ci")?))?;
    let mut clearing = Config::parse(BASE)?;
    clearing.apply(ConfigEdit::RemoveAccountClearingBindings(acct("ci")?))?;
    assert_eq!(strict, clearing);
    let error = Config::parse(BASE)?
        .apply(ConfigEdit::RemoveAccountClearingBindings(acct("ghost")?))
        .err()
        .ok_or("ghost")?;
    assert_eq!(label(&error), "reference:account:ghost");
    Ok(())
}
