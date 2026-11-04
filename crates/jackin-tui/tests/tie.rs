// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::domain::account::AccountForm;
use jackin_console::domain::agent::{AgentBlocked, AuthMode};
use jackin_console::domain::fixed::Fixed;
use jackin_console::domain::fixed_fields::{
    AccountFixed, FixedFields, SurfaceFixed, WorkspaceFixed, snapshot_fixed, world_fixed,
};
use jackin_console::domain::openshell::PROJECT_DIR;
use jackin_console::domain::role::LaunchRoleRule;
use jackin_console::domain::vocabulary::Vocabulary;
use jackin_console::domain::workspace::{DirtyExitPolicy, Isolation, WizardDefaults};
use jackin_console::fixtures::{Scenario, world_for};
use jackin_console::world::Snapshot;
use jackin_core::Config;
use jackin_tui::{Facts, Fleet, IdTable, Origins, ProjectInput, RoleChecks, RoleDocs, project};

const ACCOUNT_ID: &str = "3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f";

const GATEWAY: &str = "default_gateway = \"g\"\n\n[gateways.g]\nopenshell_workspace = \"default\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n";

fn login() -> String {
    format!(
        "[accounts.work]\nprovider = \"codex\"\ncredential = {{ source = \"codex-login\", home = \"/home/dev/.codex\", chatgpt_account_id = \"{ACCOUNT_ID}\" }}\n"
    )
}

fn shapes() -> Vec<(&'static str, String)> {
    let op = "[accounts.ci]\nprovider = \"codex\"\ncredential = { source = \"op\", reference = \"op://Dev/OpenAI/credential\" }\n";
    let env = "[accounts.envy]\nprovider = \"claude\"\ncredential = { source = \"env\", var = \"ANTHROPIC_API_KEY\" }\n";
    let role = "[roles.sentinel]\nsource = { path = \"/home/dev/role\" }\nbindings = { codex = \"work\" }\n";
    vec![
        (
            "docker workspace",
            format!(
                "{GATEWAY}{}{role}\n[workspaces.alpha]\npath = \"/home/dev/alpha\"\ndocker = true\n",
                login()
            ),
        ),
        (
            "allow-list",
            format!(
                "{GATEWAY}{}{role}\n[workspaces.alpha]\npath = \"/home/dev/alpha\"\ndefault_role = \"sentinel\"\nroles = [\"sentinel\"]\n",
                login()
            ),
        ),
        (
            "env at three scopes",
            format!(
                "{GATEWAY}{}{role}\n[env]\nREGION = \"eu\"\n\n[workspaces.alpha]\npath = \"/home/dev/alpha\"\n\n[workspaces.alpha.env]\nREGION = \"us\"\n\n[workspaces.alpha.role_env.sentinel]\nREGION = \"ap\"\n",
                login()
            ),
        ),
        (
            "op account",
            format!(
                "{GATEWAY}{}{op}{role}\n[workspaces.alpha]\npath = \"/home/dev/alpha\"\n\n[workspaces.beta]\npath = \"/home/dev/beta\"\nbindings = {{ codex = \"ci\" }}\n",
                login()
            ),
        ),
        (
            "env account",
            format!(
                "{GATEWAY}{}{env}\n[workspaces.alpha]\npath = \"/home/dev/alpha\"\nbindings = {{ claude = \"envy\" }}\n",
                login()
            ),
        ),
        (
            "login account",
            format!(
                "{GATEWAY}{}\n[workspaces.alpha]\npath = \"/home/dev/alpha\"\nbindings = {{ codex = \"work\" }}\n",
                login()
            ),
        ),
        ("zero workspaces", format!("{GATEWAY}{}", login())),
    ]
}

