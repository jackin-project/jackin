// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::{BuilderName, ImageTag, ProcessStatus, RoleName, Sha256, TerminalText};
use jackin_image::{BuildStage, EngineError};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn engine_failure_messages_show_names_and_build_output_as_plain_text() -> TestResult {
    let builder = BuilderName::parse("orbstack")?;
    let tag = ImageTag::for_role(&RoleName::parse("sentinel")?, &Sha256::of(b"key"));
    let missing = EngineError::BuilderMissing {
        builder: builder.clone(),
    }
    .to_string();
    let driver = EngineError::BuilderDriver {
        builder,
        driver: "docker-container".to_owned(),
    }
    .to_string();
    let failed = EngineError::BuildFailed {
        stage: BuildStage::Role,
        status: ProcessStatus::Exited(1),
        tail: TerminalText::sanitize(b"step 4/9 failed\nerror: boom"),
    }
    .to_string();
    let unreadable = EngineError::DigestUnreadable {
        tag: tag.clone(),
        output: TerminalText::sanitize(b"line a\nline b"),
    }
    .to_string();

    for (label, message) in [
        ("missing", &missing),
        ("driver", &driver),
        ("failed", &failed),
        ("unreadable", &unreadable),
    ] {
        assert!(!message.contains("BuilderName"), "{label}: {message:?}");
        assert!(!message.contains("ImageTag("), "{label}: {message:?}");
        assert!(!message.contains("TerminalText("), "{label}: {message:?}");
        assert!(!message.contains("\\n"), "{label}: {message:?}");
        assert!(!message.contains('"'), "{label}: {message:?}");
    }
    assert!(missing.contains("orbstack"), "{missing:?}");
    assert!(driver.contains("orbstack"), "{driver:?}");
    assert!(driver.contains("docker-container"), "{driver:?}");
    assert!(failed.contains("role"), "{failed:?}");
    let failed_lines: Vec<&str> = failed.lines().collect();
    assert!(
        failed_lines.contains(&"step 4/9 failed"),
        "{failed_lines:?}"
    );
    assert!(failed_lines.contains(&"error: boom"), "{failed_lines:?}");
    assert!(unreadable.contains(&tag.to_string()), "{unreadable:?}");
    let unreadable_lines: Vec<&str> = unreadable.lines().collect();
    assert!(unreadable_lines.contains(&"line a"), "{unreadable_lines:?}");
    assert!(unreadable_lines.contains(&"line b"), "{unreadable_lines:?}");
    Ok(())
}
