// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::{ExitClass, RoleLs, WorkspaceLs};
use jackin_core::{HostPath, RoleName, RoleRev, RoleSource, WorkspaceName};
use jackin_testkit::{TestResult, World, guard};

use super::support::{Failure, Harness, failed};

const URL: &str = "https://example.com/org/role.git";

fn role(name: &str) -> Result<RoleName, Failure> {
    Ok(RoleName::parse(name)?)
}

fn workspace(name: &str) -> Result<WorkspaceName, Failure> {
    Ok(WorkspaceName::parse(name)?)
}

#[tokio::test]
async fn role_add_stores_a_path_or_a_git_source_and_ls_pins_both_shapes() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        std::fs::create_dir(world.home().as_path().join("local-role"))?;
        let harness = Harness::new(&world);
        harness
            .run(&["role", "add", "local", "local-role"])
            .await?
            .ok()?;
        harness
            .run(&["role", "add", "remote", URL, "--rev", "v1.2"])
            .await?
            .ok()?;
        harness.run(&["role", "add", "bare", URL]).await?.ok()?;

        let config = harness.stored().await?;
        let local = config.role(&role("local")?).ok_or("no local")?;
        assert_eq!(
            local.source,
            RoleSource::Path(HostPath::parse(&world.home().as_path().join("local-role"))?)
        );
        let remote = config.role(&role("remote")?).ok_or("no remote")?;
        let RoleSource::Git { url, rev } = &remote.source else {
            return Err("not a git source".into());
        };
        assert_eq!(url.as_str(), URL);
        assert_eq!(rev.as_ref().map(RoleRev::as_str), Some("v1.2"));

        let ran = harness.run(&["role", "ls", "--json"]).await?;
        ran.ok()?;
        let local_path = world.home().as_path().join("local-role");
        assert_eq!(
            ran.stdout,
            format!(
                concat!(
                    "{{\"schema\":1,\"roles\":[",
                    "{{\"name\":\"bare\",\"source\":{{\"kind\":\"git\",\"url\":\"{url}\",\"rev\":null}},\"bindings\":{{}}}},",
                    "{{\"name\":\"local\",\"source\":{{\"kind\":\"path\",\"path\":\"{path}\"}},\"bindings\":{{}}}},",
                    "{{\"name\":\"remote\",\"source\":{{\"kind\":\"git\",\"url\":\"{url}\",\"rev\":\"v1.2\"}},\"bindings\":{{}}}}]}}\n"
                ),
                url = URL,
                path = local_path.display()
            )
        );
        let typed: RoleLs = ran.json()?;
        assert_eq!(typed.roles.len(), 3);

        harness.run(&["role", "rm", "local"]).await?.ok()?;
        assert!(harness.stored().await?.role(&role("local")?).is_none());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn workspace_add_stores_the_docker_flag_and_an_absolute_path_and_ls_pins_the_shape()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness
            .run(&["workspace", "add", "plain", "proj/one"])
            .await?
            .ok()?;
        harness
            .run(&["workspace", "add", "boxed", "../two", "--docker"])
            .await?
            .ok()?;

        let config = harness.stored().await?;
        let plain = config.workspace(&workspace("plain")?).ok_or("no plain")?;
        assert_eq!(
            plain.path.as_path(),
            world.home().as_path().join("proj/one")
        );
        assert!(!plain.docker);
        let boxed = config.workspace(&workspace("boxed")?).ok_or("no boxed")?;
        assert_eq!(
            Some(boxed.path.as_path()),
            world
                .home()
                .as_path()
                .parent()
                .map(|parent| parent.join("two"))
                .as_deref()
        );
        assert!(boxed.docker);

        let ran = harness.run(&["workspace", "ls", "--json"]).await?;
        ran.ok()?;
        let two = world.home().as_path().parent().ok_or("no parent")?.join("two");
        assert_eq!(
            ran.stdout,
            format!(
                concat!(
                    "{{\"schema\":1,\"workspaces\":[",
                    "{{\"name\":\"boxed\",\"path\":\"{two}\",\"docker\":true,\"default_role\":null,\"bindings\":{{}}}},",
                    "{{\"name\":\"plain\",\"path\":\"{one}\",\"docker\":false,\"default_role\":null,\"bindings\":{{}}}}]}}\n"
                ),
                two = two.display(),
                one = world.home().as_path().join("proj/one").display()
            )
        );
        let typed: WorkspaceLs = ran.json()?;
        assert_eq!(typed.workspaces.len(), 2);

        harness.run(&["workspace", "rm", "plain"]).await?.ok()?;
        assert!(
            harness
                .stored()
                .await?
                .workspace(&workspace("plain")?)
                .is_none()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn duplicates_shared_paths_and_roles_in_use_fail_and_leave_the_config_bytes_alone()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        std::fs::create_dir(world.home().as_path().join("proj"))?;
        let harness = Harness::new(&world);
        harness.run(&["role", "add", "remote", URL]).await?.ok()?;
        harness
            .run(&["workspace", "add", "one", "proj"])
            .await?
            .ok()?;
        let mut text = harness.config_bytes()?.ok_or("no config")?;
        text = text.replace(
            "[workspaces.one]\n",
            "[workspaces.one]\ndefault_role = \"remote\"\n",
        );
        harness.write_config(&text)?;
        let before = harness.config_bytes()?;
        let listed = harness.run(&["workspace", "ls", "--json"]).await?;
        assert_eq!(
            listed.stdout,
            format!(
                "{{\"schema\":1,\"workspaces\":[{{\"name\":\"one\",\"path\":\"{}\",\"docker\":false,\"default_role\":\"remote\",\"bindings\":{{}}}}]}}\n",
                world.home().as_path().join("proj").display()
            )
        );

        let cases: [(&[&str], &str); 4] = [
            (&["role", "add", "remote", URL], "remote"),
            (&["workspace", "add", "one", "other"], "one"),
            (&["workspace", "add", "two", "proj"], "two"),
            (&["role", "rm", "remote"], "one"),
        ];
        for (args, named) in cases {
            let ran = harness.run(args).await?;
            failed(&ran, ExitClass::Failure, &[named])?;
            assert_eq!(harness.config_bytes()?, before, "{args:?}");
        }
        let rev = harness
            .run(&["role", "add", "x", "proj", "--rev", "v1"])
            .await?;
        assert_eq!(rev.exit, ExitClass::Usage, "{}", rev.stderr);
        assert_eq!(harness.config_bytes()?, before);
        Ok(())
    })
    .await
}
