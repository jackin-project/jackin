// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{enumerate_hermes_store, parse_simple_yaml, validate_single_profile_store};
use crate::accounts::stores::{CredentialKind, StoreCandidate, StoreError, StoreKind};

#[test]
fn merges_inline_and_file_profiles_with_auth() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.yaml"),
        "# leading comment\n---\nprofiles:\n  work:\n    provider: anthropic  # trailing\n  stale:\n    provider: openai\n",
    )
    .unwrap();
    let profiles = dir.path().join("profiles");
    std::fs::create_dir(&profiles).unwrap();
    std::fs::write(profiles.join("stale.yaml"), "provider: xai\n").unwrap();
    std::fs::write(profiles.join("extra.yml"), "provider: 'zai'\n").unwrap();
    std::fs::write(profiles.join("notes.txt"), "provider: ignored\n").unwrap();
    std::fs::write(
        dir.path().join("auth.json"),
        r#"{"anthropic": {"type": "api", "key": "fixture-001"}, "xai": {"type": "oauth", "access": "fixture-002"}, "zai": {"type": "oauth", "refresh": "fixture-003"}}"#,
    )
    .unwrap();
    let candidates = enumerate_hermes_store(dir.path()).unwrap();
    let auth = dir.path().join("auth.json");
    let expected = vec![
        StoreCandidate::new(
            StoreKind::Hermes,
            "zai".to_owned(),
            Some("extra".to_owned()),
            auth.clone(),
            CredentialKind::OAuth,
            "refresh".to_owned(),
            "fixture-003".to_owned(),
        ),
        StoreCandidate::new(
            StoreKind::Hermes,
            "xai".to_owned(),
            Some("stale".to_owned()),
            auth.clone(),
            CredentialKind::OAuth,
            "access".to_owned(),
            "fixture-002".to_owned(),
        ),
        StoreCandidate::new(
            StoreKind::Hermes,
            "anthropic".to_owned(),
            Some("work".to_owned()),
            auth.clone(),
            CredentialKind::ApiKey,
            "key".to_owned(),
            "fixture-001".to_owned(),
        ),
    ];
    assert_eq!(candidates, expected);
}

#[test]
fn missing_inputs_yield_no_candidates() {
    let dir = tempfile::tempdir().unwrap();
    assert!(enumerate_hermes_store(dir.path()).unwrap().is_empty());
    std::fs::write(
        dir.path().join("config.yaml"),
        "profiles:\n  a:\n    provider: x\n",
    )
    .unwrap();
    assert!(enumerate_hermes_store(dir.path()).unwrap().is_empty());
}

#[test]
fn skips_profiles_without_provider_or_secret() {
    let dir = tempfile::tempdir().unwrap();
    let profiles = dir.path().join("profiles");
    std::fs::create_dir(&profiles).unwrap();
    std::fs::write(profiles.join("empty.yaml"), "model: big\n").unwrap();
    std::fs::write(profiles.join("blank.yaml"), "provider: '  '\n").unwrap();
    std::fs::write(profiles.join("ok.yaml"), "provider: moon\n").unwrap();
    std::fs::write(
        dir.path().join("auth.json"),
        r#"{"moon": {"type": "api", "key": "   "}, "other": {"type": "api", "key": "fixture-004"}}"#,
    )
    .unwrap();
    assert!(enumerate_hermes_store(dir.path()).unwrap().is_empty());
}

#[test]
fn whole_store_validator_rejects_multiple_profiles() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.yaml"),
        "profiles:\n  personal:\n    provider: anthropic\n  work:\n    provider: openai\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("auth.json"),
        r#"{"anthropic":{"type":"api","key":"personal-sentinel"},"openai":{"type":"api","key":"work-sentinel"}}"#,
    )
    .unwrap();
    assert_eq!(
        validate_single_profile_store(dir.path()).unwrap_err(),
        StoreError::Unsupported("Hermes credential store contains multiple profiles")
    );
}

#[test]
fn rejects_malformed_sources() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("auth.json"), "{}").unwrap();
    for (name, raw) in [
        ("tab.yaml", "profiles:\n\tbad:\n"),
        ("sequence.yaml", "profiles:\n  - item\n"),
        ("flow.yaml", "profiles: {a: b}\n"),
        ("dup.yaml", "a: 1\na: 2\n"),
        ("indent.yaml", "profiles:\n      deep: x\n  flat: y\n"),
        ("unbalanced.yaml", "a: \"oops\n"),
        ("profiles", "profiles: [x]\n"),
    ] {
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, raw).unwrap();
        assert_eq!(
            enumerate_hermes_store(dir.path()).unwrap_err(),
            StoreError::Malformed,
            "file: {name}"
        );
    }
    std::fs::remove_file(dir.path().join("config.yaml")).unwrap();
    std::fs::write(dir.path().join("auth.json"), "{oops").unwrap();
    assert_eq!(
        enumerate_hermes_store(dir.path()).unwrap_err(),
        StoreError::Malformed
    );
}

#[test]
fn yaml_subset_handles_quotes_escapes_and_keys() {
    let doc = parse_simple_yaml(
        "plain: http://host:8080/a # cut\nsingle: 'it''s'\ndouble: \"a\\tb\"\n\"quoted key\": v\nnested:\n  child: 1\nempty:\n",
    )
    .unwrap();
    assert_eq!(doc.len(), 6);
    let text = |key: &str| match doc.get(key).unwrap() {
        super::YamlNode::Scalar(value) => value.clone(),
        super::YamlNode::Map(_) => String::new(),
    };
    assert_eq!(text("plain"), "http://host:8080/a");
    assert_eq!(text("single"), "it's");
    assert_eq!(text("double"), "a\tb");
    assert_eq!(text("quoted key"), "v");
    assert!(matches!(
        doc.get("nested").unwrap(),
        super::YamlNode::Map(children) if children.len() == 1
    ));
    assert!(matches!(
        doc.get("empty").unwrap(),
        super::YamlNode::Map(children) if children.is_empty()
    ));
}
