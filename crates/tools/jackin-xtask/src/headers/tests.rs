use super::*;
use std::collections::BTreeMap;

fn tiers() -> BTreeMap<&'static str, u8> {
    BTreeMap::from([("jackin-core", 0), ("jackin-config", 1)])
}

#[test]
fn fully_conformant() {
    let h = vec![
        "jackin-core: owns stuff.".into(),
        "**Architecture Invariant:** T0.".into(),
        "Entry point: [`Agent`] — primary noun.".into(),
    ];
    assert!(check_header("jackin-core", &h, &tiers()).is_empty());
}

#[test]
fn missing_owns_line() {
    let h = vec![
        "something else".into(),
        "**Architecture Invariant:** T0.".into(),
        "Entry point: [`Agent`] — x.".into(),
    ];
    let p = check_header("jackin-core", &h, &tiers());
    assert!(p.iter().any(|x| x.contains("first doc line")));
}

#[test]
fn wrong_crate_name() {
    let h = vec![
        "other: owns".into(),
        "**Architecture Invariant:** T0.".into(),
        "Entry point: [`X`] — y.".into(),
    ];
    let p = check_header("jackin-core", &h, &tiers());
    assert!(p.iter().any(|x| x.contains("first doc line")));
}

#[test]
fn missing_tier() {
    let h = vec![
        "jackin-core: owns".into(),
        "Entry point: [`Agent`] — x.".into(),
    ];
    let p = check_header("jackin-core", &h, &tiers());
    assert!(p.iter().any(|x| x.contains("Architecture Invariant")));
}

#[test]
fn tier_mismatch() {
    let h = vec![
        "jackin-core: owns".into(),
        "**Architecture Invariant:** T3.".into(),
        "Entry point: [`Agent`] — x.".into(),
    ];
    let p = check_header("jackin-core", &h, &tiers());
    assert!(p.iter().any(|x| x.contains("does not match")));
}

#[test]
fn missing_entry() {
    let h = vec![
        "jackin-core: owns".into(),
        "**Architecture Invariant:** T0.".into(),
    ];
    let p = check_header("jackin-core", &h, &tiers());
    assert!(p.iter().any(|x| x.contains("Entry point")));
}
