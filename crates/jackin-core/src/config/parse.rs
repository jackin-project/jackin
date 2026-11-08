// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::num::NonZeroU16;
use std::path::Path;

use super::{
    Account, AllowedRoles, Config, ConfigError, CredentialSource, EnvTable, GatewayEntry,
    PlainEnvValue, RoleEntry, RoleSet, RoleSource, ScopedEnvName, Workspace,
};
use crate::docker::EngineConfig;
use crate::ids::{
    AccountName, BuilderName, ChatgptAccountId, EnvName, GatewayName, IdError, OpAccountId,
    OpReference, OpenShellWorkspace, RoleName, RoleRev, RoleUrl, WorkspaceName,
};
use crate::paths::{HostPath, PathError};
use crate::platform::Platform;
use crate::provider::Provider;
use crate::tomlkit::{Reader, parse_table};
use toml::Value;

const HOST_PATH: &str = "expected an absolute path";
const PARENT_SEGMENT: &str = "expected a path without .. segments";
const NOT_UTF8: &str = "expected a UTF-8 path";

fn host_path(text: &str) -> Result<HostPath, &'static str> {
    HostPath::parse(Path::new(text)).map_err(|error| match error {
        PathError::ParentSegment(_) => PARENT_SEGMENT,
        PathError::NotUtf8(_) => NOT_UTF8,
        _ => HOST_PATH,
    })
}

fn id_message(error: IdError) -> String {
    error.to_string()
}

fn static_message(message: &'static str) -> String {
    message.to_owned()
}

fn platform(text: &str) -> Result<Platform, &'static str> {
    match text {
        "linux/arm64" => Ok(Platform::LinuxArm64),
        "linux/amd64" => Ok(Platform::LinuxAmd64),
        _ => Err("expected linux/arm64 or linux/amd64"),
    }
}

fn provider(text: &str) -> Option<Provider> {
    Provider::ALL
        .into_iter()
        .find(|candidate| candidate.as_str() == text)
}

fn provider_value(text: &str) -> Result<Provider, String> {
    provider(text).ok_or_else(|| {
        let names: Vec<&str> = Provider::ALL.iter().map(|p| p.as_str()).collect();
        format!("expected one of {}", names.join(", "))
    })
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, ConfigError> {
        let table = parse_table(text).map_err(|problem| ConfigError::Syntax {
            line: problem.line,
            column: problem.column,
            message: problem.message,
        })?;
        let mut findings = Vec::new();
        let mut deferred = None;
        let mut config = Config::default();
        let mut root = Reader::new(String::new(), &table, &mut findings);
        config.default_gateway = root.parsed("default_gateway", GatewayName::parse, id_message);
        read_gateways(&mut root, &mut config.gateways);
        read_defaults(&mut root, &mut config.defaults);
        read_accounts(&mut root, &mut config.accounts, &mut deferred);
        read_roles(&mut root, &mut config.roles);
        if let Some(mut env) = root.table("env") {
            config.env = read_env(&mut env);
        }
        read_workspaces(&mut root, &mut config.workspaces);
        root.finish();
        if let Some(first) = findings.into_iter().next() {
            return Err(first.into());
        }
        if let Some(error) = deferred {
            return Err(error);
        }
        config.check_references()?;
        Ok(config)
    }
}

fn read_gateways(root: &mut Reader<'_, '_>, out: &mut BTreeMap<GatewayName, GatewayEntry>) {
    let Some(mut gateways) = root.table("gateways") else {
        return;
    };
    for (key, value) in gateways.entries() {
        let name = match GatewayName::parse(key) {
            Ok(name) => name,
            Err(error) => {
                gateways.report_invalid(key, &error.to_string());
                continue;
            }
        };
        let Some(mut entry) = gateways.entry_table(key, value) else {
            continue;
        };
        if let Some(parsed) = read_gateway(&mut entry) {
            out.insert(name, parsed);
        }
        entry.finish();
    }
}

fn read_gateway(entry: &mut Reader<'_, '_>) -> Option<GatewayEntry> {
    let openshell_workspace =
        entry.required_parsed("openshell_workspace", OpenShellWorkspace::parse, id_message);
    let builder = entry.required_parsed("builder", BuilderName::parse, id_message);
    let platform = entry.required_parsed("platform", platform, static_message);
    let docker_engine = read_engine(entry);
    Some(GatewayEntry {
        openshell_workspace: openshell_workspace?,
        builder: builder?,
        platform: platform?,
        docker_engine: docker_engine?,
    })
}

