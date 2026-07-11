//! Tests for `OpProbeError` display + equality.

use super::OpProbeError;

#[test]
fn display_texts_include_details() {
    let not_installed = OpProbeError::NotInstalled {
        detail: "No such file".into(),
    };
    assert_eq!(
        not_installed.to_string(),
        "failed to spawn op: No such file"
    );

    let not_signed = OpProbeError::NotSignedIn {
        detail: "no accounts".into(),
    };
    assert_eq!(
        not_signed.to_string(),
        "1Password CLI is not signed in: no accounts"
    );

    let timeout = OpProbeError::Timeout { seconds: 12 };
    assert_eq!(timeout.to_string(), "op timed out after 12s");

    let other = OpProbeError::Other {
        message: "exit 1: boom".into(),
    };
    assert_eq!(other.to_string(), "exit 1: boom");
}

#[test]
fn variants_are_eq_comparable() {
    assert_eq!(
        OpProbeError::Timeout { seconds: 5 },
        OpProbeError::Timeout { seconds: 5 }
    );
    assert_ne!(
        OpProbeError::Timeout { seconds: 5 },
        OpProbeError::Timeout { seconds: 6 }
    );
    assert_eq!(
        OpProbeError::NotInstalled {
            detail: "x".into()
        },
        OpProbeError::NotInstalled {
            detail: "x".into()
        }
    );
}
