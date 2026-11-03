// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{
    Account, AccountName, AccountQuery, Agent, AllowedRoles, ChatgptAccountId, Config, ConfigEdit,
    ConfigError, ConfigReference, CredentialSource, EngineConfig, EnvName, EnvScope, EnvValueError,
    GatewayEntry, GatewayName, HostPath, PlainEnvValue, Platform, Provider, ReservedEnvName,
    RoleName, RoleSet, RoleSource, ScopedEnvName, SelectionRule, Workspace, WorkspaceName,
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

fn plain(text: &str) -> Fallible<PlainEnvValue> {
    Ok(PlainEnvValue::parse(text)?)
}

fn only(names: &[&str]) -> Fallible<AllowedRoles> {
    Ok(AllowedRoles::Only(
        RoleSet::new(
            names
                .iter()
                .map(|name| role(name))
                .collect::<Fallible<Vec<_>>>()?,
        )
        .ok_or("an empty role set")?,
    ))
}

fn env_rows() -> Fallible<Vec<Row>> {
    let set_env = |scope, text: &str| -> Fallible<ConfigEdit> {
        Ok(ConfigEdit::SetEnv {
            scope,
            name: EnvName::parse("REGION")?,
            value: Some(plain(text)?),
        })
    };
    Ok(vec![
        row(
            "set a global variable",
            set_env(EnvScope::Global, "eu")?,
            set(&["env", "REGION"], "\"eu\""),
        ),
        row(
            "set a workspace variable",
            set_env(EnvScope::Workspace(ws("proj")?), "us")?,
            set(&["workspaces", "proj", "env", "REGION"], "\"us\""),
        ),
        row(
            "set a workspace and role variable",
            set_env(EnvScope::WorkspaceRole(ws("proj")?, role("smith")?), "ap")?,
            set(
                &["workspaces", "proj", "role_env", "smith", "REGION"],
                "\"ap\"",
            ),
        ),
        row(
            "allow two roles",
            ConfigEdit::SetWorkspaceRoles {
                workspace: ws("other")?,
                roles: only(&["sentinel", "smith"])?,
            },
            set(
                &["workspaces", "other", "roles"],
                "[\"sentinel\", \"smith\"]",
            ),
        ),
        row(
            "allow every role again",
            ConfigEdit::SetWorkspaceRoles {
                workspace: ws("proj")?,
                roles: AllowedRoles::All,
            },
            unset(&[]),
        ),
    ])
}

