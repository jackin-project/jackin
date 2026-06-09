//! Tests for `subscriptions`.
use super::*;

#[test]
fn worker_disconnect_messages_are_subscription_owned() {
    assert_eq!(
        drift_check_worker_disconnected_message(),
        "drift check worker disconnected"
    );
    assert_eq!(
        isolation_cleanup_worker_disconnected_message(),
        "isolation cleanup worker disconnected"
    );
    assert_eq!(
        role_loader_worker_disconnected_message(),
        "role loader worker disconnected"
    );
    assert_eq!(
        op_read_worker_disconnected_message(),
        "op read worker disconnected"
    );
    assert_eq!(
        instance_refresh_worker_disconnected_message(),
        "instance refresh worker disconnected"
    );
}
