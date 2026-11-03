// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::unwrap_used,
    reason = "test helpers fail the test by panicking"
)]

use std::collections::BTreeSet;
use std::num::NonZeroU16;

use jackin_core::ProviderName;
use jackin_openshell::{
    ClosedPolicy, ClosedPolicyError, ClosureViolation, EndpointProtocol, HostPattern, NetworkRule,
    PolicyDocument, PolicyError, RuleKey, TlsMode,
};
use openshell_core::host_pattern::host_matches;
use openshell_policy::{
    PolicyViolation, parse_sandbox_policy, validate_and_canonicalize_sandbox_policy,
    validate_sandbox_policy,
};

mod common;

use common::{profile, rule_doc, with_header};

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
        PolicyDocument::parse_authored(&with_header("network_policies: 3\n")),
        Err(PolicyError::NotAMapping { ref key }) if key == "network_policies"
    ));
    for port in [0_i64, 70_000] {
        let text = with_header(&format!(
            "network_policies:\n  r1:\n    name: r1\n    endpoints:\n      - host: api.example.com\n        port: {port}\n"
        ));
        assert!(matches!(
            PolicyDocument::parse_authored(&text),
            Err(PolicyError::InvalidPort { ref rule, port: got }) if rule == "r1" && got == port
        ));
    }
    let empty_host = with_header(
        "network_policies:\n  r1:\n    name: r1\n    endpoints:\n      - host: ''\n        port: 443\n",
    );
    assert!(matches!(
        PolicyDocument::parse_authored(&empty_host),
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
        ("r6", "{api,x}.openai.com", 443),
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
                host: "{api,x}.openai.com".to_owned(),
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

const UNPARSABLE_FORMS: [&str; 6] = [
    "{chatgpt,x}.com",
    "chatgpt.{com,net}",
    "bad host",
    "a..b",
    "chatgpt.com.",
    "[unclosed.com",
];

const REFUSED_HOSTS: [&str; 33] = [
    "[^x]*hatgpt.com",
    "[^x]*.chatgpt.com",
    "[^x]*pi.openai.com",
    "*\\.openai.com",
    "a\\p*.openai.com",
    "[^x]*.openai.com",
    "[a]pi.openai.com",
    "a?i.openai.com",
    "[c]hatgpt.com",
    "[a-z]hatgpt.com",
    "chatgpt.co[m]",
    "api.openai.co[m]",
    "[d]hatgpt.com",
    "chatgpt.co[n]",
    "chatgpt.com!",
    "chat^gpt.com",
    "chat/gpt.com",
    "chat:gpt.com",
    "chat\u{200B}gpt.com",
    "\u{FF43}hatgpt.com",
    "\u{00E9}.example.com",
    "\u{4F8B}\u{3048}.example.com",
    "ab.chatgpt.co\u{212A}",
    "*\u{212A}.example.com",
    "a**.com",
    "**a.com",
    "a**b.com",
    "***.com",
    "*",
    "**",
    "*.com",
    "**.com",
    "x.a*.com",
];

fn assert_refused_in_both_parses(form: &str) {
    let profile = profile();
    let text = rule_doc(&[("r1", form, 443)]);
    assert!(
        matches!(
            PolicyDocument::parse_authored(&text),
            Err(PolicyError::InvalidHost(ref host)) if host == form
        ),
        "parse_authored must refuse {form:?}"
    );
    assert!(
        matches!(
            ClosedPolicy::parse(&text),
            Err(ClosedPolicyError::Policy(PolicyError::InvalidHost(_)))
        ),
        "ClosedPolicy::parse must refuse {form:?}"
    );
    assert!(
        matches!(
            HostPattern::parse(form),
            Err(PolicyError::InvalidHost(ref host)) if host == form
        ),
        "HostPattern::parse must refuse {form:?}"
    );
    let live = PolicyDocument::parse(&text).unwrap();
    assert_eq!(
        live.closure_violations(&profile, &[]),
        vec![ClosureViolation::UnparsableHost {
            rule: key_of("r1"),
            host: form.to_owned(),
        }],
        "live closure_violations of {form:?}"
    );
}

#[test]
fn host_labels_outside_the_dns_alphabet_are_refused_in_both_parses() {
    for form in REFUSED_HOSTS {
        assert_refused_in_both_parses(form);
    }
}

#[test]
fn hosts_the_upstream_matcher_rejects_are_unparsable_in_both_parses() {
    for form in UNPARSABLE_FORMS {
        assert!(host_matches(form, "chatgpt.com").is_err(), "{form}");
        assert_refused_in_both_parses(form);
    }
}

#[test]
fn a_closed_policy_exists_only_for_an_authored_policy_that_passes_the_closure_check() {
    let clean = rule_doc(&[("r1", "example.com", 443), ("r2", "*.example.org", 8443)]);
    let closed = ClosedPolicy::parse(&clean).unwrap();
    assert_eq!(
        closed.document().network_rules(),
        PolicyDocument::parse_authored(&clean)
            .unwrap()
            .network_rules()
    );

    let overlapping = ClosedPolicy::parse(&rule_doc(&[("r1", "*gpt.com", 443)])).unwrap_err();
    assert!(
        matches!(
            overlapping,
            ClosedPolicyError::Violations(ref found)
                if found == &vec![violation("r1", "*gpt.com", "chatgpt.com")]
        ),
        "{overlapping:?}"
    );
    let bracket = ClosedPolicy::parse(&rule_doc(&[("r1", "[c]hatgpt.com", 443)])).unwrap_err();
    assert!(
        matches!(
            bracket,
            ClosedPolicyError::Policy(PolicyError::InvalidHost(ref host)) if host == "[c]hatgpt.com"
        ),
        "{bracket:?}"
    );
    let reserved =
        ClosedPolicy::parse(&rule_doc(&[("_provider_x", "example.com", 443)])).unwrap_err();
    assert!(
        matches!(
            reserved,
            ClosedPolicyError::Policy(PolicyError::ReservedRuleName(ref key)) if key == "_provider_x"
        ),
        "{reserved:?}"
    );
    let port_zero = ClosedPolicy::parse(&rule_doc(&[("r1", "example.com", 0)])).unwrap_err();
    assert!(
        matches!(
            port_zero,
            ClosedPolicyError::Policy(PolicyError::InvalidPort { ref rule, port: 0 }) if rule == "r1"
        ),
        "{port_zero:?}"
    );
    let unparsable = ClosedPolicy::parse(&rule_doc(&[("r1", "{a,b}.com", 443)])).unwrap_err();
    assert!(
        matches!(
            unparsable,
            ClosedPolicyError::Policy(PolicyError::InvalidHost(_))
        ),
        "{unparsable:?}"
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
  compatibility: hard_requirement
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
    let after: noyalib::Value = noyalib::from_str(&updated.render().unwrap()).unwrap();
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

    let rendered = updated.render().unwrap();
    let reparsed = ClosedPolicy::parse(&rendered).unwrap();
    assert_eq!(reparsed.document().network_rules(), updated.network_rules());
    let rendered: noyalib::Value = noyalib::from_str(&rendered).unwrap();
    assert_eq!(
        rendered
            .get("landlock")
            .and_then(|landlock| landlock.get("compatibility"))
            .and_then(noyalib::Value::as_str),
        Some("hard_requirement")
    );

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
        PolicyDocument::parse(&updated.render().unwrap())
            .unwrap()
            .network_rules(),
        updated.network_rules()
    );
}

const ACCEPTED_HOSTS: [&str; 12] = [
    "github.com",
    "*.github.com",
    "registry.npmjs.org",
    "static.crates.io",
    "files.pythonhosted.org",
    "host-1_a.example.com",
    "**.example.org",
    "api-*.example.com",
    "*-api.example.com",
    "a*b.example.com",
    "host.openshell.internal",
    "x.*.example.com",
];

#[test]
fn hosts_inside_the_alphabet_keep_their_d10_outcomes() {
    let profile = profile();
    for host in ACCEPTED_HOSTS {
        let document = PolicyDocument::parse_authored(&rule_doc(&[("r1", host, 443)])).unwrap();
        assert_eq!(
            document.closure_violations(&profile, &[]),
            Vec::new(),
            "{host}"
        );
        assert_eq!(HostPattern::parse(host).unwrap().as_str(), host);
    }
    assert_eq!(
        HostPattern::parse("Example.COM").unwrap().as_str(),
        "example.com"
    );

    let covers: [(&str, &[&str]); 7] = [
        ("API.OPENAI.COM", &["api.openai.com"]),
        ("ab.chatgpt.com", &["ab.chatgpt.com"]),
        ("**.openai.com", &["api.openai.com", "auth.openai.com"]),
        ("*gpt.com", &["chatgpt.com"]),
        ("chat*.com", &["chatgpt.com"]),
        ("*.chatgpt.com", &["ab.chatgpt.com"]),
        ("a*i.openai.com", &["api.openai.com"]),
    ];
    for (host, expected) in covers {
        let document = PolicyDocument::parse_authored(&rule_doc(&[("r1", host, 443)])).unwrap();
        let found: BTreeSet<String> = document
            .closure_violations(&profile, &[])
            .into_iter()
            .map(|found| match found {
                ClosureViolation::Overlap {
                    rule,
                    host: got,
                    provider_host,
                } => {
                    assert_eq!(rule.as_str(), "r1", "{host}");
                    assert_eq!(got.as_str(), host.to_ascii_lowercase(), "{host}");
                    provider_host.as_str().to_owned()
                }
                other => panic!("{host}: {other:?}"),
            })
            .collect();
        let expected: BTreeSet<String> = expected.iter().map(|host| (*host).to_owned()).collect();
        assert_eq!(found, expected, "{host}");
    }
}

const BYTES_THAT_SHAPE_LABELS: &[u8] = b".*";

const PLACEMENTS: [&str; 7] = [
    "{c}.example.com",
    "a{c}.example.com",
    "{c}a.example.com",
    "a{c}b.example.com",
    "*{c}.example.com",
    "x.a{c}b.example.com",
    "x.y.a{c}",
];

#[test]
fn every_ascii_byte_outside_the_label_alphabet_is_refused_in_any_position() {
    let mut refused = 0;
    let mut accepted = 0;
    for byte in 0x00_u8..=0x7F {
        if BYTES_THAT_SHAPE_LABELS.contains(&byte) {
            continue;
        }
        let inside = byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-';
        for placement in PLACEMENTS {
            let host = placement.replace("{c}", &char::from(byte).to_string());
            let parsed = HostPattern::parse(&host);
            if inside {
                accepted += 1;
                assert!(parsed.is_ok(), "{host:?} should be accepted");
            } else {
                refused += 1;
                assert!(
                    matches!(parsed, Err(PolicyError::InvalidHost(ref got)) if *got == host),
                    "{host:?} should be refused"
                );
            }
        }
    }
    assert_eq!(accepted, 64 * PLACEMENTS.len());
    assert_eq!(refused, 62 * PLACEMENTS.len());
}

fn label_sets() -> Vec<String> {
    let labels = ["a", "*", "**", "a*", "*a", "a**"];
    let mut hosts: Vec<String> = Vec::new();
    for count in 1..=3_u32 {
        for index in 0..labels.len().pow(count) {
            let mut rest = index;
            let mut parts = Vec::new();
            for _ in 0..count {
                parts.push(labels[rest % labels.len()]);
                rest /= labels.len();
            }
            hosts.push(parts.join("."));
        }
    }
    for extra in [
        "*", "**", "*.com", "**.com", "**.org", "x.a*.com", "x.**.com", "a.b*",
    ] {
        hosts.push(extra.to_owned());
    }
    hosts
}

fn gateway_refuses_shape(host: &str) -> bool {
    let proto = parse_sandbox_policy(&rule_doc(&[("r1", host, 443)])).unwrap();
    validate_sandbox_policy(&proto).is_err_and(|found| {
        found.iter().any(|violation| {
            matches!(
                violation,
                PolicyViolation::TldWildcard { .. } | PolicyViolation::InvalidHostWildcard { .. }
            )
        })
    })
}

#[test]
fn role_check_refuses_every_host_shape_the_gateway_refuses() {
    let profile = profile();
    assert!(!gateway_refuses_shape("example.com"));
    let mut both_refused = 0;
    let mut both_accepted = 0;
    for host in label_sets() {
        let gateway = gateway_refuses_shape(&host);
        let text = rule_doc(&[("r1", &host, 443)]);
        let jackin = PolicyDocument::parse_authored(&text).is_err();
        assert_eq!(
            jackin, gateway,
            "host {host}: gateway refuses {gateway}, jackin refuses {jackin}"
        );
        if jackin {
            both_refused += 1;
            assert!(matches!(
                PolicyDocument::parse_authored(&text),
                Err(PolicyError::InvalidHost(ref got)) if *got == host
            ));
        } else {
            both_accepted += 1;
        }
        let live = PolicyDocument::parse(&text).unwrap();
        let unreadable = live
            .closure_violations(&profile, &[])
            .iter()
            .any(|found| matches!(found, ClosureViolation::UnparsableHost { .. }));
        assert_eq!(unreadable, gateway, "live parse of {host}");
    }
    assert!(
        both_refused >= 237 && both_accepted >= 29,
        "refused {both_refused} accepted {both_accepted}"
    );
}

const NETWORK: &str = "network_policies:
  r1:
    name: r1
    endpoints:
      - host: example.com
        port: 443
    binaries:
      - path: /usr/bin/curl
";

const PATHS: &str = "filesystem_policy:\n  read_only:\n    - /bin\n";

fn landlock_doc(landlock: &str, network: &str) -> String {
    format!("version: 1\n{PATHS}{landlock}{network}")
}

fn assert_landlock_refused(text: &str, found: Option<&str>) {
    assert!(
        matches!(
            PolicyDocument::parse_authored(text),
            Err(PolicyError::LandlockMode { found: ref got }) if got.as_deref() == found
        ),
        "parse_authored accepted:\n{text}"
    );
    assert!(
        matches!(
            ClosedPolicy::parse(text),
            Err(ClosedPolicyError::Policy(PolicyError::LandlockMode { found: ref got }))
                if got.as_deref() == found
        ),
        "{text}"
    );
}

#[test]
fn an_authored_policy_must_set_landlock_hard_requirement() {
    assert_landlock_refused(
        &landlock_doc("landlock:\n  compatibility: best_effort\n", NETWORK),
        Some("best_effort"),
    );
    assert_landlock_refused(&landlock_doc("", NETWORK), None);
    assert_landlock_refused(
        &landlock_doc("landlock: {}\n", NETWORK),
        Some("best_effort"),
    );
    for syntax in [
        "landlock:\n",
        "landlock:\n  compatibility: ''\n",
        "landlock:\n  compatibility: Hard_Requirement\n",
        "landlock:\n  compatibility: bogus\n",
    ] {
        let text = landlock_doc(syntax, NETWORK);
        assert!(
            matches!(
                PolicyDocument::parse_authored(&text),
                Err(PolicyError::Syntax { .. })
            ),
            "{syntax}"
        );
    }
    let hard = "landlock:\n  compatibility: hard_requirement\n";
    PolicyDocument::parse_authored(&landlock_doc(hard, NETWORK)).unwrap();
    PolicyDocument::parse_authored(&landlock_doc(hard, "")).unwrap();
    ClosedPolicy::parse(&landlock_doc(hard, NETWORK)).unwrap();

    let message = PolicyError::LandlockMode { found: None }.to_string();
    assert!(message.contains("landlock.compatibility") && message.contains("hard_requirement"));
}

#[test]
fn the_live_parse_accepts_any_landlock_mode_and_any_filesystem_lists() {
    for landlock in [
        "landlock:\n  compatibility: best_effort\n",
        "",
        "landlock:\n  compatibility: hard_requirement\n",
    ] {
        PolicyDocument::parse(&landlock_doc(landlock, NETWORK)).unwrap();
    }
    for paths in [
        "",
        "filesystem_policy:\n  read_only: []\n  read_write: []\n",
    ] {
        let text = format!("version: 1\n{paths}landlock:\n  compatibility: best_effort\n{NETWORK}");
        PolicyDocument::parse(&text).unwrap();
    }
}

#[test]
fn an_authored_policy_with_no_filesystem_paths_is_refused() {
    let hard = "landlock:\n  compatibility: hard_requirement\n";
    let doc = |paths: &str| format!("version: 1\n{paths}{hard}{NETWORK}");
    for paths in [
        "",
        "filesystem_policy: {}\n",
        "filesystem_policy:\n  read_only: []\n  read_write: []\n",
        "filesystem_policy:\n  include_workdir: true\n",
    ] {
        let text = doc(paths);
        assert!(
            matches!(
                PolicyDocument::parse_authored(&text),
                Err(PolicyError::NoFilesystemPaths)
            ),
            "parse_authored accepted the filesystem section {paths:?}"
        );
        assert!(
            matches!(
                ClosedPolicy::parse(&text),
                Err(ClosedPolicyError::Policy(PolicyError::NoFilesystemPaths))
            ),
            "ClosedPolicy::parse accepted the filesystem section {paths:?}"
        );
    }
    for paths in [
        "filesystem_policy:\n  read_only:\n    - /bin\n",
        "filesystem_policy:\n  read_write:\n    - /tmp\n",
        "filesystem_policy:\n  include_workdir: true\n  read_only:\n    - /bin\n  read_write:\n    - /tmp\n",
    ] {
        PolicyDocument::parse_authored(&doc(paths)).unwrap();
    }
    let message = PolicyError::NoFilesystemPaths.to_string();
    assert!(message.contains("read_only") && message.contains("read_write"));
}

const HARD: &str = "landlock:\n  compatibility: hard_requirement\n";

fn path_doc(list: &str, path: &str) -> String {
    format!("version: 1\nfilesystem_policy:\n  {list}:\n    - '{path}'\n{HARD}")
}

fn gateway_refuses_text(text: &str) -> bool {
    let proto = parse_sandbox_policy(text).unwrap();
    validate_sandbox_policy(&proto).is_err()
}

fn is_root_form(path: &str) -> bool {
    path.starts_with('/')
        && path
            .split('/')
            .all(|piece| piece.is_empty() || piece == ".")
}

fn alias_form(path: &str) -> bool {
    path.starts_with('/')
        && match path
            .split('/')
            .find(|piece| !piece.is_empty() && *piece != ".")
        {
            None => true,
            Some(piece) => piece == "proc" || piece == "sys",
        }
}

fn strings_over(tokens: &[&str], longest: usize) -> Vec<String> {
    let mut all = Vec::new();
    let mut layer = vec![String::new()];
    for _ in 0..longest {
        layer = layer
            .iter()
            .flat_map(|head| tokens.iter().map(move |token| format!("{head}{token}")))
            .collect();
        all.extend(layer.iter().cloned());
    }
    all
}

#[test]
fn an_authored_read_write_list_refuses_every_path_that_names_the_root_or_aliases_it() {
    for path in ["/", "//", "/.", "/./", "/./.", "//.", "/.//."] {
        let text = path_doc("read_write", path);
        assert!(
            matches!(
                PolicyDocument::parse_authored(&text),
                Err(PolicyError::ReadWriteRoot { path: ref got }) if got == path
            ),
            "read_write {path}"
        );
        assert!(
            matches!(
                ClosedPolicy::parse(&text),
                Err(ClosedPolicyError::Policy(PolicyError::ReadWriteRoot { .. }))
            ),
            "closed read_write {path}"
        );
    }
    for path in [
        "/proc",
        "/proc/",
        "//proc/x",
        "/./sys",
        "/proc/self/root",
        "/sys",
        "/sys/kernel",
    ] {
        assert!(
            matches!(
                PolicyDocument::parse_authored(&path_doc("read_write", path)),
                Err(PolicyError::ReadWriteAlias { path: ref got }) if got == path
            ),
            "read_write {path}"
        );
    }
    assert!(matches!(
        PolicyDocument::parse_authored(&path_doc("read_write", "/usr/..")),
        Err(PolicyError::Gateway { .. })
    ));
    for path in [
        "/tmp", "/tmp/", "/tmp/.", "/./tmp", "/.hidden", "/..x", "/...", "/a/./b", "/sandbox",
        "/procfs", "/proc2", "/sysroot", "/system", "/a/proc", "/PROC",
    ] {
        PolicyDocument::parse_authored(&path_doc("read_write", path))
            .unwrap_or_else(|error| panic!("read_write {path}: {error}"));
    }
    for path in ["/", "//", "/.", "/./", "/proc", "/sys", "/tmp"] {
        PolicyDocument::parse_authored(&path_doc("read_only", path))
            .unwrap_or_else(|error| panic!("read_only {path}: {error}"));
    }
    assert!(matches!(
        PolicyDocument::parse_authored(&path_doc("read_only", "/usr/..")),
        Err(PolicyError::Gateway { .. })
    ));
}

#[test]
fn role_check_refuses_every_filesystem_path_shape_the_gateway_refuses() {
    let universe = strings_over(&["/", ".", "a"], 6);
    assert_eq!(universe.len(), 1092);
    let (mut gateway_only, mut lexical_only, mut accepted) = (0, 0, 0);
    for path in &universe {
        for list in ["read_write", "read_only"] {
            let text = path_doc(list, path);
            let gateway = gateway_refuses_text(&text);
            let lexical = list == "read_write" && is_root_form(path);
            let jackin = PolicyDocument::parse_authored(&text).is_err();
            assert_eq!(
                jackin,
                gateway || lexical,
                "{list} {path}: gateway refuses {gateway}, lexical root {lexical}, jackin refuses {jackin}"
            );
            match (gateway, lexical) {
                (true, _) => gateway_only += 1,
                (false, true) => lexical_only += 1,
                (false, false) => accepted += 1,
            }
        }
    }
    assert_eq!(
        (gateway_only, lexical_only, accepted),
        (1528, 26, 630),
        "gateway-refused, lexical-root-only and accepted path rows"
    );
}

#[test]
fn read_write_refuses_every_path_equal_or_below_proc_and_sys() {
    let universe = strings_over(&["/", ".", "proc", "sys", "a"], 4);
    assert_eq!(universe.len(), 780);
    let (mut refused, mut accepted) = (0, 0);
    for path in &universe {
        let text = path_doc("read_write", path);
        let expected = gateway_refuses_text(&text) || is_root_form(path) || alias_form(path);
        let jackin = PolicyDocument::parse_authored(&text).is_err();
        assert_eq!(jackin, expected, "read_write {path}");
        let read_only = PolicyDocument::parse_authored(&path_doc("read_only", path)).is_err();
        assert_eq!(
            read_only,
            gateway_refuses_text(&path_doc("read_only", path)),
            "read_only {path}"
        );
        if jackin {
            refused += 1;
        } else {
            accepted += 1;
        }
    }
    assert_eq!(
        (refused, accepted),
        (660, 120),
        "refused and accepted paths"
    );
}

#[test]
fn role_check_refuses_each_gateway_violation_class() {
    let paths = |count: usize| {
        (0..count).fold(String::new(), |mut text, index| {
            text.push_str(&format!("    - /p{index}\n"));
            text
        })
    };
    let fs = |list: &str| format!("filesystem_policy:\n  read_only:\n    - /bin\n{list}");
    let endpoint = |fields: &str| {
        format!(
            "network_policies:\n  r1:\n    name: r1\n    endpoints:\n      - host: example.com\n{fields}    binaries:\n      - path: /usr/bin/curl\n"
        )
    };
    let cases: Vec<(&str, String)> = vec![
        (
            "run_as_user root",
            format!(
                "version: 1\n{}{HARD}process:\n  run_as_user: root\n",
                fs("")
            ),
        ),
        (
            "run_as_user 0",
            format!("version: 1\n{}{HARD}process:\n  run_as_user: '0'\n", fs("")),
        ),
        (
            "run_as_group root",
            format!(
                "version: 1\n{}{HARD}process:\n  run_as_group: root\n",
                fs("")
            ),
        ),
        (
            "relative path",
            format!("version: 1\nfilesystem_policy:\n  read_only:\n    - bin\n{HARD}"),
        ),
        (
            "dotdot in read_only",
            format!("version: 1\nfilesystem_policy:\n  read_only:\n    - /usr/../etc\n{HARD}"),
        ),
        (
            "too many paths",
            format!(
                "version: 1\nfilesystem_policy:\n  read_only:\n{}{HARD}",
                paths(257)
            ),
        ),
        (
            "path too long",
            format!(
                "version: 1\nfilesystem_policy:\n  read_only:\n    - /{}\n{HARD}",
                "a".repeat(4100)
            ),
        ),
        (
            "no port",
            format!("version: 1\n{}{HARD}{}", fs(""), endpoint("")),
        ),
        (
            "unknown credential signing",
            format!(
                "version: 1\n{}{HARD}{}",
                fs(""),
                endpoint(
                    "        port: 443\n        credential_signing: bogus\n        signing_service: s3\n"
                )
            ),
        ),
        (
            "signing without service",
            format!(
                "version: 1\n{}{HARD}{}",
                fs(""),
                endpoint("        port: 443\n        credential_signing: sigv4\n")
            ),
        ),
        (
            "tld wildcard",
            format!(
                "version: 1\n{}{HARD}{}",
                fs(""),
                endpoint("        port: 443\n").replace("example.com", "'*.com'")
            ),
        ),
    ];
    for (label, text) in &cases {
        assert!(gateway_refuses_text(text), "{label}: the gateway accepts");
        assert!(
            matches!(
                PolicyDocument::parse_authored(text),
                Err(PolicyError::Gateway { .. } | PolicyError::InvalidHost(_))
            ),
            "{label}: parse_authored accepted"
        );
        PolicyDocument::parse(text).unwrap();
    }
    for identity in ["sandbox", "'1000'"] {
        let text = format!(
            "version: 1\n{}{HARD}process:\n  run_as_user: {identity}\n  run_as_group: sandbox\n",
            fs("")
        );
        PolicyDocument::parse_authored(&text).unwrap();
    }
}

#[test]
fn policy_errors_print_no_control_sequences() {
    for (list, path) in [
        ("read_only", "\\u001b[31mx"),
        ("read_write", "/\\u001b[31m/.."),
    ] {
        let text = format!("version: 1\nfilesystem_policy:\n  {list}:\n    - \"{path}\"\n{HARD}");
        let error = PolicyDocument::parse_authored(&text).unwrap_err();
        assert!(
            matches!(error, PolicyError::Gateway { .. }),
            "{list}: {error:?}"
        );
        let closed = ClosedPolicy::parse(&text).unwrap_err();
        for shown in [error.to_string(), closed.to_string()] {
            assert!(shown.contains("[31m"), "{shown}");
            assert!(!shown.chars().any(char::is_control), "{list}: {shown:?}");
        }
    }
}

#[test]
fn the_live_parse_still_accepts_the_same_paths() {
    for list in ["read_write", "read_only"] {
        for path in [
            "/",
            "//",
            "/.",
            "/./",
            "/proc",
            "/proc/self/root",
            "/sys",
            "/usr/..",
            "bin",
        ] {
            PolicyDocument::parse(&path_doc(list, path))
                .unwrap_or_else(|error| panic!("live {list} {path}: {error}"));
        }
    }
}

#[test]
fn every_read_write_entry_is_checked_not_only_the_first() {
    let doc = |paths: &[&str]| {
        let list = paths.iter().fold(String::new(), |mut text, path| {
            text.push_str(&format!("    - '{path}'\n"));
            text
        });
        format!("version: 1\nfilesystem_policy:\n  read_write:\n{list}{HARD}")
    };
    PolicyDocument::parse_authored(&doc(&["/tmp", "/sandbox"])).unwrap();
    assert!(matches!(
        PolicyDocument::parse_authored(&doc(&["/tmp", "/."])),
        Err(PolicyError::ReadWriteRoot { ref path }) if path == "/."
    ));
    assert!(matches!(
        PolicyDocument::parse_authored(&doc(&["/tmp", "/proc"])),
        Err(PolicyError::ReadWriteAlias { ref path }) if path == "/proc"
    ));
    assert!(matches!(
        PolicyDocument::parse_authored(&doc(&["/tmp", "/sandbox", "/./sys/kernel"])),
        Err(PolicyError::ReadWriteAlias { .. })
    ));
}

fn gateway_create_violations(text: &str) -> Vec<PolicyViolation> {
    let proto = parse_sandbox_policy(text).unwrap();
    match validate_and_canonicalize_sandbox_policy(proto) {
        Err(error) => error.violations().to_vec(),
        Ok(canonical) => validate_sandbox_policy(&canonical)
            .err()
            .unwrap_or_default(),
    }
}

fn endpoint_doc(host: &str, extra: &str) -> String {
    format!(
        "version: 1\nfilesystem_policy:\n  read_only:\n    - /bin\n{HARD}network_policies:\n  peek:\n    name: peek\n    endpoints:\n      - host: '{host}'\n        port: 2376\n{extra}    binaries:\n      - path: /usr/bin/curl\n"
    )
}

#[test]
fn role_check_gives_the_verdict_of_the_gateway_create_check_for_every_tcp_endpoint_host() {
    let tcp = "        protocol: tcp\n";
    let rows: Vec<(&str, String, bool)> = vec![
        ("198.18.0.2", endpoint_doc("198.18.0.2", tcp), true),
        (
            "198.18.0.2 tls skip",
            endpoint_doc("198.18.0.2", &format!("{tcp}        tls: skip\n")),
            true,
        ),
        (
            "198.18.0.2 upper case protocol",
            endpoint_doc("198.18.0.2", "        protocol: TCP\n"),
            true,
        ),
        ("127.0.0.1", endpoint_doc("127.0.0.1", tcp), true),
        ("10.0.0.1", endpoint_doc("10.0.0.1", tcp), true),
        ("::1", endpoint_doc("::1", tcp), true),
        (
            "::ffff:198.18.0.2",
            endpoint_doc("::ffff:198.18.0.2", tcp),
            true,
        ),
        ("[::1]", endpoint_doc("[::1]", tcp), true),
        ("198.18.0.2 dot", endpoint_doc("198.18.0.2.", tcp), true),
        ("3323068418", endpoint_doc("3323068418", tcp), false),
        ("0xc6120002", endpoint_doc("0xc6120002", tcp), false),
        ("198.18.2", endpoint_doc("198.18.2", tcp), false),
        ("example.com", endpoint_doc("example.com", tcp), false),
        (
            "198.18.0.2.nip.io",
            endpoint_doc("198.18.0.2.nip.io", tcp),
            false,
        ),
        ("wildcard", endpoint_doc("*.example.com", tcp), false),
        (
            "198.18.0.2 without a protocol",
            endpoint_doc("198.18.0.2", ""),
            false,
        ),
        (
            "198.18.0.2 on a rest endpoint",
            endpoint_doc(
                "198.18.0.2",
                "        protocol: rest\n        access: read-only\n",
            ),
            false,
        ),
    ];
    for (label, text, refused) in &rows {
        let violations = gateway_create_violations(text);
        assert_eq!(!violations.is_empty(), *refused, "{label}: the gateway");
        let jackin = PolicyDocument::parse_authored(text);
        assert_eq!(jackin.is_err(), *refused, "{label}: role check");
    }
    let literal = PolicyDocument::parse_authored(&endpoint_doc("198.18.0.2", tcp));
    let Err(PolicyError::Gateway { violations }) = literal else {
        panic!("{literal:?}");
    };
    assert!(
        violations.iter().any(|found| matches!(
            found,
            PolicyViolation::TcpEndpointIpLiteral { host, .. } if host == "198.18.0.2"
        )),
        "{violations:?}"
    );
    assert!(
        PolicyDocument::parse(&endpoint_doc("198.18.0.2", tcp)).is_ok(),
        "the live parse of a stored policy stays lenient"
    );
}
