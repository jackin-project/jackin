use super::*;

#[test]
fn identity_values_are_uuid_unique_and_parseable() {
    let first = InvocationId::mint();
    let second = InvocationId::mint();
    assert_ne!(first, second);
    assert_eq!(InvocationId::parse(&first.to_string()).unwrap(), first);
}

#[test]
fn session_tracks_previous_only_when_known() {
    let first = begin_session();
    assert_eq!(first.previous, None);
    let second = begin_session();
    assert_eq!(second.previous, Some(first.current));
    end_session(second.current);
    assert_eq!(current_session(), None);
}
