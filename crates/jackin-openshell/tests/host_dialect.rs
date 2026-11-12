// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::unwrap_used,
    reason = "test helpers fail the test by panicking"
)]

use std::collections::BTreeSet;

use globset::{GlobBuilder, GlobMatcher};
use jackin_core::ProviderName;
use jackin_openshell::{AuthoredPolicy, ClosureViolation, HostPattern, RuleKey};
use openshell_core::host_pattern::host_patterns_overlap;

mod common;

use common::{profile, rule_doc};

enum Sandbox {
    Equal(String),
    Glob(GlobMatcher),
    Undefined,
}

impl Sandbox {
    fn new(pattern: &str) -> Sandbox {
        let pattern = pattern.to_lowercase();
        if !pattern.contains('*') {
            return Sandbox::Equal(pattern);
        }
        match GlobBuilder::new(&Self::rewrite(&pattern))
            .literal_separator(true)
            .build()
        {
            Ok(glob) => Sandbox::Glob(glob.compile_matcher()),
            Err(_) => Sandbox::Undefined,
        }
    }

    fn rewrite(text: &str) -> String {
        text.replace('/', "\0").replace('.', "/./")
    }

    fn matches(&self, host: &str) -> bool {
        let host = host.to_lowercase();
        match self {
            Sandbox::Equal(pattern) => *pattern == host,
            Sandbox::Glob(glob) => glob.is_match(Self::rewrite(&host)),
            Sandbox::Undefined => false,
        }
    }
}

fn sandbox_matches(pattern: &str, host: &str) -> bool {
    Sandbox::new(pattern).matches(host)
}

const REVIEW_PAIRS: [(&str, &str); 6] = [
    ("[^x]*hatgpt.com", "chatgpt.com"),
    ("[^x]*.chatgpt.com", "ab.chatgpt.com"),
    ("[^x]*pi.openai.com", "api.openai.com"),
    ("*\\.openai.com", "api.openai.com"),
    ("a\\p*.openai.com", "api.openai.com"),
    ("[^x]*.openai.com", "api.openai.com"),
];

const RECORDED_REGO: [(&str, &str, bool); 15] = [
    ("[^x]*hatgpt.com", "chatgpt.com", true),
    ("[^x]*.chatgpt.com", "ab.chatgpt.com", true),
    ("[^x]*pi.openai.com", "api.openai.com", true),
    ("*\\.openai.com", "api.openai.com", true),
    ("*\\.openai.com", "auth.openai.com", true),
    ("a\\p*.openai.com", "api.openai.com", true),
    ("[^x]*.example.org", "api.openai.com", false),
    ("*.openai.com", "openai.com", false),
    ("**.openai.com", "x.y.openai.com", true),
    ("*.openai.com", "x.y.openai.com", false),
    ("API.OPENAI.COM", "api.openai.com", true),
    ("chatgpt.com", "chatgpt.com.evil.net", false),
    ("*\u{212A}.example.com", "xk.example.com", true),
    ("a?i.openai.com", "api.openai.com", false),
    ("*?i.openai.com", "api.openai.com", true),
];

const PATTERN_LABELS: [&str; 10] = ["a", "b", "ab", "a-b", "_", "*", "**", "a*", "*a*", "a*b"];
const HOST_LABELS: [&str; 5] = ["a", "b", "ab", "aab", "a-b"];
const NEAR_PROVIDER: [&str; 11] = [
    "*.openai.com",
    "**.openai.com",
    "*pi.openai.com",
    "api.*.com",
    "*.chatgpt.com",
    "**.chatgpt.com",
    "chat*.com",
    "*gpt.com",
    "a*i.openai.com",
    "*.OPENAI.COM",
    "API.OPENAI.COM",
];
const EXTRA_HOSTS: [&str; 8] = [
    "api.openai.com",
    "auth.openai.com",
    "chatgpt.com",
    "ab.chatgpt.com",
    "openai.com",
    "x.api.openai.com",
    "chatgpt.com.evil.net",
    "API.openai.com",
];

fn joined(labels: &[&str], longest: u32) -> Vec<String> {
    let mut hosts = Vec::new();
    for count in 1..=longest {
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
    hosts
}

fn hostile_variants(provider_hosts: &[String]) -> Vec<String> {
    let bytes = (0x20_u8..=0x7E).filter(|byte| {
        !(byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'*'))
    });
    let bytes: Vec<char> = bytes.map(char::from).collect();
    let mut variants = Vec::new();
    for host in provider_hosts {
        let chars: Vec<char> = host.chars().collect();
        for index in 0..chars.len() {
            let (head, tail): (String, String) = (
                chars[..index].iter().collect(),
                chars[index + 1..].iter().collect(),
            );
            for byte in &bytes {
                variants.push(format!("{head}{byte}{tail}"));
                variants.push(format!("{head}{byte}{}{tail}", chars[index]));
                variants.push(format!("{head}{byte}*{tail}"));
                variants.push(format!("{head}*{byte}{tail}"));
            }
        }
    }
    variants
}

struct Tally {
    compared: usize,
    sandbox_true: usize,
    bypasses: Vec<String>,
    drift: Vec<String>,
}

