// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::app::Route;
use jackin_core::{
    ConfigEdit, EnvName, EnvScope, HookKind, PlainEnvValue, RoleName, WorkspaceName,
};
use jackin_testkit::{Rig, RoleFixture, TestResult, guard};

use super::support::{answer, begin, console, env_file, only, until_route};

type Failure = Box<dyn std::error::Error>;

fn set(scope: EnvScope, name: &str, value: &str) -> Result<ConfigEdit, Failure> {
    Ok(ConfigEdit::SetEnv {
        scope,
        name: EnvName::parse(name)?,
        value: Some(PlainEnvValue::parse(value)?),
    })
}

#[tokio::test]
async fn the_env_file_follows_the_scope_order() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?
            .env("GREETING", Some("hello"), Some("Greeting"))
            .env("LEVEL", Some("manifest"), Some("Level"))
            .hook(HookKind::Setup, b"#!/bin/sh\n")
            .hook(HookKind::Preflight, b"#!/bin/sh\n");
        let rig = Rig::builder().role(role).build().await?;
        rig.world.seed_all(&rig.config).await?;
        let workspace = WorkspaceName::parse("platform-backend")?;
        let role = RoleName::parse("sentinel")?;
        rig.world
            .config
            .edit(vec![
                set(EnvScope::Global, "ALPHA", "from-global")?,
                set(EnvScope::Global, "BRAVO", "from-global")?,
                set(EnvScope::Global, "CHARLIE", "from-global")?,
                set(EnvScope::Global, "UNDECLARED", "from-global")?,
                set(
                    EnvScope::Workspace(workspace.clone()),
                    "BRAVO",
                    "from-workspace",
                )?,
                set(
                    EnvScope::Workspace(workspace.clone()),
                    "CHARLIE",
                    "from-workspace",
                )?,
                set(
                    EnvScope::Workspace(workspace.clone()),
                    "LEVEL",
                    "from-workspace",
                )?,
                set(
                    EnvScope::WorkspaceRole(workspace, role),
                    "CHARLIE",
                    "from-role",
                )?,
            ])
            .await?;
        let mut console = console(&rig).await?;
        begin(&mut console)?;
        answer(&mut console, Some("typed")).await?;
        until_route(&mut console, Route::Capsule).await?;
        let file = env_file(&only(&rig)?)?;
        for line in [
            "export ALPHA='from-global'",
            "export BRAVO='from-workspace'",
            "export CHARLIE='from-role'",
            "export UNDECLARED='from-global'",
            "export LEVEL='from-workspace'",
            "export GREETING='typed'",
        ] {
            assert!(file.lines().any(|l| l == line), "{line}\n{file}");
        }
        Ok(())
    })
    .await
}
