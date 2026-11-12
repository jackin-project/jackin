// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::{BuilderName, ImageId, ImageTag, ProcessStatus, RoleName, Sha256, TerminalText};
use jackin_image::{BuildStage, ContextProblem, EngineError, ImageError};

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

    let entry = EngineError::ContextEntry {
        path: "src/link".to_owned(),
        problem: ContextProblem::Symlink,
    }
    .to_string();
    let builder_unreadable = EngineError::BuilderUnreadable {
        builder: BuilderName::parse("orbstack")?,
        status: ProcessStatus::Exited(1),
        output: TerminalText::sanitize(b"Cannot connect\nto the daemon"),
    }
    .to_string();
    let built = ImageId::parse(&format!("sha256:{}", "a".repeat(64)))?;
    let found = ImageId::parse(&format!("sha256:{}", "b".repeat(64)))?;
    let absent = ImageError::NotDelivered {
        tag: tag.clone(),
        built: built.clone(),
        found: None,
    }
    .to_string();
    let moved = ImageError::NotDelivered {
        tag: tag.clone(),
        built: built.clone(),
        found: Some(found.clone()),
    }
    .to_string();
    let source_gone = ImageError::SourceMoved {
        tag: tag.clone(),
        expected: built.clone(),
        found: None,
    }
    .to_string();
    let source_moved = ImageError::SourceMoved {
        tag: tag.clone(),
        expected: built.clone(),
        found: Some(found.clone()),
    }
    .to_string();

    for (label, message) in [
        ("missing", &missing),
        ("driver", &driver),
        ("failed", &failed),
        ("unreadable", &unreadable),
        ("entry", &entry),
        ("builder unreadable", &builder_unreadable),
        ("not delivered, absent", &absent),
        ("not delivered, moved", &moved),
        ("source moved, absent", &source_gone),
        ("source moved, retagged", &source_moved),
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
    assert!(entry.contains("src/link"), "{entry:?}");
    assert!(entry.contains("symbolic link"), "{entry:?}");
    assert!(
        builder_unreadable.contains("orbstack"),
        "{builder_unreadable:?}"
    );
    let unreadable_lines: Vec<&str> = builder_unreadable.lines().collect();
    assert!(
        unreadable_lines.contains(&"to the daemon"),
        "{unreadable_lines:?}"
    );
    for message in [&absent, &moved] {
        assert!(message.contains(&tag.to_string()), "{message:?}");
        assert!(message.contains(&built.to_string()), "{message:?}");
    }
    assert!(moved.contains(&found.to_string()), "{moved:?}");
    assert!(!absent.contains(&found.to_string()), "{absent:?}");
    for message in [&source_gone, &source_moved] {
        assert!(message.contains(&tag.to_string()), "{message:?}");
        assert!(message.contains(&built.to_string()), "{message:?}");
    }
    assert!(
        source_moved.contains(&found.to_string()),
        "{source_moved:?}"
    );
    assert!(!source_gone.contains(&found.to_string()), "{source_gone:?}");
    Ok(())
}

#[test]
fn engine_text_fields_are_shown_without_control_characters() -> TestResult {
    let driver = EngineError::BuilderDriver {
        builder: BuilderName::parse("jackin-builder")?,
        driver: "docker\u{1b}[2J\ncontainer".to_owned(),
    };
    assert_eq!(
        driver.to_string(),
        "builder jackin-builder uses the docker\\u{1b}[2J\\ncontainer driver; the docker driver is required"
    );
    let entry = EngineError::ContextEntry {
        path: "bin/a\u{1b}b\nc".to_owned(),
        problem: ContextProblem::Symlink,
    };
    assert_eq!(
        entry.to_string(),
        "the build context cannot hold bin/a\\u{1b}b\\nc: it is a symbolic link"
    );
    Ok(())
}
