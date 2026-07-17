use super::enforce;

#[test]
fn incomplete_status_does_not_require_wiring() {
    enforce("**Status**: Design complete", "", "agent_status.rs").unwrap();
}

#[test]
fn implemented_status_requires_module_and_policy() {
    assert!(enforce("**Status**: Implemented", "", "").is_err());
    assert!(
        enforce(
            "**Status**: Shipped",
            "pub mod agent_status;",
            "agent_status.rs"
        )
        .is_err()
    );
    enforce("**Status**: Landed", "pub mod agent_status;", "").unwrap();
}
