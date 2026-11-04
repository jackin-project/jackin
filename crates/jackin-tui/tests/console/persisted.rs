// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::domain::account::{Account, CredentialSource, DetectedKind, UNSAVED_ID};
use jackin_console::domain::agent::Provider;
use jackin_console::domain::vocabulary::{Rejection, SaveField};
use jackin_console::domain::workspace::EnvVar;
use jackin_core::{Config, ConfigEdit, EnvScope, RoleName, WorkspaceName};
use jackin_tui::persist::{self, PersistError, Resolved};
use jackin_tui::{Facts, Fleet, IdTable, Origins, ProjectInput, RoleChecks, RoleDocs, project};

const ACCOUNT_ID: &str = "3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f";
const GATEWAY: &str = "default_gateway = \"g\"\n\n[gateways.g]\nopenshell_workspace = \"default\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n";

fn login(name: &str) -> String {
    format!(
        "[accounts.{name}]\nprovider = \"codex\"\ncredential = {{ source = \"codex-login\", home = \"/home/dev/.codex\", chatgpt_account_id = \"{ACCOUNT_ID}\" }}\n"
    )
}

fn shapes() -> Vec<String> {
    let roles = "[roles.sentinel]\nsource = { path = \"/home/dev/role\" }\nbindings = { codex = \"work\" }\n\n[roles.other]\nsource = { path = \"/home/dev/other\" }\n";
    vec![
        GATEWAY.to_owned(),
        format!(
            "{GATEWAY}{}{}{roles}\n[env]\nREGION = \"eu\"\n\n[workspaces.alpha]\npath = \"/home/dev/alpha\"\ndocker = true\ndefault_role = \"sentinel\"\nroles = [\"sentinel\"]\nbindings = {{ codex = \"work\" }}\n\n[workspaces.alpha.env]\nREGION = \"us\"\n\n[workspaces.alpha.role_env.sentinel]\nREGION = \"ap\"\n\n[workspaces.beta]\npath = \"/home/dev/beta\"\n",
            login("work"),
            login("second")
        ),
    ]
}

fn snapshot(config: &Config) -> jackin_console::world::Snapshot {
    project(
        &ProjectInput {
            config,
            fleet: &Fleet::default(),
            facts: &Facts {
                version: "1.0.0".to_owned(),
                config_path: "/home/dev/.config/jackin/config.toml".to_owned(),
                home: "/home/dev".to_owned(),
                now_secs: 0,
            },
            roles: &RoleDocs::new(),
            checks: &RoleChecks::new(),
            origins: &Origins::new(),
        },
        &mut IdTable::new(),
    )
}

type Out = Result<(), Box<dyn std::error::Error>>;

#[test]
fn round_trip_emits_no_edit() -> Out {
    for text in shapes() {
        let config = Config::parse(&text)?;
        let shown = snapshot(&config);
        for workspace in &shown.workspaces {
            let edits =
                persist::workspace(&config, Some(workspace), workspace, &Resolved::default())?;
            assert_eq!(edits, Vec::new(), "workspace {}", workspace.name);
        }
        assert_eq!(persist::global(&shown.global, &shown.global)?, Vec::new());
        assert_eq!(
            persist::accounts(&shown.accounts, &shown.accounts, &Resolved::default())?,
            Vec::new()
        );
    }
    Ok(())
}

