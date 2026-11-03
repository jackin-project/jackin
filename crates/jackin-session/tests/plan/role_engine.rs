// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_openshell::RuleKey;
use jackin_session::RoleError;
use jackin_testkit::RoleFixture;

use super::roles::{HEADER, rules, scope};
use super::support::{TestResult, guard};

async fn findings(policy: &str) -> Result<Vec<RoleError>, Box<dyn std::error::Error>> {
    let role = RoleFixture::new("sentinel")?.policy_yaml(policy);
    role.commit().await?;
    Ok(scope()?.catalog.findings(role.root()).await)
}

fn keys(found: &[RoleError]) -> Option<Vec<String>> {
    match found {
        [RoleError::EngineHost(rules)] => Some(rules.iter().map(RuleKey::to_string).collect()),
        _ => None,
    }
}

#[tokio::test]
async fn a_role_policy_that_names_the_engine_host_on_any_port_is_refused_with_every_rule_listed()
-> TestResult {
    guard(async {
        for (host, port) in [
            ("host.openshell.internal", 443),
            ("host.openshell.internal", 2376),
            ("host.openshell.internal", 80),
            ("HOST.OpenShell.Internal", 2376),
            ("*.openshell.internal", 443),
        ] {
            let found = findings(&rules(&[("engine", host, port, "/usr/bin/curl")])).await?;
            assert_eq!(keys(&found), Some(vec!["engine".to_owned()]), "{host}:{port}");
            let shown = found[0].to_string();
            assert!(shown.contains("engine") && shown.contains("host.openshell.internal"));
        }
        let two = format!(
            "{HEADER}network_policies:\n  zeta:\n    name: zeta\n    endpoints:\n      - host: \"example.com\"\n        port: 443\n      - host: \"host.openshell.internal\"\n        port: 9\n    binaries:\n      - path: /usr/bin/curl\n  alpha:\n    name: alpha\n    endpoints:\n      - host: \"*.openshell.internal\"\n        port: 1\n    binaries:\n      - path: /usr/bin/curl\n  fine:\n    name: fine\n    endpoints:\n      - host: \"github.com\"\n        port: 443\n    binaries:\n      - path: /usr/bin/git\n"
        );
        let found = findings(&two).await?;
        assert_eq!(keys(&found), Some(vec!["alpha".to_owned(), "zeta".to_owned()]));
        for host in [
            "host.openshell.internal.example.com",
            "openshell.internal",
            "example.com",
        ] {
            let found = findings(&rules(&[("other", host, 443, "/usr/bin/curl")])).await?;
            assert!(found.is_empty(), "{host}: {found:?}");
        }
        let none = format!("{HEADER}network_policies: {{}}\n");
        assert!(findings(&none).await?.is_empty());
        Ok(())
    })
    .await
}
