//! Tests for OSC passthrough policy.

use super::*;

fn policy_with(pairs: &[(&str, &str)]) -> OscPolicy {
    OscPolicy::from_lookup(|name| {
        pairs
            .iter()
            .find_map(|(key, value)| (*key == name).then(|| (*value).to_owned()))
    })
}

#[test]
fn default_policy_denies_osc52_but_keeps_lower_risk_defaults() {
    let policy = OscPolicy::default();

    assert!(!policy.allow_osc52());
    assert!(policy.allow_title());
    assert!(policy.allow_notify());
    assert!(policy.allow_hyperlink());
}

#[test]
fn env_allow_enables_osc52() {
    let policy = policy_with(&[(ENV_OSC52, "allow")]);

    assert!(policy.allow_osc52());
}

#[test]
fn env_deny_keeps_osc52_denied() {
    for value in ["deny", "off", "no"] {
        let policy = policy_with(&[(ENV_OSC52, value)]);

        assert!(!policy.allow_osc52());
    }
}

#[test]
fn deny_values_still_disable_other_osc_surfaces() {
    let policy = policy_with(&[
        (ENV_OSC_TITLE, "deny"),
        (ENV_OSC_NOTIFY, "off"),
        (ENV_OSC_HYPERLINK, "no"),
    ]);

    assert!(!policy.allow_title());
    assert!(!policy.allow_notify());
    assert!(!policy.allow_hyperlink());
}
