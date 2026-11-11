// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use jackin_console::backend::Effect;
use jackin_console::world::World;
use jackin_core::{Config, ConfigEdit, RoleName, RoleSource};
use jackin_testkit::{ConsoleRig, Rig, RoleFixture, TestResult};

pub(crate) struct Lab {
    pub(crate) rig: Rig,
    pub(crate) console: ConsoleRig,
    pub(crate) code: PathBuf,
}

impl Lab {
    pub(crate) async fn open() -> Result<Lab, Box<dyn std::error::Error>> {
        Lab::with(Rig::new().await?).await
    }

    pub(crate) async fn with(rig: Rig) -> Result<Lab, Box<dyn std::error::Error>> {
        Lab::with_env(rig, &[]).await
    }

    pub(crate) async fn with_env(
        rig: Rig,
        env: &[(&str, &str)],
    ) -> Result<Lab, Box<dyn std::error::Error>> {
        let code = rig.world.home().as_path().join("code");
        for name in ["alpha", "beta"] {
            tokio::fs::create_dir_all(code.join(name)).await?;
        }
        rig.world.seed_all(&rig.config).await?;
        let mut builder = ConsoleRig::builder(&rig.world);
        for (name, value) in env {
            builder = builder.env(name, value);
        }
        let mut console = builder.build().await?;
        console.skip_intro();
        Ok(Lab { rig, console, code })
    }

    pub(crate) async fn reopen(&mut self) -> TestResult {
        self.console = ConsoleRig::new(&self.rig.world).await?;
        self.console.skip_intro();
        Ok(())
    }

    pub(crate) fn world(&mut self) -> &mut World {
        &mut self.console.console().app().world
    }

    pub(crate) async fn config(&self) -> Result<Config, Box<dyn std::error::Error>> {
        Ok(self.rig.world.config.load().await?)
    }

    pub(crate) async fn edit(&self, edits: Vec<ConfigEdit>) -> TestResult {
        self.rig.world.config.edit(edits).await?;
        Ok(())
    }

    pub(crate) async fn add_role(
        &self,
        name: &str,
    ) -> Result<RoleFixture, Box<dyn std::error::Error>> {
        let fixture = RoleFixture::new(name)?;
        fixture.commit().await?;
        self.edit(vec![ConfigEdit::AddRole {
            name: RoleName::parse(name)?,
            source: RoleSource::Path(fixture.root().clone()),
        }])
        .await?;
        Ok(fixture)
    }

    pub(crate) async fn save(&mut self) -> TestResult {
        self.console.keys("ctrl+s right enter")?;
        self.console.settle().await
    }

    pub(crate) async fn perform(&mut self, effect: Effect) -> TestResult {
        self.world().perform(effect);
        self.console.settle().await
    }

    pub(crate) fn workspace(
        &mut self,
        name: &str,
    ) -> Option<jackin_console::domain::workspace::Workspace> {
        self.world()
            .workspaces
            .iter()
            .find(|w| w.name == name)
            .cloned()
    }
}

pub(crate) const CANARY: &str = "CANARY-op-secret-91c7d2e8";

impl Lab {
    pub(crate) async fn step(&mut self, keys: &str) -> TestResult {
        self.console.keys(keys)?;
        self.console.settle().await
    }

    pub(crate) async fn open_form(&mut self, name: &str) -> TestResult {
        self.step("c").await?;
        self.step("a").await?;
        self.step("enter").await?;
        self.console.type_text(name);
        self.console.settle().await?;
        self.step("tab").await?;
        self.step("tab").await
    }

    pub(crate) async fn choose_codex(&mut self) -> TestResult {
        self.step("enter").await?;
        self.step("down").await?;
        self.step("enter").await
    }

    pub(crate) async fn choose_one_password(&mut self) -> TestResult {
        self.step("tab").await?;
        self.step("up").await?;
        self.step("space").await
    }
}

impl Lab {
    pub(crate) async fn folder_form(
        &mut self,
        name: &str,
        provider: usize,
        folder: &str,
    ) -> TestResult {
        self.open_form(name).await?;
        self.step("enter").await?;
        for _ in 0..provider {
            self.step("down").await?;
        }
        self.step("enter").await?;
        self.step("tab").await?;
        self.step("tab").await?;
        self.console.type_text(folder);
        self.console.settle().await
    }

    pub(crate) async fn save_form(&mut self) -> TestResult {
        self.step("tab tab tab tab enter").await
    }
}
