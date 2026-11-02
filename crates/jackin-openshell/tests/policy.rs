// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::unwrap_used,
    reason = "test helpers fail the test by panicking"
)]

use std::collections::BTreeSet;
use std::num::NonZeroU16;

use jackin_core::{Agent, ProviderName};
use jackin_openshell::{
    ClosureViolation, EndpointProtocol, HostPattern, NetworkRule, PolicyDocument, PolicyError,
    ProviderProfile, RuleKey, TlsMode,
};

fn rule_doc(rules: &[(&str, &str, u16)]) -> String {
    let mut text = String::from("version: 1\nnetwork_policies:\n");
    for (key, host, port) in rules {
        text.push_str(&format!(
            "  {key}:\n    name: {key}\n    endpoints:\n      - host: '{host}'\n        port: {port}\n    binaries:\n      - path: /usr/bin/curl\n"
        ));
    }
    text
}

fn profile() -> ProviderProfile {
    ProviderProfile::parse(&Agent::Codex.spec().openshell.chatgpt_profile).unwrap()
}

fn violation(rule: &str, host: &str, provider: &str) -> ClosureViolation {
    ClosureViolation::Overlap {
        rule: key_of(rule),
        host: HostPattern::parse(host).unwrap(),
        provider_host: HostPattern::parse(provider).unwrap(),
    }
}

fn key_of(text: &str) -> RuleKey {
    PolicyDocument::parse(&rule_doc(&[(text, "example.org", 443)]))
        .unwrap()
        .network_rules()
        .remove(0)
        .key
}

#[test]
fn authored_policies_reject_reserved_keys_while_the_live_parse_accepts_them() {
    let reserved = rule_doc(&[("_provider_x", "api.example.com", 443)]);
    assert!(matches!(
        PolicyDocument::parse_authored(&reserved),
        Err(PolicyError::ReservedRuleName(ref key)) if key == "_provider_x"
    ));
    let live = PolicyDocument::parse(&reserved).unwrap();
    assert_eq!(live.network_rules()[0].key.as_str(), "_provider_x");
    assert!(live.network_rules()[0].key.is_reserved());
    assert!(
        !NetworkRule::docker_engine(NonZeroU16::new(1).unwrap())
            .key
            .is_reserved()
    );

    assert!(matches!(
        PolicyDocument::parse_authored("version: 1\nnetwork_policies: 3\n"),
        Err(PolicyError::NotAMapping { ref key }) if key == "network_policies"
    ));
    for port in [0_i64, 70_000] {
        let text = format!(
            "version: 1\nnetwork_policies:\n  r1:\n    name: r1\n    endpoints:\n      - host: api.example.com\n        port: {port}\n"
        );
        assert!(matches!(
            PolicyDocument::parse_authored(&text),
            Err(PolicyError::InvalidPort { ref rule, port: got }) if rule == "r1" && got == port
        ));
    }
    let empty_host = "version: 1\nnetwork_policies:\n  r1:\n    name: r1\n    endpoints:\n      - host: ''\n        port: 443\n";
    assert!(matches!(
        PolicyDocument::parse_authored(empty_host),
        Err(PolicyError::InvalidHost(_))
    ));
    assert!(matches!(
        PolicyDocument::parse_authored("a: [unclosed"),
        Err(PolicyError::Syntax { .. })
    ));
}

#[test]
fn closure_violations_follow_d10_for_hosts_ports_and_binaries() {
    let profile = profile();
    let hosts: BTreeSet<&str> = profile
        .endpoints
        .iter()
        .map(|endpoint| endpoint.host.as_str())
        .collect();
    assert!(
        [
            "api.openai.com",
            "auth.openai.com",
            "chatgpt.com",
            "ab.chatgpt.com"
        ]
        .iter()
        .all(|host| hosts.contains(host))
    );
    let provider = ProviderName::parse("jackin-work-codex").unwrap();
    let generated = RuleKey::provider_rule(&provider);
    let generated_name = generated.as_str().to_owned();
    assert_eq!(generated_name, "_provider_jackin_work_codex");

    let only_generated =
        PolicyDocument::parse(&rule_doc(&[(&generated_name, "api.openai.com", 443)])).unwrap();
    assert!(
        only_generated
            .closure_violations(&profile, std::slice::from_ref(&generated))
            .is_empty()
    );
    assert_eq!(
        only_generated.closure_violations(&profile, &[]),
        vec![violation(
            &generated_name,
            "api.openai.com",
            "api.openai.com"
        )]
    );

    let wildcard =
        PolicyDocument::parse_authored(&rule_doc(&[("r1", "*.openai.com", 443)])).unwrap();
    assert_eq!(
        wildcard.closure_violations(&profile, &[]),
        vec![
            violation("r1", "*.openai.com", "api.openai.com"),
            violation("r1", "*.openai.com", "auth.openai.com"),
        ]
    );

    let other_port =
        PolicyDocument::parse_authored(&rule_doc(&[("r2", "api.openai.com", 8080)])).unwrap();
    assert_eq!(
        other_port.closure_violations(&profile, &[]),
        vec![violation("r2", "api.openai.com", "api.openai.com")]
    );

    let everything = PolicyDocument::parse_authored(&rule_doc(&[("r3", "**", 443)])).unwrap();
    let violations = everything.closure_violations(&profile, &[]);
    assert_eq!(violations.len(), 4);
    assert!(violations.iter().all(|found| matches!(
        found,
        ClosureViolation::Overlap { rule, host, .. } if rule.as_str() == "r3" && host.as_str() == "**"
    )));

    let foreign =
        PolicyDocument::parse(&rule_doc(&[("_provider_other", "api.openai.com", 443)])).unwrap();
    assert_eq!(
        foreign.closure_violations(&profile, std::slice::from_ref(&generated)),
        vec![violation(
            "_provider_other",
            "api.openai.com",
            "api.openai.com"
        )]
    );

    let unrelated = PolicyDocument::parse_authored(&rule_doc(&[
        ("r4", "example.com", 443),
        ("r5", "*.example.com", 443),
    ]))
    .unwrap();
    assert!(unrelated.closure_violations(&profile, &[]).is_empty());
}