fn compare(patterns: &[String], hosts: &[String], tally: &mut Tally) {
    let literals: Vec<HostPattern> = hosts
        .iter()
        .map(|host| HostPattern::parse(host).unwrap())
        .collect();
    for pattern in patterns {
        let Ok(parsed) = HostPattern::parse(pattern) else {
            continue;
        };
        let sandbox = Sandbox::new(pattern);
        for (host, literal) in hosts.iter().zip(&literals) {
            assert!(
                host_patterns_overlap(parsed.as_str(), literal.as_str()).is_ok(),
                "pattern {pattern} host {host}: the matcher errs"
            );
            let (closure, sandbox) = (parsed.overlaps(literal), sandbox.matches(host));
            tally.compared += 1;
            tally.sandbox_true += usize::from(sandbox);
            if sandbox && !closure {
                tally.bypasses.push(format!(
                    "sandbox true, closure false: pattern {pattern} host {host}"
                ));
            } else if closure && !sandbox {
                tally.drift.push(format!(
                    "closure true, sandbox false: pattern {pattern} host {host}"
                ));
            }
        }
    }
}

#[test]
fn the_closure_agrees_with_the_sandbox_matcher_on_every_accepted_pattern() {
    for (pattern, host) in REVIEW_PAIRS {
        assert!(sandbox_matches(pattern, host), "{pattern} {host}");
        assert_eq!(
            host_patterns_overlap(pattern, host),
            Ok(false),
            "{pattern} {host}"
        );
    }
    let provider_hosts: Vec<String> = profile()
        .endpoints
        .iter()
        .map(|endpoint| endpoint.host.as_str().to_owned())
        .collect();
    let mut hosts = joined(&HOST_LABELS, 3);
    hosts.extend(EXTRA_HOSTS.iter().map(|host| (*host).to_owned()));
    let mut patterns = joined(&PATTERN_LABELS, 3);
    patterns.extend(NEAR_PROVIDER.iter().map(|host| (*host).to_owned()));
    patterns.extend(
        REVIEW_PAIRS
            .iter()
            .map(|(pattern, _)| (*pattern).to_owned()),
    );

    let mut tally = Tally {
        compared: 0,
        sandbox_true: 0,
        bypasses: Vec::new(),
        drift: Vec::new(),
    };
    compare(&patterns, &hosts, &mut tally);
    let (broad, broad_true) = (tally.compared, tally.sandbox_true);
    let hostile = hostile_variants(&provider_hosts);
    assert!(hostile.len() > 5000, "{}", hostile.len());
    let narrow: Vec<String> = provider_hosts
        .iter()
        .cloned()
        .chain(["openai.com".to_owned(), "chatgpt.com.evil.net".to_owned()])
        .collect();
    compare(&hostile, &narrow, &mut tally);
    let failures: Vec<&String> = tally.bypasses.iter().chain(&tally.drift).collect();
    assert!(
        failures.is_empty(),
        "{} bypasses, {} drifts, first: {:#?}",
        tally.bypasses.len(),
        tally.drift.len(),
        &failures[..failures.len().min(8)]
    );
    for (pattern, _) in REVIEW_PAIRS {
        assert!(
            HostPattern::parse(pattern).is_err(),
            "{pattern} is accepted"
        );
    }
    assert!(
        broad > MIN_PAIRS && broad_true > MIN_TRUE,
        "{broad} pairs, {broad_true} sandbox-true"
    );
}

const MIN_PAIRS: usize = 69_000;
const MIN_TRUE: usize = 2_200;

#[test]
fn the_sandbox_model_reproduces_the_recorded_rego_answers() {
    for (pattern, host, expected) in RECORDED_REGO {
        assert_eq!(
            sandbox_matches(pattern, host),
            expected,
            "pattern {pattern} host {host}"
        );
    }
}

const BYPASS_FORMS: [&str; 4] = ["*gpt.com", "chat*.com", "*.openai.com", "**.openai.com"];

const OTHER_HOSTS: [&str; 4] = [
    "x*.com",
    "chatgpt.com.evil.net",
    "example.com",
    "*.example.com",
];

#[test]
fn closure_covers_every_glob_form_that_reaches_a_provider_host() {
    let profile = profile();
    let foreign = RuleKey::provider_rule(&ProviderName::parse("jackin-other-codex").unwrap());
    for form in BYPASS_FORMS {
        assert!(
            profile
                .endpoints
                .iter()
                .any(|endpoint| sandbox_matches(form, endpoint.host.as_str())),
            "{form} reaches no profile host, so it is not a bypass"
        );
        let document = AuthoredPolicy::parse(&rule_doc(&[("r1", form, 443)])).unwrap();
        for generated in [&[][..], std::slice::from_ref(&foreign)] {
            let violations = document.document().closure_violations(&profile, generated);
            assert!(
                !violations.is_empty()
                    && violations.iter().all(|found| matches!(
                        found,
                        ClosureViolation::Overlap { rule, host, .. }
                            if rule.as_str() == "r1" && host.as_str() == form
                    )),
                "{form}: {violations:?}"
            );
        }
    }
    let covered: BTreeSet<&str> = BYPASS_FORMS.into_iter().collect();
    assert_eq!(covered.len(), BYPASS_FORMS.len());
    for form in OTHER_HOSTS {
        assert!(
            profile
                .endpoints
                .iter()
                .all(|endpoint| !sandbox_matches(form, endpoint.host.as_str())),
            "{form} reaches a profile host"
        );
        let document = AuthoredPolicy::parse(&rule_doc(&[("r1", form, 443)])).unwrap();
        assert_eq!(
            document.document().closure_violations(&profile, &[]),
            Vec::new(),
            "{form}"
        );
    }
}
