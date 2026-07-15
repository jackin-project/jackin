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
