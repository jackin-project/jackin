use super::*;

#[test]
fn artifact_must_belong_to_the_current_repository() {
    let artifact = Artifact {
        id: 1,
        expired: false,
        created_at: String::new(),
        workflow_run: Some(WorkflowRun {
            head_repository_id: 42,
        }),
    };
    assert!(artifact.reusable(42));
    assert!(!artifact.reusable(7));
}

#[test]
fn expired_artifact_is_not_reusable() {
    let artifact = Artifact {
        id: 1,
        expired: true,
        created_at: String::new(),
        workflow_run: Some(WorkflowRun {
            head_repository_id: 42,
        }),
    };
    assert!(!artifact.reusable(42));
}
