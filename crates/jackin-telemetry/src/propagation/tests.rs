use super::*;

#[derive(Default)]
struct TestCarrier {
    v: u16,
    parent: Option<String>,
    state: Option<String>,
    invocation: Option<String>,
    session: Option<String>,
    job: Option<String>,
}
impl Carrier for TestCarrier {
    fn version(&self) -> u16 {
        self.v
    }
    fn traceparent(&self) -> Option<&str> {
        self.parent.as_deref()
    }
    fn tracestate(&self) -> Option<&str> {
        self.state.as_deref()
    }
    fn invocation_id(&self) -> Option<&str> {
        self.invocation.as_deref()
    }
    fn session_id(&self) -> Option<&str> {
        self.session.as_deref()
    }
    fn job_id(&self) -> Option<&str> {
        self.job.as_deref()
    }
    fn set_trace(&mut self, parent: String, state: Option<String>) {
        self.parent = Some(parent);
        self.state = state;
    }
    fn set_product_ids(
        &mut self,
        invocation: Option<String>,
        session: Option<String>,
        job: Option<String>,
    ) {
        self.invocation = invocation;
        self.session = session;
        self.job = job;
    }
}

#[test]
fn extraction_matrix_honors_unsampled_and_rejects_product_ids() {
    let valid = TestCarrier {
        v: 1,
        parent: Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00".into()),
        ..Default::default()
    };
    let ExtractOutcome::Parent(parent) = extract(&valid) else {
        panic!("valid parent")
    };
    assert!(!parent.is_sampled());
    let malformed = TestCarrier {
        v: 1,
        parent: Some("bad".into()),
        ..Default::default()
    };
    assert_eq!(extract(&malformed), ExtractOutcome::LocalRoot);
    let invalid_id = TestCarrier {
        v: 1,
        invocation: Some("not-a-uuid".into()),
        ..Default::default()
    };
    assert_eq!(extract(&invalid_id), ExtractOutcome::RejectRequest);
}

#[test]
fn session_id_is_an_opaque_bounded_value() {
    for session in [
        "opaque session/with symbols!?".to_owned(),
        "a".repeat(64),
        "é".repeat(32),
    ] {
        let carrier = TestCarrier {
            v: VERSION,
            session: Some(session),
            ..Default::default()
        };
        assert_eq!(extract(&carrier), ExtractOutcome::LocalRoot);
    }

    for session in [String::new(), "a".repeat(65), "é".repeat(33)] {
        let carrier = TestCarrier {
            v: VERSION,
            session: Some(session),
            ..Default::default()
        };
        assert_eq!(extract(&carrier), ExtractOutcome::RejectRequest);
    }
}