fn projected(text: &str) -> Result<(Config, Snapshot), Box<dyn std::error::Error>> {
    let config = Config::parse(text)?;
    let snapshot = project(
        &ProjectInput {
            config: &config,
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
    );
    Ok((config, snapshot))
}

struct Fixture {
    workspace: WorkspaceFixed,
    account: AccountFixed,
    fixed: FixedFields,
}

fn fixture() -> Fixture {
    let fixed = world_fixed(&world_for(Scenario::OpenshellFixed));
    let first = |fields: &FixedFields| (fields.workspaces[0].clone(), fields.accounts[0].clone());
    let (workspace, account) = first(&fixed);
    assert!(
        fixed.workspaces.iter().all(|w| *w == workspace),
        "the fixture holds one workspace value"
    );
    assert!(
        fixed.accounts.iter().all(|a| *a == account),
        "the fixture holds one account value"
    );
    Fixture {
        workspace,
        account,
        fixed,
    }
}

type Shape = (&'static str, Config, FixedFields, Snapshot);

fn every_shape() -> Result<Vec<Shape>, Box<dyn std::error::Error>> {
    shapes()
        .into_iter()
        .map(|(name, text)| {
            let (config, snapshot) = projected(&text)?;
            let fields = snapshot_fixed(&snapshot);
            Ok((name, config, fields, snapshot))
        })
        .collect()
}

#[test]
fn workspace_fixed_values_match() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture();
    assert_eq!(fixture.workspace.workdir, PROJECT_DIR);
    assert_eq!(fixture.workspace.mounts.len(), 1);
    assert_eq!(fixture.workspace.mounts[0].destination, PROJECT_DIR);
    assert_eq!(fixture.workspace.mounts[0].isolation, Isolation::Clone);
    assert_eq!(fixture.workspace.dirty_policy, DirtyExitPolicy::Ask);
    for (name, _, fields, _) in every_shape()? {
        for workspace in &fields.workspaces {
            assert_eq!(workspace, &fixture.workspace, "{name}");
        }
    }
    Ok(())
}

#[test]
fn global_fixed_values_match() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture();
    assert!(fixture.fixed.global.non_launchable_modes_ignore);
    assert!(fixture.fixed.global.all_trusted);
    for (name, _, fields, _) in every_shape()? {
        assert_eq!(fields.global, fixture.fixed.global, "{name}");
    }
    Ok(())
}

#[test]
fn account_fixed_values_match() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture();
    for (name, _, fields, _) in every_shape()? {
        for account in &fields.accounts {
            assert_eq!(account, &fixture.account, "{name}");
        }
        for role in &fields.roles {
            assert!(role.trusted, "{name}");
        }
    }
    Ok(())
}

#[test]
fn surface_values_match() -> Result<(), Box<dyn std::error::Error>> {
    let expected = SurfaceFixed {
        vocabulary: Vocabulary::openshell(),
        fixed: Some(Fixed::openshell()),
        account_form: AccountForm::openshell(),
        agent_blocked: Some(AgentBlocked::openshell()),
        wizard: Some(WizardDefaults::openshell()),
        launch_role: LaunchRoleRule::Configured,
    };
    assert_eq!(fixture().fixed.surfaces, expected);
    for (name, _, fields, _) in every_shape()? {
        let live = &fields.surfaces;
        assert_eq!(live.vocabulary, expected.vocabulary, "{name} vocabulary");
        assert_eq!(live.fixed, expected.fixed, "{name} fixed");
        assert_eq!(
            live.account_form, expected.account_form,
            "{name} account_form"
        );
        assert_eq!(
            live.agent_blocked, expected.agent_blocked,
            "{name} agent_blocked"
        );
        assert_eq!(live.wizard, expected.wizard, "{name} wizard");
        assert_eq!(live.launch_role, expected.launch_role, "{name} launch_role");
    }
    Ok(())
}