fn read_engine(entry: &mut Reader<'_, '_>) -> Option<Option<EngineConfig>> {
    let Some(mut engine) = entry.table("docker_engine") else {
        return Some(None);
    };
    let port = engine.required_integer("port").and_then(|number| {
        let port = u16::try_from(number).ok().and_then(NonZeroU16::new);
        let Some(port) = port else {
            engine.report_invalid("port", "expected an integer from 1 to 65535");
            return None;
        };
        if let Some(refusal) = EngineConfig::port_refusal(port) {
            engine.report_invalid("port", &refusal);
            return None;
        }
        Some(port)
    });
    let client_certs = engine.required_parsed("client_certs", host_path, static_message);
    engine.finish();
    Some(Some(EngineConfig {
        port: port?,
        client_certs: client_certs?,
    }))
}

fn read_binding_map<K: Ord>(
    owner: &mut Reader<'_, '_>,
    key: &'static str,
    lookup: impl Fn(&str) -> Option<K>,
) -> BTreeMap<K, AccountName> {
    let mut out = BTreeMap::new();
    let Some(mut map) = owner.table(key) else {
        return out;
    };
    for (name, value) in map.entries() {
        let Some(slot) = lookup(name) else {
            map.report_unknown(name);
            continue;
        };
        if let Some(account) = map.entry_parsed(name, value, AccountName::parse, id_message) {
            out.insert(slot, account);
        }
    }
    out
}

fn read_defaults(root: &mut Reader<'_, '_>, out: &mut BTreeMap<Provider, AccountName>) {
    let Some(mut defaults) = root.table("defaults") else {
        return;
    };
    *out = read_binding_map(&mut defaults, "accounts", provider);
    defaults.finish();
}

fn read_accounts(
    root: &mut Reader<'_, '_>,
    out: &mut BTreeMap<AccountName, Account>,
    deferred: &mut Option<ConfigError>,
) {
    let Some(mut accounts) = root.table("accounts") else {
        return;
    };
    for (key, value) in accounts.entries() {
        let name = match AccountName::parse(key) {
            Ok(name) => name,
            Err(error) => {
                accounts.report_invalid(key, &error.to_string());
                continue;
            }
        };
        let Some(mut entry) = accounts.entry_table(key, value) else {
            continue;
        };
        let provider = entry.required_parsed("provider", provider_value, |message| message);
        let credential = read_credential(&mut entry);
        entry.finish();
        if let (Some(provider), Some(credential)) = (provider, credential) {
            match Account::new(name.clone(), provider, credential) {
                Ok(account) => {
                    out.insert(name, account);
                }
                Err(error) => {
                    deferred.get_or_insert(error);
                }
            }
        }
    }
}

fn read_credential(account: &mut Reader<'_, '_>) -> Option<CredentialSource> {
    let mut credential = account.required_table("credential")?;
    let source = credential.required_string("source")?;
    let parsed = match source {
        "env" => credential
            .required_parsed("var", EnvName::parse, id_message)
            .map(|var| CredentialSource::Env { var }),
        "op" => {
            let reference = credential.required_parsed("reference", OpReference::parse, id_message);
            let named = credential.contains("account");
            let account = credential.parsed("account", OpAccountId::parse, id_message);
            if named && account.is_none() {
                None
            } else {
                reference.map(|reference| CredentialSource::OnePassword { reference, account })
            }
        }
        "omp-host" => {
            credential.finish();
            return Some(CredentialSource::OmpHost);
        }
        "file" => credential
            .required_parsed("path", host_path, static_message)
            .map(|path| CredentialSource::File { path }),
        "host-login" => credential
            .required_parsed("path", host_path, static_message)
            .map(|path| CredentialSource::HostLogin { path }),
        "codex-login" => {
            let home = credential.required_parsed("home", host_path, static_message);
            let id = credential.required_parsed(
                "chatgpt_account_id",
                ChatgptAccountId::parse,
                id_message,
            );
            Some(CredentialSource::CodexLogin {
                home: home?,
                chatgpt_account_id: id?,
            })
        }
        _ => {
            credential.report_invalid(
                "source",
                "expected env, op, file, omp-host, codex-login, or host-login",
            );
            None
        }
    };
    credential.finish();
    parsed
}

fn read_roles(root: &mut Reader<'_, '_>, out: &mut BTreeMap<RoleName, RoleEntry>) {
    let Some(mut roles) = root.table("roles") else {
        return;
    };
    for (key, value) in roles.entries() {
        let name = match RoleName::parse(key) {
            Ok(name) => name,
            Err(error) => {
                roles.report_invalid(key, &error.to_string());
                continue;
            }
        };
        let Some(mut entry) = roles.entry_table(key, value) else {
            continue;
        };
        let source = read_role_source(&mut entry);
        let bindings = read_binding_map(&mut entry, "bindings", provider);
        entry.finish();
        if let Some(source) = source {
            out.insert(name, RoleEntry { source, bindings });
        }
    }
}

