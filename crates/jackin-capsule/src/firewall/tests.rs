// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn ensure_tool_errors_actionably_for_missing_binary() {
    let err = ensure_tool("jackin-no-such-firewall-binary-xyz").unwrap_err();
    assert!(
        err.to_string().contains("allowlist"),
        "missing-tool error must name the allowlist requirement: {err}"
    );
}

#[test]
fn classifies_ipv4_and_cidr_as_net() {
    assert_eq!(classify_entry("1.2.3.4"), Entry::Net("1.2.3.4".into()));
    assert_eq!(
        classify_entry("1.2.3.0/24"),
        Entry::Net("1.2.3.0/24".into())
    );
}

#[test]
fn classifies_ipv6_literal_without_port_mangling() {
    // The old shell script's `${entry%%:*}` truncated this to `2001`.
    assert_eq!(classify_entry("::1"), Entry::Net("::1".into()));
    assert_eq!(
        classify_entry("2001:db8::1"),
        Entry::Net("2001:db8::1".into())
    );
    assert_eq!(
        classify_entry("2001:db8::/32"),
        Entry::Net("2001:db8::/32".into())
    );
}

#[test]
fn strips_wildcard_to_apex() {
    assert_eq!(
        classify_entry("*.cdn.example.com"),
        Entry::Domain("cdn.example.com".into())
    );
}

#[test]
fn strips_port_from_domain() {
    assert_eq!(
        classify_entry("registry.npmjs.org:443"),
        Entry::Domain("registry.npmjs.org".into())
    );
}

#[test]
fn parses_list_with_whitespace_and_blanks() {
    let entries = parse_allowed_hosts(" api.anthropic.com , 1.2.3.0/24 ,, *.cdn.x.com ");
    assert_eq!(
        entries,
        vec![
            Entry::Domain("api.anthropic.com".into()),
            Entry::Net("1.2.3.0/24".into()),
            Entry::Domain("cdn.x.com".into()),
        ]
    );
}

#[test]
fn empty_input_yields_no_entries() {
    assert!(parse_allowed_hosts("").is_empty());
    assert!(parse_allowed_hosts("  ,  , ").is_empty());
}

#[test]
fn enforceable_ipv4_accepts_addr_and_valid_cidr() {
    assert!(enforceable_ipv4("1.2.3.4"));
    assert!(enforceable_ipv4("10.0.0.0/8"));
    assert!(enforceable_ipv4("1.2.3.0/32"));
}

#[test]
fn enforceable_ipv4_rejects_ipv6_and_malformed() {
    // IPv6 is classified as Net but cannot be enforced (inet ipset / IPv4
    // iptables) — must be rejected so it never enters the restore batch.
    assert!(!enforceable_ipv4("::1"));
    assert!(!enforceable_ipv4("2001:db8::/32"));
    // Malformed prefixes must not reach `ipset restore` and abort the batch.
    assert!(!enforceable_ipv4("1.2.3.0/99"));
    assert!(!enforceable_ipv4("1.2.3.0/abc"));
    assert!(!enforceable_ipv4("1.2.3.0/"));
    assert!(!enforceable_ipv4("not-an-ip"));
}

#[test]
fn ipset_restore_stream_has_create_flush_then_sorted_adds() {
    let members: BTreeSet<String> = ["10.0.0.2", "10.0.0.1"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    assert_eq!(
        ipset_restore_stream(&members),
        "create jackin-allowed hash:net maxelem 65536\n\
             flush jackin-allowed\n\
             add jackin-allowed 10.0.0.1\n\
             add jackin-allowed 10.0.0.2\n"
    );
}

#[test]
fn ipset_restore_stream_empty_is_create_flush_only() {
    let members = BTreeSet::new();
    assert_eq!(
        ipset_restore_stream(&members),
        "create jackin-allowed hash:net maxelem 65536\nflush jackin-allowed\n"
    );
}

#[test]
fn ipv6_deny_rules_drop_first_loopback_and_established_only() {
    // Fail-closed ordering: the default-DROP policy must be the first rule, so a
    // mid-apply error can never leave IPv6 OUTPUT at ACCEPT.
    assert_eq!(BASE_DENY_RULES[0], ["-P", "OUTPUT", "DROP"]);
    // Loopback + established return traffic permitted.
    assert!(BASE_DENY_RULES.iter().any(|r| r.contains(&"lo")));
    assert!(
        BASE_DENY_RULES
            .iter()
            .any(|r| r.contains(&"ESTABLISHED,RELATED"))
    );
    // Deny-all: no destination/allowlist accept (the ipset is IPv4-only).
    assert!(
        !BASE_DENY_RULES
            .iter()
            .any(|r| r.contains(&"-d") || r.contains(&"--match-set")),
        "IPv6 must have no allowlist accept — it is deny-all"
    );
}
