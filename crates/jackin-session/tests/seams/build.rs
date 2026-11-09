// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Config, ConfigError, GatewayName, HostPath, RoleName};
use jackin_image::{BuiltImage, ImageError, ImageRequest, ImageSource, Rebuild, ensure_image};
use jackin_session::{
    BuildError, BuildRequest, LAUNCH_EVENT_CAPACITY, RoleError, build_for_workspace,
    plan_image_for, plan_launch,
};
use jackin_testkit::{ENGINE_PORT, Rig, RoleFixture, TestResult, guard};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

async fn build(
    rig: &Rig,
    config: &Config,
    request: &BuildRequest<'_>,
) -> Result<BuiltImage, BuildError> {
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
        agent: jackin_core::Agent::Codex,
        provider: jackin_core::Provider::Codex,
        model: None,
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
            agent: jackin_core::Agent::Codex,
            provider: jackin_core::Provider::Codex,
            model: None,
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
        let second = RoleFixture::new("second")?;
        second.commit().await?;
        let root = second.root().as_path().display().to_string();
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
            agent: jackin_core::Agent::Codex,
            provider: jackin_core::Provider::Codex,
            model: None,
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

async fn reuse(rig: &Rig, image: &ImageRequest) -> Result<BuiltImage, ImageError> {
    let (progress, mut events) = mpsc::channel(LAUNCH_EVENT_CAPACITY);
    let drain = tokio::spawn(async move { while events.recv().await.is_some() {} });
    let found = ensure_image(
        rig.world.engine.as_ref(),
        image,
        &progress,
        &CancellationToken::new(),
    )
    .await;
    drop(progress);
    drop(drain.await);
    found
}

#[tokio::test]
async fn a_build_makes_the_image_that_a_launch_of_the_same_workspace_finds_already_built()
-> TestResult {
    guard(async {
        for role_docker in [false, true] {
            let rig = Rig::builder()
                .role_docker(role_docker)
                .engine(ENGINE_PORT)
                .build()
                .await?;
            let flagged_text = rig
                .config
                .render()
                .replace("docker = false", "docker = true");
            assert_ne!(flagged_text, rig.config.render());
            let flagged = Config::parse(&flagged_text)?;
            let path = here(&rig);
            let mut tags = Vec::new();
            for (label, config) in [("workspace docker off", &rig.config), ("on", &flagged)] {
                let built = build(&rig, config, &request_for(None, &path)).await?;
                let builds = rig.world.engine.builds().len();
                let plan = plan_launch(config, &rig.loaded, rig.request(1)?)?;
                let found = reuse(&rig, plan.image_request()).await?;
                assert_eq!(
                    (&found.tag, &found.id, found.source),
                    (&built.tag, &built.id, ImageSource::Cache),
                    "role docker {role_docker}, {label}"
                );
                assert_eq!(rig.world.engine.builds().len(), builds);
                tags.push(built.tag);
            }
            assert_eq!(tags[0] == tags[1], role_docker);

            let unsaved_dir = tempfile::tempdir()?;
            let unsaved = HostPath::parse(&std::fs::canonicalize(unsaved_dir.path())?)?;
            let elsewhere = build(&rig, &flagged, &request_for(None, &unsaved)).await?;
            let (gateway, _) = flagged.resolve_gateway(None)?;
            let role_only = plan_image_for(
                &flagged,
                &rig.loaded,
                gateway,
                None,
                Rebuild::No,
                jackin_core::Agent::Codex,
            )?;
            let found = reuse(&rig, &role_only).await?;
            assert_eq!(
                (&found.tag, found.source),
                (&elsewhere.tag, ImageSource::Cache),
                "role docker {role_docker}, an unsaved directory"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_the_workspace_does_not_allow_is_refused_before_anything_is_built() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let second = RoleFixture::new("second")?;
        second.commit().await?;
        let root = second.root().as_path().display().to_string();
        let saved = rig.project.path().as_path().display().to_string();
        let restricted = Config::parse(&format!(
            "{}\n[roles.second]\nsource = {{ path = \"{root}\" }}\n",
            rig.config.render().replace(
                &format!("path = \"{saved}\"\n"),
                &format!("path = \"{saved}\"\nroles = [\"second\"]\n")
            )
        ))?;
        let path = here(&rig);
        let sentinel = RoleName::parse("sentinel")?;
        let refused = build(&rig, &restricted, &request_for(Some(&sentinel), &path)).await;
        assert!(
            matches!(&refused, Err(BuildError::Role(RoleError::NotAllowed(name))) if *name == sentinel),
            "{:?}",
            refused.err()
        );
        assert!(rig.world.engine.builds().is_empty());

        build(&rig, &restricted, &request_for(None, &path)).await?;
        assert!(!rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unavailable_tuples_and_invalid_models_are_refused_before_image_effects() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let path = here(&rig);
        let unavailable = BuildRequest {
            agent: jackin_core::Agent::ClaudeCode,
            provider: jackin_core::Provider::Claude,
            ..request_for(None, &path)
        };
        assert!(matches!(
            build(&rig, &rig.config, &unavailable).await,
            Err(BuildError::Plan(
                jackin_session::PlanError::TupleNotSupported { .. }
            ))
        ));
        assert!(rig.world.engine.builds().is_empty());

        let invalid = BuildRequest {
            model: Some("not-a-model"),
            ..request_for(None, &path)
        };
        assert!(matches!(
            build(&rig, &rig.config, &invalid).await,
            Err(BuildError::Plan(
                jackin_session::PlanError::TupleNotSupported { .. }
            ))
        ));
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn default_and_explicit_t01_selection_share_identity_across_build_and_load() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let path = here(&rig);
        let default = build(&rig, &rig.config, &request_for(None, &path)).await?;
        let builds = rig.world.engine.builds().len();
        let explicit = BuildRequest {
            model: Some("gpt-6.1-sol"),
            ..request_for(None, &path)
        };
        let selected = build(&rig, &rig.config, &explicit).await?;
        assert_eq!(
            (selected.tag, selected.id),
            (default.tag.clone(), default.id.clone())
        );
        assert_eq!(rig.world.engine.builds().len(), builds);

        let other_model = BuildRequest {
            model: Some("gpt-5.1-codex"),
            ..request_for(None, &path)
        };
        let changed_model = build(&rig, &rig.config, &other_model).await?;
        assert_eq!(
            (changed_model.tag, changed_model.id),
            (default.tag.clone(), default.id.clone())
        );
        assert_eq!(rig.world.engine.builds().len(), builds);

        let request = rig.request(1)?.model(Some("gpt-5.1-codex".to_owned()));
        let plan = plan_launch(&rig.config, &rig.loaded, request)?;
        let found = reuse(&rig, plan.image_request()).await?;
        assert_eq!(
            (found.tag, found.id, found.source),
            (default.tag, default.id, ImageSource::Cache)
        );
        assert_eq!(rig.world.engine.builds().len(), builds);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_build_installs_the_selected_agent_and_refuses_mismatched_selectors() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let path = here(&rig);
        let default = build(&rig, &rig.config, &request_for(None, &path)).await?;
        let builds = rig.world.engine.builds().len();

        for (agent, provider) in [
            (jackin_core::Agent::Omp, jackin_core::Provider::Codex),
            (jackin_core::Agent::Codex, jackin_core::Provider::Claude),
            (jackin_core::Agent::Omp, jackin_core::Provider::Claude),
        ] {
            let mismatched = BuildRequest {
                agent,
                provider,
                ..request_for(None, &path)
            };
            assert!(
                matches!(
                    build(&rig, &rig.config, &mismatched).await,
                    Err(BuildError::Plan(
                        jackin_session::PlanError::TupleNotSupported { .. }
                    ))
                ),
                "{agent:?} with {provider:?}"
            );
            assert_eq!(rig.world.engine.builds().len(), builds);
        }

        let omp = BuildRequest {
            agent: jackin_core::Agent::Omp,
            provider: jackin_core::Provider::Zai,
            model: Some("glm-5.3"),
            ..request_for(None, &path)
        };
        let built = build(&rig, &rig.config, &omp).await?;
        assert_ne!(built.tag, default.tag);
        assert_ne!(built.id, default.id);
        let (gateway, _) = rig.config.resolve_gateway(None)?;
        let omp_image = plan_image_for(
            &rig.config,
            &rig.loaded,
            gateway,
            Some(&jackin_session::lookup_workspace(&rig.config, &path).workspace()),
            Rebuild::No,
            jackin_core::Agent::Omp,
        )?;
        let found = reuse(&rig, &omp_image).await?;
        assert_eq!(
            (found.tag, found.id, found.source),
            (built.tag.clone(), built.id.clone(), ImageSource::Cache)
        );

        let codex_on_zai = BuildRequest {
            agent: jackin_core::Agent::Codex,
            provider: jackin_core::Provider::Zai,
            model: Some("glm-5.3"),
            ..request_for(None, &path)
        };
        let shared = build(&rig, &rig.config, &codex_on_zai).await?;
        assert_eq!(
            (shared.tag, shared.id),
            (default.tag.clone(), default.id.clone())
        );
        Ok(())
    })
    .await
}
