// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::world::TrustKind;
use jackin_core::{Config, ConfigEdit, RoleName};
use jackin_testkit::{ConsoleRig, TestResult, World, guard};

const BASE: &str = "default_gateway = \"fake-gateway\"\n\n[gateways.fake-gateway]\nopenshell_workspace = \"default\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n";

const ROLES: &str = "\n[roles.one]\nsource = { git = \"https://github.com/acme/roles\" }\n\n[roles.two]\nsource = { git = \"https://github.com/acme/roles\" }\n\n[roles.local]\nsource = { path = \"/home/dev/local-role\" }\n";

async fn console(text: &str) -> Result<(World, ConsoleRig), Box<dyn std::error::Error>> {
    let world = World::builder().build()?;
    world.seed_all(&Config::parse(text)?).await?;
    let console = ConsoleRig::new(&world).await?;
    Ok((world, console))
}

#[tokio::test]
async fn trust_rows_from_sources() -> TestResult {
    guard(async {
        let (_world, mut console) = console(&format!("{BASE}{ROLES}")).await?;
        let trust = console.console().app().world.global.trust.clone();
        assert_eq!(trust.len(), 2);
        assert!(trust.iter().all(|row| row.trusted));
        let git = trust
            .iter()
            .find(|r| r.kind == TrustKind::Git)
            .ok_or("no git row")?;
        assert_eq!(git.roles, 2);
        assert!(git.source.contains("acme/roles"));
        let path = trust
            .iter()
            .find(|r| r.kind == TrustKind::Path)
            .ok_or("no path row")?;
        assert_eq!(path.roles, 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn trust_o_shows_source_sticky() -> TestResult {
    guard(async {
        let (_world, mut console) = console(&format!("{BASE}{ROLES}")).await?;
        console.skip_intro();
        console.keys("s ] ] ] ] enter o")?;
        let status = console.status_line();
        assert!(status.contains("Source"), "{status}");
        assert!(status.contains("local-role"), "{status}");
        assert!(
            !status.contains("Opened") && !status.contains("Revealed"),
            "{status}"
        );
        console.tick(40);
        assert_eq!(console.status_line(), status);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn trust_row_updates_after_role_removal() -> TestResult {
    guard(async {
        let (world, mut console) = console(&format!("{BASE}{ROLES}")).await?;
        console.skip_intro();
        world
            .config
            .edit(
                vec![
                    ConfigEdit::RemoveRole(RoleName::parse("one")?),
                    ConfigEdit::RemoveRole(RoleName::parse("two")?),
                ],
                &tokio_util::sync::CancellationToken::new(),
            )
            .await?;
        for _ in 0..8 {
            console.elapse(1);
            console.tick(1);
        }
        console.settle().await?;
        let trust = &console.console().app().world.global.trust;
        assert_eq!(trust.len(), 1);
        assert_eq!(trust[0].kind, TrustKind::Path);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn trust_empty_state() -> TestResult {
    guard(async {
        let (_world, mut console) = console(BASE).await?;
        assert!(console.console().app().world.global.trust.is_empty());
        console.skip_intro();
        console.keys("s ] ] ] ] enter")?;
        let screen = console.screen();
        assert!(screen.to_lowercase().contains("no "), "{screen}");
        Ok(())
    })
    .await
}
