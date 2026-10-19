// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::fs;

#[test]
fn invalid_adoption_pin_is_rejected_before_generation_or_writes() {
    let source = repo_root().unwrap();
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    let relative = "crates/services/jackin-telemetry";
    for name in [
        "registry/manifest.yaml",
        "registry/rust.toml",
        "vendor/semconv-1.44.0/manifest.yaml",
        "vendor/semconv-1.44.0/events.yaml",
        "vendor/semconv-1.44.0/registry.yaml",
    ] {
        let destination = root.join(relative).join(name);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(source.join(relative).join(name), destination).unwrap();
    }
    let manifest = fs::read_to_string(source.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("version = \"=0.33.1\""));
    fs::write(
        root.join("Cargo.toml"),
        manifest.replace("version = \"=0.33.1\"", "version = \"=0.33.0\""),
    )
    .unwrap();
    let artifact = root.join(relative).join("src/schema/attrs.rs");
    fs::create_dir_all(artifact.parent().unwrap()).unwrap();
    fs::write(&artifact, b"preserved generated sentinel").unwrap();
    let failure = generate_rust_sources(root).unwrap_err();
    assert!(
        failure
            .to_string()
            .contains("workspace semantic-conventions dependency does not match"),
        "{failure:#}"
    );
    assert_eq!(fs::read(artifact).unwrap(), b"preserved generated sentinel");
    assert!(!root.join(relative).join("src/schema/events.rs").exists());
}