#[test]
fn closure_check_reports_hosts_it_cannot_parse_instead_of_skipping_them() {
    let profile = profile();
    let provider = ProviderName::parse("jackin-work-codex").unwrap();
    let generated = RuleKey::provider_rule(&provider);
    let generated_name = generated.as_str().to_owned();
    let document = PolicyDocument::parse(&rule_doc(&[
        ("r6", "api.openai.com/v1", 443),
        ("r7", "", 443),
        ("r8", "example.com", 0),
        (&generated_name, "bad host!", 443),
        ("r10", "example.net", 443),
    ]))
    .unwrap();
    let found: BTreeSet<ClosureViolation> = document
        .closure_violations(&profile, std::slice::from_ref(&generated))
        .into_iter()
        .collect();
    assert_eq!(
        found,
        BTreeSet::from([
            ClosureViolation::UnparsableHost {
                rule: key_of("r6"),
                host: "api.openai.com/v1".to_owned(),
            },
            ClosureViolation::UnparsableHost {
                rule: key_of("r7"),
                host: String::new(),
            },
            ClosureViolation::InvalidPort {
                rule: key_of("r8"),
                port: 0,
            },
        ])
    );
}

const BASE: &str = "version: 1
filesystem_policy:
  include_workdir: true
  read_only:
    - /bin
    - /usr
  read_write:
    - /tmp
    - /sandbox
landlock:
  compatibility: best_effort
process:
  run_as_user: sandbox
  run_as_group: sandbox
network_policies:
  existing:
    name: existing
    endpoints:
      - host: api.example.com
        port: 443
    binaries:
      - path: /usr/bin/curl
";

#[test]
fn with_rule_adds_exactly_one_docker_rule_and_keeps_every_other_node() {
    let original = PolicyDocument::parse_authored(BASE).unwrap();
    let engine = NetworkRule::docker_engine(NonZeroU16::new(12376).unwrap());
    let updated = original.with_rule(engine.clone()).unwrap();

    let before: noyalib::Value = noyalib::from_str(BASE).unwrap();
    let after: noyalib::Value = noyalib::from_str(&updated.render()).unwrap();
    let (before, after) = (before.as_mapping().unwrap(), after.as_mapping().unwrap());
    for (key, value) in before {
        if key != "network_policies" {
            assert_eq!(after.get(key), Some(value), "{key:?}");
        }
    }
    assert_eq!(after.len(), before.len());
    let keys = |mapping: &noyalib::Mapping| -> BTreeSet<String> {
        mapping
            .get("network_policies")
            .and_then(noyalib::Value::as_mapping)
            .unwrap()
            .keys()
            .cloned()
            .collect()
    };
    let mut expected = keys(before);
    expected.insert("docker_engine".to_owned());
    assert_eq!(keys(after), expected);

    let added = updated
        .network_rules()
        .into_iter()
        .find(|rule| rule.key.as_str() == "docker_engine")
        .unwrap();
    assert_eq!(added, engine);
    assert_eq!(added.endpoints[0].host.as_str(), "host.openshell.internal");
    assert_eq!(added.endpoints[0].port, 12376);
    assert_eq!(added.endpoints[0].protocol, EndpointProtocol::Tcp);
    assert_eq!(added.endpoints[0].tls, TlsMode::Skip);
    assert_eq!(added.binaries[0].as_str(), "/usr/local/bin/docker");

    assert!(matches!(
        updated.with_rule(engine),
        Err(PolicyError::DuplicateRule(ref key)) if key.as_str() == "docker_engine"
    ));
    assert_eq!(
        PolicyDocument::parse(&updated.render())
            .unwrap()
            .network_rules(),
        updated.network_rules()
    );
}
