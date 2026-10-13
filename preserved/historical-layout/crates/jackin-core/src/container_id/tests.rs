use super::*;

#[test]
fn rejects_empty_and_whitespace() {
    assert!(matches!(
        ContainerId::parse(""),
        Err(ContainerIdError::Empty)
    ));
    assert!(matches!(
        ContainerId::parse("jk role"),
        Err(ContainerIdError::ForbiddenChars(_))
    ));
    assert!(matches!(
        ContainerId::parse("a/b"),
        Err(ContainerIdError::ForbiddenChars(_))
    ));
}

#[test]
fn accepts_docker_style_name() {
    let id = ContainerId::parse("jk-ab12cd34-myws-myrole").unwrap();
    assert_eq!(id.as_str(), "jk-ab12cd34-myws-myrole");
}

#[test]
fn serde_transparent_round_trip() {
    let id = ContainerId::parse("jk-x").unwrap();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"jk-x\"");
    let back: ContainerId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}

#[test]
fn serde_rejects_every_forbidden_name_class() {
    for raw in [
        "",
        "jk role",
        "jk\trole",
        "jk\nrole",
        "jk\rrole",
        "jk\u{00a0}role",
        "jk\u{2003}role",
        "a/b",
        "a\\b",
    ] {
        let json = serde_json::to_string(raw).unwrap();
        let error = serde_json::from_str::<ContainerId>(&json).unwrap_err();
        let expected = ContainerId::parse(raw).unwrap_err().to_string();
        assert!(error.to_string().contains(&expected), "{raw:?}: {error}");
    }
}

#[test]
fn serde_preserves_valid_string_wire() {
    for raw in [
        "jk-x",
        "jk-ab12cd34-myws-myrole",
        "role_1",
        "role.name",
        "役割",
        "a:b",
    ] {
        let id = ContainerId::parse(raw).unwrap();
        let json = serde_json::to_string(raw).unwrap();
        assert_eq!(serde_json::to_string(&id).unwrap(), json);
        let back: ContainerId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }
}

#[test]
fn serde_requires_string_wire() {
    for json in ["null", "0", "true", "[]", "{}"] {
        assert!(serde_json::from_str::<ContainerId>(json).is_err(), "{json}");
    }
}