fn read_role_source(role: &mut Reader<'_, '_>) -> Option<RoleSource> {
    let mut source = role.required_table("source")?;
    let parsed = match (source.contains("path"), source.contains("git")) {
        (true, false) => source
            .parsed("path", host_path, static_message)
            .map(RoleSource::Path),
        (false, true) => read_git_source(&mut source),
        (true, true) => {
            source.report_invalid_here("expected exactly one of path and git");
            None
        }
        (false, false) => {
            source.report_invalid_here("expected a path or a git URL");
            None
        }
    };
    source.finish();
    parsed
}

fn read_git_source(source: &mut Reader<'_, '_>) -> Option<RoleSource> {
    let url = source.parsed("git", RoleUrl::parse, id_message);
    let rev = source.parsed("rev", RoleRev::parse, id_message);
    Some(RoleSource::Git { url: url?, rev })
}

fn read_workspaces(root: &mut Reader<'_, '_>, out: &mut BTreeMap<WorkspaceName, Workspace>) {
    let Some(mut workspaces) = root.table("workspaces") else {
        return;
    };
    for (key, value) in workspaces.entries() {
        let name = match WorkspaceName::parse(key) {
            Ok(name) => name,
            Err(error) => {
                workspaces.report_invalid(key, &error.to_string());
                continue;
            }
        };
        let Some(mut entry) = workspaces.entry_table(key, value) else {
            continue;
        };
        let path = entry.required_parsed("path", host_path, static_message);
        let docker = entry.boolean("docker").unwrap_or(false);
        let default_role = entry.parsed("default_role", RoleName::parse, id_message);
        let bindings = read_binding_map(&mut entry, "bindings", provider);
        let roles = read_allowed_roles(&mut entry);
        let env = entry.table("env").map(|mut table| read_env(&mut table));
        let role_env = read_role_env(&mut entry);
        entry.finish();
        if let Some(path) = path {
            out.insert(
                name,
                Workspace {
                    path,
                    docker,
                    default_role,
                    bindings,
                    roles,
                    env: env.unwrap_or_default(),
                    role_env,
                },
            );
        }
    }
}

fn read_env(table: &mut Reader<'_, '_>) -> EnvTable {
    let mut out = EnvTable::new();
    for (key, value) in table.entries() {
        let name = match EnvName::parse(key) {
            Ok(name) => name,
            Err(error) => {
                table.report_invalid(key, &error.to_string());
                continue;
            }
        };
        let name = match ScopedEnvName::new(name) {
            Ok(name) => name,
            Err(reserved) => {
                table.report_invalid(key, &reserved.to_string());
                continue;
            }
        };
        if let Some(text) = table.entry_parsed(key, value, PlainEnvValue::parse, |e| e.to_string())
        {
            out.insert(name, text);
        }
    }
    out
}

fn read_role_env(entry: &mut Reader<'_, '_>) -> BTreeMap<RoleName, EnvTable> {
    let mut out = BTreeMap::new();
    let Some(mut roles) = entry.table("role_env") else {
        return out;
    };
    for (key, value) in roles.entries() {
        let role = match RoleName::parse(key) {
            Ok(role) => role,
            Err(error) => {
                roles.report_invalid(key, &error.to_string());
                continue;
            }
        };
        if let Some(mut table) = roles.entry_table(key, value) {
            out.insert(role, read_env(&mut table));
        }
    }
    out
}

fn read_allowed_roles(entry: &mut Reader<'_, '_>) -> AllowedRoles {
    const EXPECTED: &str = "expected a list of role names";
    let Some(value) = entry.value("roles") else {
        return AllowedRoles::All;
    };
    let Value::Array(items) = value else {
        entry.report_invalid("roles", EXPECTED);
        return AllowedRoles::All;
    };
    let mut roles = Vec::new();
    for item in items {
        match item.as_str().map(RoleName::parse) {
            Some(Ok(role)) => roles.push(role),
            Some(Err(error)) => entry.report_invalid("roles", &error.to_string()),
            None => entry.report_invalid("roles", EXPECTED),
        }
    }
    if let Some(set) = RoleSet::new(roles) {
        return AllowedRoles::Only(set);
    }
    entry.report_invalid(
        "roles",
        "expected at least one role; leave the key out to allow all roles",
    );
    AllowedRoles::All
}