fn entry_rows() -> Fallible<Vec<Row>> {
    let new_workspace = Workspace {
        default_role: Some(role("smith")?),
        bindings: [(Provider::Claude, acct("spare")?)].into(),
        ..Workspace::new(hp("/p/ws3")?, true)
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
    rows.extend(env_rows()?);
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
        default_role: default_role.map(role).transpose()?,
        bindings: bindings
            .into_iter()
            .map(|(provider, name)| Ok((provider, acct(name)?)))
            .collect::<Fallible<_>>()?,
        ..Workspace::new(hp("/p/new")?, false)
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

fn env_rejected_rows() -> Fallible<Vec<RejectedRow>> {
    let var = |text: &str| EnvName::parse(text);
    Ok(vec![
        (
            "a reserved global variable",
            ConfigEdit::SetEnv {
                scope: EnvScope::Global,
                name: var("DOCKER_HOST")?,
                value: Some(plain("x")?),
            },
            "invalid:env.DOCKER_HOST".to_owned(),
        ),
        (
            "a reserved workspace variable",
            ConfigEdit::SetEnv {
                scope: EnvScope::Workspace(ws("proj")?),
                name: var("DOCKER_HOST")?,
                value: Some(plain("x")?),
            },
            "invalid:workspaces.proj.env.DOCKER_HOST".to_owned(),
        ),
        (
            "a reserved workspace role variable",
            ConfigEdit::SetEnv {
                scope: EnvScope::WorkspaceRole(ws("proj")?, role("smith")?),
                name: var("DOCKER_HOST")?,
                value: Some(plain("x")?),
            },
            "invalid:workspaces.proj.role_env.smith.DOCKER_HOST".to_owned(),
        ),
        (
            "unsetting a reserved workspace role variable",
            ConfigEdit::SetEnv {
                scope: EnvScope::WorkspaceRole(ws("proj")?, role("smith")?),
                name: var("PATH")?,
                value: None,
            },
            "invalid:workspaces.proj.role_env.smith.PATH".to_owned(),
        ),
        (
            "unsetting a reserved variable",
            ConfigEdit::SetEnv {
                scope: EnvScope::Workspace(ws("proj")?),
                name: var("JACKIN_LAUNCH_ID")?,
                value: None,
            },
            "invalid:workspaces.proj.env.JACKIN_LAUNCH_ID".to_owned(),
        ),
        (
            "a variable of a ghost workspace",
            ConfigEdit::SetEnv {
                scope: EnvScope::Workspace(ws("ghost-ws")?),
                name: var("REGION")?,
                value: Some(plain("x")?),
            },
            "reference:workspace:ghost-ws".to_owned(),
        ),
        (
            "a variable of a ghost role in a workspace",
            ConfigEdit::SetEnv {
                scope: EnvScope::WorkspaceRole(ws("proj")?, role("ghost-role")?),
                name: var("REGION")?,
                value: Some(plain("x")?),
            },
            "reference:role:ghost-role".to_owned(),
        ),
        (
            "a variable of a role in a ghost workspace",
            ConfigEdit::SetEnv {
                scope: EnvScope::WorkspaceRole(ws("ghost-ws")?, role("smith")?),
                name: var("REGION")?,
                value: Some(plain("x")?),
            },
            "reference:workspace:ghost-ws".to_owned(),
        ),
        (
            "allowed roles naming a ghost role",
            ConfigEdit::SetWorkspaceRoles {
                workspace: ws("other")?,
                roles: only(&["sentinel", "ghost"])?,
            },
            "reference:role:ghost".to_owned(),
        ),
        (
            "allowed roles of a ghost workspace",
            ConfigEdit::SetWorkspaceRoles {
                workspace: ws("ghost-ws")?,
                roles: only(&["sentinel"])?,
            },
            "reference:workspace:ghost-ws".to_owned(),
        ),
        (
            "allowed roles that leave out the default role",
            ConfigEdit::SetWorkspaceRoles {
                workspace: ws("proj")?,
                roles: only(&["smith"])?,
            },
            "invalid:workspaces.proj.default_role".to_owned(),
        ),
    ])
}

fn rejected_rows() -> Fallible<Vec<RejectedRow>> {
    let mut rows = env_rejected_rows()?;
    rows.extend(duplicate_rows()?);
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

fn var(name: &str) -> Fallible<EnvName> {
    Ok(EnvName::parse(name)?)
}

#[test]
fn env_edits_replace_unset_and_drop_an_emptied_role_table() -> TestResult {
    let mut config = Config::parse(BASE)?;
    let sentinel = role("sentinel")?;
    let scopes = [
        EnvScope::Global,
        EnvScope::Workspace(ws("proj")?),
        EnvScope::WorkspaceRole(ws("proj")?, sentinel.clone()),
    ];
    for (scope, text) in scopes.iter().zip(["g", "w", "r"]) {
        for value in [text, "replaced"] {
            config.apply(ConfigEdit::SetEnv {
                scope: scope.clone(),
                name: var("REGION")?,
                value: Some(plain(value)?),
            })?;
        }
    }
    let proj = config.workspace(&ws("proj")?).ok_or("proj")?;
    assert_eq!(config.env().len(), 1);
    assert_eq!(proj.env.len(), 1);
    assert_eq!(
        proj.role_env
            .get(&sentinel)
            .map(std::collections::BTreeMap::len),
        Some(1)
    );
    for table in [config.env(), &proj.env, &proj.role_env[&sentinel]] {
        assert_eq!(table[&var("REGION")?].as_str(), "replaced");
    }
    let rendered = config.render();
    assert!(
        rendered.contains("[env]\nREGION = \"replaced\"\n"),
        "{rendered}"
    );
    assert_eq!(Config::parse(&rendered)?, config);

    for scope in scopes {
        config.apply(ConfigEdit::SetEnv {
            scope,
            name: var("REGION")?,
            value: None,
        })?;
    }
    let proj = config.workspace(&ws("proj")?).ok_or("proj")?;
    assert!(config.env().is_empty() && proj.env.is_empty());
    assert!(
        proj.role_env.is_empty(),
        "an emptied role table stays behind"
    );
    assert_eq!(config.render(), Config::parse(BASE)?.render());
    Ok(())
}

#[test]
fn moving_a_variable_between_scopes_is_an_unset_and_a_set_in_one_edit_list() -> TestResult {
    let mut config = Config::parse(BASE)?;
    let from = EnvScope::Workspace(ws("proj")?);
    let to = EnvScope::WorkspaceRole(ws("proj")?, role("sentinel")?);
    config.apply(ConfigEdit::SetEnv {
        scope: from.clone(),
        name: var("MODE")?,
        value: Some(plain("fast")?),
    })?;
    for edit in [
        ConfigEdit::SetEnv {
            scope: from,
            name: var("MODE")?,
            value: None,
        },
        ConfigEdit::SetEnv {
            scope: to,
            name: var("MODE")?,
            value: Some(plain("fast")?),
        },
    ] {
        config.apply(edit)?;
    }
    let proj = config.workspace(&ws("proj")?).ok_or("proj")?;
    assert!(proj.env.is_empty());
    assert_eq!(
        proj.role_env[&role("sentinel")?][&var("MODE")?].as_str(),
        "fast"
    );
    Ok(())
}

#[test]
fn a_role_that_a_workspace_allows_or_gives_variables_cannot_be_removed() -> TestResult {
    let mut config = Config::parse(BASE)?;
    config.apply(ConfigEdit::SetWorkspaceRoles {
        workspace: ws("other")?,
        roles: only(&["smith"])?,
    })?;
    let blocked = config.clone().apply(ConfigEdit::RemoveRole(role("smith")?));
    assert!(
        matches!(&blocked, Err(ConfigError::RoleInUse { by, .. }) if *by == [ConfigReference::WorkspaceRoles(ws("other")?)]),
        "{blocked:?}"
    );
    config.apply(ConfigEdit::SetWorkspaceRoles {
        workspace: ws("other")?,
        roles: AllowedRoles::All,
    })?;
    config.apply(ConfigEdit::SetEnv {
        scope: EnvScope::WorkspaceRole(ws("other")?, role("smith")?),
        name: var("MODE")?,
        value: Some(plain("fast")?),
    })?;
    let blocked = config.clone().apply(ConfigEdit::RemoveRole(role("smith")?));
    assert!(
        matches!(&blocked, Err(ConfigError::RoleInUse { by, .. }) if *by == [ConfigReference::WorkspaceRoleEnv(ws("other")?)]),
        "{blocked:?}"
    );
    config.apply(ConfigEdit::SetEnv {
        scope: EnvScope::WorkspaceRole(ws("other")?, role("smith")?),
        name: var("MODE")?,
        value: None,
    })?;
    config.apply(ConfigEdit::RemoveRole(role("smith")?))?;
    Ok(())
}

#[test]
fn the_default_role_must_stay_inside_the_allowed_roles() -> TestResult {
    let mut config = Config::parse(BASE)?;
    config.apply(ConfigEdit::SetWorkspaceDefaultRole {
        workspace: ws("proj")?,
        role: None,
    })?;
    config.apply(ConfigEdit::SetWorkspaceRoles {
        workspace: ws("proj")?,
        roles: only(&["sentinel"])?,
    })?;
    let outside = config.clone().apply(ConfigEdit::SetWorkspaceDefaultRole {
        workspace: ws("proj")?,
        role: Some(role("smith")?),
    });
    assert!(
        matches!(&outside, Err(ConfigError::InvalidValue { key, .. }) if key == "workspaces.proj.default_role"),
        "{outside:?}"
    );
    config.apply(ConfigEdit::SetWorkspaceDefaultRole {
        workspace: ws("proj")?,
        role: Some(role("sentinel")?),
    })?;
    let mut workspace = Workspace::new(hp("/p/new")?, false);
    workspace.roles = only(&["sentinel"])?;
    workspace.default_role = Some(role("smith")?);
    let added = config.apply(ConfigEdit::AddWorkspace {
        name: ws("new")?,
        workspace,
    });
    assert!(
        matches!(&added, Err(ConfigError::InvalidValue { key, .. }) if key == "workspaces.new.default_role"),
        "{added:?}"
    );
    Ok(())
}

#[test]
fn a_plain_env_value_refuses_a_1password_reference_and_control_characters() -> TestResult {
    for (text, expected) in [
        ("", Ok("")),
        ("hello world", Ok("hello world")),
        ("a=b;c", Ok("a=b;c")),
        ("see op://not-at-start", Ok("see op://not-at-start")),
        ("op://Dev/Item/field", Err(EnvValueError::OnePassword)),
        ("op://", Err(EnvValueError::OnePassword)),
        ("one\ntwo", Err(EnvValueError::Control)),
        ("nul\0byte", Err(EnvValueError::Control)),
    ] {
        let parsed = PlainEnvValue::parse(text);
        assert_eq!(
            parsed.as_ref().map(PlainEnvValue::as_str),
            expected.as_ref().map(|text| *text),
            "{text:?}"
        );
        assert_eq!(
            parsed.map(|value| value.to_string()),
            expected.map(str::to_owned)
        );
    }
    Ok(())
}

#[test]
fn a_scoped_env_name_is_never_a_name_jackin_reserves() -> TestResult {
    for reserved in [
        "PATH",
        "HOME",
        "JACKIN_LAUNCH_ID",
        "DOCKER_HOST",
        "BUILDX_BUILDER",
        "OPENSHELL_ENDPOINT",
        "CODEX_AUTH_ACCESS_TOKEN",
        "LD_PRELOAD",
        "OPENAI_API_KEY",
        "CODEX_HOME",
    ] {
        assert_eq!(
            ScopedEnvName::new(EnvName::parse(reserved)?),
            Err(ReservedEnvName),
            "{reserved}"
        );
    }
    for free in ["GITHUB_TOKEN", "path", "REGION", "HOMEPAGE", "MY_PATH"] {
        let name = ScopedEnvName::new(EnvName::parse(free)?)?;
        assert_eq!(name.as_str(), free);
    }
    Ok(())
}

#[test]
fn replacing_a_credential_keeps_the_bindings_and_never_creates_a_default() -> TestResult {
    let mut config = Config::parse(&BASE.replace("claude = \"personal\"\n", ""))?;
    assert_eq!(config.default_account(Provider::Claude), None);
    config.apply(ConfigEdit::SetAccountCredential {
        account: acct("personal")?,
        credential: CredentialSource::File {
            path: hp("/keys/personal")?,
        },
    })?;
    assert_eq!(config.default_account(Provider::Claude), None);
    assert_eq!(
        config.default_account(Provider::Codex),
        Some(&acct("work")?)
    );
    let bound = config
        .workspace(&ws("proj")?)
        .and_then(|workspace| workspace.bindings.get(&Provider::Claude));
    assert_eq!(bound, Some(&acct("personal")?));
    Ok(())
}

#[test]
fn a_plain_env_value_refuses_a_credential_reference_in_any_case_after_leading_space() -> TestResult
{
    let refused = [
        ("op://v/i/f", EnvValueError::OnePassword),
        (" op://v/i/f", EnvValueError::OnePassword),
        ("\t Op://x", EnvValueError::OnePassword),
        ("OP://V/I/F", EnvValueError::OnePassword),
        ("\u{feff}op://x", EnvValueError::OnePassword),
        (" \u{feff} \u{feff}op://x", EnvValueError::OnePassword),
        ("openshell:resolve:env:X", EnvValueError::Placeholder),
        (" OPENSHELL:RESOLVE:x", EnvValueError::Placeholder),
        (
            "\u{feff}openshell:resolve:env:X",
            EnvValueError::Placeholder,
        ),
    ];
    for (text, error) in refused {
        assert_eq!(PlainEnvValue::parse(text), Err(error), "{text:?}");
    }
    for text in [
        "ghp_abc",
        "sk-live-1",
        "-----BEGIN PRIVATE KEY-----",
        "\"op://x\"",
        "see op://x",
        "https://op.example",
        "o",
        "é",
        "",
        "openshell:resolve",
        "abcdé",
        "openshell:resolveé",
    ] {
        assert_eq!(
            PlainEnvValue::parse(text)
                .as_ref()
                .map(PlainEnvValue::as_str),
            Ok(text),
            "{text:?}"
        );
    }
    Ok(())
}
