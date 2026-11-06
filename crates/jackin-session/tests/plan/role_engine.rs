// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::RoleError;
use jackin_testkit::RoleFixture;

use super::roles::{HEADER, rules, scope};
use super::support::{TestResult, guard};

const OPENSHELL: &str = "host.openshell.internal";
const CONTAINERS: &str = "host.containers.internal";
const DOCKER: &str = "host.docker.internal";
const ALIASES: [&str; 3] = [OPENSHELL, CONTAINERS, DOCKER];

async fn findings(policy: &str) -> Result<Vec<RoleError>, Box<dyn std::error::Error>> {
    let role = RoleFixture::new("sentinel")?.policy_yaml(policy);
    role.commit().await?;
    Ok(scope()?
        .catalog
        .findings(role.root(), &tokio_util::sync::CancellationToken::new())
        .await)
}

fn keys(found: &[RoleError]) -> Option<Vec<String>> {
    match found {
        [RoleError::EngineHost(violations)] => Some(
            violations
                .iter()
                .map(|found| found.rule.to_string())
                .collect(),
        ),
        _ => None,
    }
}

fn endpoint(key: &str, host: &str, port_lines: &str) -> String {
    format!(
        "{HEADER}network_policies:\n  {key}:\n    name: {key}\n    endpoints:\n      - host: \"{host}\"\n        {port_lines}\n    binaries:\n      - path: /usr/bin/curl\n"
    )
}

#[tokio::test]
async fn a_role_policy_that_names_a_host_gateway_alias_is_refused_with_the_alias_found()
-> TestResult {
    guard(async {
        let mut table: Vec<(String, &str, Vec<&str>)> = Vec::new();
        for alias in ALIASES {
            table.push((alias.to_owned(), "port: 443", vec![alias]));
            table.push((alias.to_owned(), "ports: [2375, 8080]", vec![alias]));
        }
        for (host, found) in [
            ("HOST.Docker.Internal", vec![DOCKER]),
            ("HOST.OpenShell.Internal", vec![OPENSHELL]),
            ("*.docker.internal", vec![DOCKER]),
            ("**.docker.internal", vec![DOCKER]),
            ("*.containers.internal", vec![CONTAINERS]),
            ("*.openshell.internal", vec![OPENSHELL]),
            ("*ost.docker.internal", vec![DOCKER]),
            ("host.*.internal", vec![OPENSHELL, CONTAINERS, DOCKER]),
        ] {
            table.push((host.to_owned(), "port: 2376", found));
        }
        for (host, port_lines, expected) in table {
            let found = findings(&endpoint("engine", &host, port_lines)).await?;
            assert_eq!(
                keys(&found),
                Some(vec!["engine".to_owned()]),
                "{host} {port_lines}"
            );
            let shown = found[0].to_string();
            assert!(shown.contains("engine"), "{shown}");
            for alias in ALIASES {
                assert_eq!(
                    shown.contains(alias),
                    expected.contains(&alias),
                    "{host}: {alias} in {shown}"
                );
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn every_refused_rule_is_listed_in_key_order_each_with_its_own_alias() -> TestResult {
    guard(async {
        let two = format!(
            "{HEADER}network_policies:\n  zeta:\n    name: zeta\n    endpoints:\n      - host: \"example.com\"\n        port: 443\n      - host: \"host.docker.internal\"\n        port: 9\n    binaries:\n      - path: /usr/bin/curl\n  alpha:\n    name: alpha\n    endpoints:\n      - host: \"*.containers.internal\"\n        port: 1\n    binaries:\n      - path: /usr/bin/curl\n  fine:\n    name: fine\n    endpoints:\n      - host: \"github.com\"\n        port: 443\n    binaries:\n      - path: /usr/bin/git\n"
        );
        let found = findings(&two).await?;
        assert_eq!(keys(&found), Some(vec!["alpha".to_owned(), "zeta".to_owned()]));
        let shown = found[0].to_string();
        assert!(shown.contains("alpha (host.containers.internal)"), "{shown}");
        assert!(shown.contains("zeta (host.docker.internal)"), "{shown}");
        assert!(!shown.contains(OPENSHELL), "{shown}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_trailing_dot_a_tld_wildcard_and_a_later_label_wildcard_are_refused_for_another_reason()
-> TestResult {
    guard(async {
        for host in ["host.docker.internal.", "*.internal", "host.dock*.internal"] {
            let found = findings(&endpoint("other", host, "port: 443")).await?;
            assert!(!found.is_empty(), "{host}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn hosts_that_only_look_like_an_alias_are_accepted() -> TestResult {
    guard(async {
        for host in [
            "host.docker.internal.example.com",
            "evil.host.docker.internal",
            "docker.internal",
            "myhost.docker.internals.org",
            "host-docker-internal.example",
            "host.openshell.internal.evil.com",
            "evil.host.openshell.internal",
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
