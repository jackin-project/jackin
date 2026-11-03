// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Config, ConfigError, GatewayName, HostPath, RoleName};
use jackin_image::Rebuild;
use jackin_session::{
    BuildError, BuildRequest, LAUNCH_EVENT_CAPACITY, RoleError, build_for_workspace,
};
use jackin_testkit::{Rig, TestResult, guard};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

async fn build(
    rig: &Rig,
    config: &Config,
    request: &BuildRequest<'_>,
) -> Result<jackin_image::BuiltImage, BuildError> {
    let (progress, mut events) = mpsc::channel(LAUNCH_EVENT_CAPACITY);
    let drain = tokio::spawn(async move { while events.recv().await.is_some() {} });
    let built = build_for_workspace(
        rig.world.engine.as_ref(),
        rig.world.roles.as_ref(),
        config,
        request,
        &progress,
        &CancellationToken::new(),
    )
    .await;
    drop(progress);
    drop(drain.await);
    built
}

fn request_for<'a>(role: Option<&'a RoleName>, path: &'a HostPath) -> BuildRequest<'a> {
    BuildRequest {
        gateway: None,
        role,
        path,
        rebuild: Rebuild::No,
    }
}

fn here(rig: &Rig) -> HostPath {
    rig.project.path().clone()
}

#[tokio::test]
async fn a_build_makes_the_role_image_once_and_a_rebuild_makes_it_again() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let path = here(&rig);
        let request = BuildRequest {
            gateway: None,
            role: None,
            path: &path,
            rebuild: Rebuild::No,
        };
        let first = build(&rig, &rig.config, &request).await?;
        assert_eq!(rig.world.engine.images().get(&first.tag), Some(&first.id));
        let builds = rig.world.engine.builds().len();
        let again = build(&rig, &rig.config, &request).await?;
        assert_eq!(
            (again.tag.clone(), again.id.clone()),
            (first.tag.clone(), first.id.clone())
        );
        assert_eq!(rig.world.engine.builds().len(), builds);
        let forced = BuildRequest {
            rebuild: Rebuild::Yes,
            ..request
        };
        build(&rig, &rig.config, &forced).await?;
        assert!(rig.world.engine.builds().len() > builds);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_role_comes_from_the_request_then_the_workspace_default_then_the_only_role()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let root = rig.loaded.root().as_path().display().to_string();
        let saved = rig.project.path().as_path().display().to_string();
        let two_roles = Config::parse(&format!(
            "{}\n[roles.second]\nsource = {{ path = \"{root}\" }}\n",
            rig.config.render()
        ))?;
        let with_default = Config::parse(&format!(
            "{}\n[roles.second]\nsource = {{ path = \"{root}\" }}\n",
            rig.config.render().replace(
                &format!("path = \"{saved}\"\n"),
                &format!("path = \"{saved}\"\ndefault_role = \"second\"\n")
            )
        ))?;
        let path = here(&rig);
        let request = |role| request_for(role, &path);
        build(&rig, &rig.config, &request(None)).await?;

        let unsaved_dir = tempfile::tempdir()?;
        let unsaved = HostPath::parse(&std::fs::canonicalize(unsaved_dir.path())?)?;
        let from_elsewhere = BuildRequest {
            path: &unsaved,
            ..request(None)
        };
        build(&rig, &rig.config, &from_elsewhere).await?;
        assert!(matches!(
            build(&rig, &two_roles, &from_elsewhere).await,
            Err(BuildError::Role(RoleError::NoRole))
        ));
        assert!(matches!(
            build(&rig, &two_roles, &request(None)).await,
            Err(BuildError::Role(RoleError::NoRole))
        ));
        build(&rig, &with_default, &request(None)).await?;
        assert!(matches!(
            build(&rig, &with_default, &from_elsewhere).await,
            Err(BuildError::Role(RoleError::NoRole))
        ));
        let sentinel = RoleName::parse("sentinel")?;
        build(&rig, &two_roles, &request(Some(&sentinel))).await?;
        let ghost = RoleName::parse("ghost")?;
        assert!(matches!(
            build(&rig, &rig.config, &request(Some(&ghost))).await,
            Err(BuildError::Role(RoleError::UnknownRole(name))) if name == ghost
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unknown_gateway_is_refused_before_any_role_is_read() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let path = here(&rig);
        let ghost = GatewayName::parse("ghost")?;
        let request = BuildRequest {
            gateway: Some(&ghost),
            role: None,
            path: &path,
            rebuild: Rebuild::No,
        };
        assert!(matches!(
            build(&rig, &rig.config, &request).await,
            Err(BuildError::Config(ConfigError::UnknownGateway(name))) if name == ghost
        ));
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}
