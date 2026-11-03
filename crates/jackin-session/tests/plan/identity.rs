// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{
    AccountName, Agent, EnvName, PlannedLabels, ProviderName, RoleName, SandboxLabels, SandboxPath,
    WorkspaceName,
};
use jackin_openshell::MainProcess;
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, built_image, guard, launch_id};

#[tokio::test]
async fn the_plan_derives_name_labels_and_draft_from_the_launch_id() -> TestResult {
    guard(async {
        let setup = Spec::new(RoleFixture::new("sentinel")?).build().await?;
        let plan = setup.plan(setup.request(1)?)?;
        let other = setup.plan(setup.request(2)?)?;
        assert_eq!(plan.sandbox_name().as_str(), "jackin-platfor-ij4f");
        assert_eq!(other.sandbox_name().as_str(), "jackin-platfor-uxe1");
        assert!(plan.sandbox_name().as_str().len() <= 19);
        assert_eq!(plan.launch_id(), &launch_id(1));

        let expected = PlannedLabels {
            workspace: WorkspaceName::parse("platform-backend")?,
            role: RoleName::parse("sentinel")?,
            agent: Agent::Codex,
            account: AccountName::parse("work")?,
            launch: launch_id(1),
        };
        assert_eq!(plan.draft().labels(), &expected);
        assert_eq!(plan.draft().name(), plan.sandbox_name());

        let built = built_image(&setup.role, '3')?;
        let create = plan.draft().complete(&built, jackin_core::DockerLabel::Off);
        assert_eq!(create.image(), &built.tag);
        assert_eq!(create.name(), plan.sandbox_name());
        let labels = SandboxLabels::from_labels(&create.labels().to_labels())?;
        assert_eq!(labels.planned, expected);
        assert_eq!(labels.image.as_str(), "3".repeat(32));
        assert_eq!(
            create.environment(),
            BTreeMap::from([(
                EnvName::parse("JACKIN_LAUNCH_ID")?,
                launch_id(1).to_string()
            )])
        );
        assert_eq!(
            create.providers(),
            [ProviderName::parse("jackin-work-codex")?]
        );
        assert_eq!(
            create.main(),
            &MainProcess {
                program: SandboxPath::parse("/usr/local/bin/jackin-start")?,
                args: Agent::Codex.spec().launch_argv(),
            }
        );
        assert_eq!(create.policy(), setup.role.policy());

        let again = setup.plan(setup.request(1)?)?;
        assert_eq!(again.sandbox_name(), plan.sandbox_name());
        assert_eq!(again.draft().labels(), plan.draft().labels());
        assert_eq!(again.files(), plan.files());
        assert_eq!(
            again
                .draft()
                .complete(&built, jackin_core::DockerLabel::Off)
                .labels(),
            create.labels()
        );
        Ok(())
    })
    .await
}