#[test]
fn every_config_shape_matches() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture();
    for (name, config, fields, snapshot) in every_shape()? {
        assert_eq!(
            fields.workspaces.len(),
            config.workspaces().count(),
            "{name}"
        );
        assert_eq!(
            snapshot.accounts.accounts.len(),
            config.accounts().count(),
            "{name}"
        );
        assert!(
            fields.workspaces.iter().all(|w| *w == fixture.workspace),
            "{name}"
        );
        assert_eq!(fields.global, fixture.fixed.global, "{name}");
        assert!(
            snapshot
                .global
                .agent_modes
                .iter()
                .filter(|(agent, _)| **agent != Fixed::openshell().launchable)
                .all(|(_, mode)| *mode == AuthMode::Ignore),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn counts_are_not_empty() -> Result<(), Box<dyn std::error::Error>> {
    let shapes = every_shape()?;
    let workspaces: usize = shapes.iter().map(|(_, _, f, _)| f.workspaces.len()).sum();
    let accounts: usize = shapes.iter().map(|(_, _, f, _)| f.accounts.len()).sum();
    let roles: usize = shapes.iter().map(|(_, _, f, _)| f.roles.len()).sum();
    assert_eq!(shapes.len(), 7);
    assert_eq!(workspaces, 1 + 1 + 1 + 2 + 1 + 1);
    assert_eq!(accounts, 1 + 1 + 1 + 2 + 2 + 1 + 1);
    assert_eq!(roles, 4);
    Ok(())
}

type Mutation = (&'static str, fn(&mut Snapshot), fn(&FixedFields) -> String);

fn mutations() -> Vec<Mutation> {
    vec![
        (
            "workdir",
            |s| s.workspaces[0].workdir = "/work".into(),
            |f| f.workspaces[0].workdir.clone(),
        ),
        (
            "mounts",
            |s| s.workspaces[0].mounts.clear(),
            |f| format!("{:?}", f.workspaces[0].mounts),
        ),
        (
            "keep_awake",
            |s| s.workspaces[0].keep_awake = true,
            |f| f.workspaces[0].keep_awake.to_string(),
        ),
        (
            "git_pull",
            |s| s.workspaces[0].git_pull = true,
            |f| f.workspaces[0].git_pull.to_string(),
        ),
        (
            "dirty_policy",
            |s| s.workspaces[0].dirty_policy = DirtyExitPolicy::Discard,
            |f| format!("{:?}", f.workspaces[0].dirty_policy),
        ),
        (
            "disabled_defaults_empty",
            |s| {
                s.workspaces[0]
                    .accounts
                    .disabled_defaults
                    .insert("work".into());
            },
            |f| f.workspaces[0].disabled_defaults_empty.to_string(),
        ),
        (
            "role_preferred_empty",
            |s| {
                s.workspaces[0].accounts.role_preferred.insert(
                    (
                        "sentinel".into(),
                        jackin_console::domain::agent::Provider::OpenAi,
                    ),
                    "work".into(),
                );
            },
            |f| f.workspaces[0].role_preferred_empty.to_string(),
        ),
        (
            "enabled_is_every_account",
            |s| {
                s.workspaces[0].accounts.enabled.clear();
                s.accounts.accounts[0].default_for_provider = false;
            },
            |f| f.workspaces[0].enabled_is_every_account.to_string(),
        ),
        (
            "coauthor_trailer",
            |s| s.global.coauthor_trailer = true,
            |f| f.global.coauthor_trailer.to_string(),
        ),
        (
            "dco_signoff",
            |s| s.global.dco_signoff = true,
            |f| f.global.dco_signoff.to_string(),
        ),
        (
            "mounts_empty",
            |s| {
                s.global
                    .mounts
                    .push(jackin_console::domain::workspace::Mount::host("/a", "/b"));
            },
            |f| f.global.mounts_empty.to_string(),
        ),
        (
            "role_env_empty",
            |s| {
                s.global.role_env.insert("sentinel".into(), Vec::new());
            },
            |f| f.global.role_env_empty.to_string(),
        ),
        (
            "non_launchable_modes_ignore",
            |s| {
                s.global
                    .agent_modes
                    .insert(jackin_console::domain::agent::Agent::Amp, AuthMode::Sync);
            },
            |f| f.global.non_launchable_modes_ignore.to_string(),
        ),
        (
            "all_trusted",
            |s| {
                if let Some(row) = s.global.trust.first_mut() {
                    row.trusted = false;
                }
            },
            |f| f.global.all_trusted.to_string(),
        ),
        (
            "enabled",
            |s| s.accounts.accounts[0].enabled = false,
            |f| f.accounts[0].enabled.to_string(),
        ),
        (
            "origin",
            |s| {
                s.accounts.accounts[0].origin =
                    jackin_console::domain::account::AccountOrigin::Discovered;
            },
            |f| format!("{:?}", f.accounts[0].origin),
        ),
        (
            "vocabulary",
            |s| s.surfaces.vocabulary = Vocabulary::demo(),
            |f| format!("{:?}", f.surfaces.vocabulary),
        ),
        (
            "fixed",
            |s| s.surfaces.fixed = None,
            |f| format!("{:?}", f.surfaces.fixed),
        ),
        (
            "account_form",
            |s| s.surfaces.account_form = AccountForm::default(),
            |f| format!("{:?}", f.surfaces.account_form),
        ),
        (
            "agent_blocked",
            |s| s.surfaces.agent_blocked = None,
            |f| format!("{:?}", f.surfaces.agent_blocked),
        ),
        (
            "wizard",
            |s| s.surfaces.wizard = None,
            |f| format!("{:?}", f.surfaces.wizard),
        ),
        (
            "launch_role",
            |s| s.surfaces.launch_role = LaunchRoleRule::Demo,
            |f| format!("{:?}", f.surfaces.launch_role),
        ),
    ]
}

#[test]
fn each_field_mutation_is_detected() -> Result<(), Box<dyn std::error::Error>> {
    let (_, base) = projected(&shapes()[0].1)?;
    let before = snapshot_fixed(&base);
    for (field, mutate, read) in mutations() {
        let mut changed = base.clone();
        mutate(&mut changed);
        let after = snapshot_fixed(&changed);
        assert_ne!(read(&after), read(&before), "{field} is not detected");
        assert_ne!(after, before, "{field} leaves the classification equal");
    }
    Ok(())
}
