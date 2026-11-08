// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, Provider};
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, built_image, guard, login_account};

fn role_policy(read_only: &[&str]) -> String {
    let entries: String = read_only
        .iter()
        .map(|path| format!("    - {path}\n"))
        .collect();
    format!(
        "version: 1\nfilesystem_policy:\n  include_workdir: true\n  read_only:\n{entries}  read_write:\n    - /tmp\n    - /sandbox\nlandlock:\n  compatibility: hard_requirement\nprocess:\n  run_as_user: sandbox\n  run_as_group: sandbox\nnetwork_policies:\n  github:\n    name: github\n    endpoints:\n      - host: github.com\n        port: 443\n    binaries:\n      - path: /usr/bin/git\n"
    )
}

fn read_only_entries(
    policy: &jackin_openshell::PolicyDocument,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let rendered = policy.render()?;
    let value: noyalib::Value = noyalib::from_str(&rendered)?;
    Ok(value
        .get("filesystem_policy")
        .and_then(|filesystem| filesystem.get("read_only"))
        .and_then(noyalib::Value::as_sequence)
        .map(|items| {
            items
                .iter()
                .filter_map(noyalib::Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default())
}

async fn setup(
    role: RoleFixture,
    zai_account: bool,
) -> Result<super::support::Setup, Box<dyn std::error::Error>> {
    let mut spec = Spec::new(role);
    if zai_account {
        spec.accounts = format!(
            "{}[accounts.zai]\nprovider = \"zai\"\ncredential = {{ source = \"env\", var = \"ZAI_API_KEY\" }}\n",
            login_account("work")
        );
    }
    spec.build().await
}

#[tokio::test]
async fn supported_tuples_launch_with_the_role_read_only_and_no_added_root() -> TestResult {
    guard(async {
        let cases = [
            (
                RoleFixture::new("sentinel")?,
                Agent::Codex,
                Provider::Codex,
                false,
            ),
            (
                RoleFixture::new("sentinel")?.omp(),
                Agent::Omp,
                Provider::Zai,
                true,
            ),
        ];
        for (role, agent, provider, zai_account) in cases {
            let setup = setup(
                role.policy_yaml(&role_policy(&["/bin", "/usr"])),
                zai_account,
            )
            .await?;
            let plan = setup.plan(setup.request(1)?.tuple(agent, provider))?;
            let created = plan.draft().complete(
                &built_image(&setup.role, '4')?,
                jackin_core::DockerLabel::Off,
            );
            assert_eq!(
                read_only_entries(created.policy())?,
                ["/bin", "/usr"],
                "{agent:?}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn role_granted_root_read_only_reaches_supported_launches_once() -> TestResult {
    guard(async {
        let cases = [
            (
                RoleFixture::new("sentinel")?,
                Agent::Codex,
                Provider::Codex,
                false,
            ),
            (
                RoleFixture::new("sentinel")?.omp(),
                Agent::Omp,
                Provider::Zai,
                true,
            ),
        ];
        for (role, agent, provider, zai_account) in cases {
            let setup = setup(
                role.policy_yaml(&role_policy(&["/bin", "/usr", "/"])),
                zai_account,
            )
            .await?;
            let plan = setup.plan(setup.request(2)?.tuple(agent, provider))?;
            let created = plan.draft().complete(
                &built_image(&setup.role, '4')?,
                jackin_core::DockerLabel::Off,
            );
            assert_eq!(
                read_only_entries(created.policy())?,
                ["/bin", "/usr", "/"],
                "{agent:?}"
            );
        }
        Ok(())
    })
    .await
}
