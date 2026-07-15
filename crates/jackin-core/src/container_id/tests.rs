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
