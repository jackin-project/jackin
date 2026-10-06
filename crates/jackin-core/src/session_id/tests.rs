use super::*;

#[test]
fn rejects_zero() {
    assert!(matches!(SessionId::new(0), Err(SessionIdError::Zero)));
}

#[test]
fn accepts_nonzero() {
    let id = SessionId::new(42).unwrap();
    assert_eq!(id.get(), 42);
    assert_eq!(u64::from(id), 42);
}

#[test]
fn serde_transparent_round_trip() {
    let id = SessionId::new(7).unwrap();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "7");
    let back: SessionId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}

#[test]
fn serde_rejects_zero() {
    let error = serde_json::from_str::<SessionId>("0").unwrap_err();
    assert!(error.to_string().contains("session id cannot be zero"));
}

#[test]
fn serde_preserves_valid_numeric_handles() {
    for raw in [1, 7, 42, u64::MAX] {
        let json = raw.to_string();
        let id: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, SessionId::new(raw).unwrap());
        assert_eq!(serde_json::to_string(&id).unwrap(), json);
    }
}

#[test]
fn serde_rejects_non_u64_wire_values() {
    for json in [
        "-1",
        "1.5",
        "18446744073709551616",
        "\"7\"",
        "null",
        "true",
        "{}",
        "[]",
    ] {
        assert!(serde_json::from_str::<SessionId>(json).is_err(), "{json}");
    }
}