#[test]
fn unrepresentable_field_is_named() -> Out {
    let config = Config::parse(&shapes()[1])?;
    let shown = snapshot(&config);
    let base = shown.workspaces[0].clone();
    let mut payload = base.clone();
    payload.keep_awake = true;
    let result = persist::workspace(&config, Some(&base), &payload, &Resolved::default());
    assert!(
        matches!(
            result,
            Err(PersistError::Rejected(Rejection::CannotSave(
                SaveField::KeepAwake
            )))
        ),
        "{result:?}"
    );
    let mut global = shown.global.clone();
    global.coauthor_trailer = true;
    let result = persist::global(&shown.global, &global);
    assert!(
        matches!(
            result,
            Err(PersistError::Rejected(Rejection::CannotSave(
                SaveField::CoauthorTrailer
            )))
        ),
        "{result:?}"
    );
    let mut registry = shown.accounts.clone();
    registry.accounts[0].purpose = Some("work".to_owned());
    let result = persist::accounts(&shown.accounts, &registry, &Resolved::default());
    assert!(
        matches!(
            result,
            Err(PersistError::Rejected(Rejection::CannotSave(
                SaveField::Purpose
            )))
        ),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn env_goes_to_its_own_scope() -> Out {
    let config = Config::parse(&shapes()[1])?;
    let shown = snapshot(&config);
    let base = shown.workspaces[0].clone();
    let mut payload = base.clone();
    payload.env.retain(|v| v.key != "REGION");
    payload.env.push(EnvVar::plain("MODE", "fast"));
    payload
        .role_env
        .get_mut("sentinel")
        .ok_or("role env")?
        .push(EnvVar::plain("MODE", "slow"));
    let edits = persist::workspace(&config, Some(&base), &payload, &Resolved::default())?;
    let alpha = WorkspaceName::parse("alpha")?;
    let set = |scope: EnvScope,
               name: &str,
               value: Option<&str>|
     -> Result<ConfigEdit, Box<dyn std::error::Error>> {
        Ok(ConfigEdit::SetEnv {
            scope,
            name: jackin_core::EnvName::parse(name)?,
            value: value.map(jackin_core::PlainEnvValue::parse).transpose()?,
        })
    };
    let expected = vec![
        set(EnvScope::Workspace(alpha.clone()), "MODE", Some("fast"))?,
        set(EnvScope::Workspace(alpha.clone()), "REGION", None)?,
        set(
            EnvScope::WorkspaceRole(alpha, RoleName::parse("sentinel")?),
            "MODE",
            Some("slow"),
        )?,
    ];
    assert_eq!(edits, expected);
    Ok(())
}

#[test]
fn env_refuses_a_secret_reference() -> Out {
    let config = Config::parse(&shapes()[1])?;
    let shown = snapshot(&config);
    let base = shown.workspaces[0].clone();
    let mut payload = base.clone();
    payload
        .env
        .push(EnvVar::plain("TOKEN_X", "op://Dev/OpenAI/credential"));
    let result = persist::workspace(&config, Some(&base), &payload, &Resolved::default());
    assert!(matches!(result, Err(PersistError::Value(_))), "{result:?}");
    Ok(())
}

#[test]
fn accounts_diff_per_account() -> Out {
    let config = Config::parse(&shapes()[1])?;
    let shown = snapshot(&config);
    let mut registry = shown.accounts.clone();
    let second = registry
        .accounts
        .iter()
        .position(|a| a.id == "second")
        .ok_or("second")?;
    registry.accounts.remove(second);
    let work = registry
        .accounts
        .iter_mut()
        .find(|a| a.id == "work")
        .ok_or("work")?;
    work.default_for_provider = true;
    let edits = persist::accounts(&shown.accounts, &registry, &Resolved::default())?;
    assert_eq!(
        edits,
        vec![
            ConfigEdit::RemoveAccountClearingBindings(jackin_core::AccountName::parse("second")?),
            ConfigEdit::SetDefaultAccount {
                account: jackin_core::AccountName::parse("work")?
            },
        ]
    );
    assert_eq!(Provider::OpenAi, registry.accounts[0].provider);
    Ok(())
}

#[test]
fn the_unsaved_account_is_named_for_the_reply() -> Out {
    let config = Config::parse(&shapes()[1])?;
    let shown = snapshot(&config);
    assert_eq!(persist::added(&shown.accounts)?, None);
    let mut registry = shown.accounts.clone();
    registry.accounts.push(Account::registered(
        UNSAVED_ID,
        "spare",
        Provider::OpenAi,
        CredentialSource::HostEnv {
            var: "SPARE_KEY".to_owned(),
            detected: DetectedKind::ClaudeApiKeyEnv,
        },
    ));
    assert_eq!(persist::added(&registry)?, Some("spare".to_owned()));
    Ok(())
}

#[test]
fn a_rename_with_new_roles_and_a_new_default_applies_cleanly() -> Out {
    let mut config = Config::parse(&shapes()[1])?;
    let shown = snapshot(&config);
    let base = shown.workspaces[0].clone();
    assert_eq!(base.name, "alpha");
    let mut payload = base.clone();
    payload.name = "alpha2".to_owned();
    payload.roles.allowed =
        jackin_console::domain::workspace::AllowedRoles::Custom(vec!["other".to_owned()]);
    payload.roles.default = Some("other".to_owned());
    let edits = persist::workspace(&config, Some(&base), &payload, &Resolved::default())?;
    for edit in edits {
        config.apply(edit)?;
    }
    assert!(config.workspace(&WorkspaceName::parse("alpha")?).is_none());
    let moved = config
        .workspace(&WorkspaceName::parse("alpha2")?)
        .ok_or("alpha2")?;
    assert_eq!(moved.default_role, Some(RoleName::parse("other")?));
    assert!(
        matches!(&moved.roles, jackin_core::AllowedRoles::Only(set) if set.iter().map(ToString::to_string).collect::<Vec<_>>() == ["other"])
    );
    Ok(())
}

#[test]
fn a_rename_keeps_what_another_handle_wrote_after_the_read() -> Out {
    let stale = Config::parse(&shapes()[1])?;
    let shown = snapshot(&stale);
    let base = shown.workspaces[0].clone();
    let mut payload = base.clone();
    payload.name = "alpha2".to_owned();
    let edits = persist::workspace(&stale, Some(&base), &payload, &Resolved::default())?;
    let alpha = WorkspaceName::parse("alpha")?;
    let mut newer = stale.clone();
    newer.apply(ConfigEdit::SetWorkspaceDocker {
        workspace: alpha.clone(),
        docker: false,
    })?;
    newer.apply(ConfigEdit::SetEnv {
        scope: EnvScope::Workspace(alpha),
        name: jackin_core::EnvName::parse("LATE")?,
        value: Some(jackin_core::PlainEnvValue::parse("1")?),
    })?;
    for edit in edits {
        newer.apply(edit)?;
    }
    let moved = newer
        .workspace(&WorkspaceName::parse("alpha2")?)
        .ok_or("alpha2")?;
    assert!(!moved.docker, "the other handle's docker was overwritten");
    assert!(
        moved.env.keys().any(|key| key.as_str() == "LATE"),
        "the other handle's env was dropped"
    );
    Ok(())
}
