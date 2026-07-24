// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `claude_keychain`.
use super::*;

#[test]
fn claude_keychain_service_names_match_live_scheme() {
    let home = Path::new("/Users/donbeave");
    let cwd = Path::new("/tmp/work");

    let default = claude_keychain_scope(&home.join(".claude"), home, cwd).expect("default scope");
    assert_eq!(default.service, "Claude Code-credentials");
    assert!(default.is_default);

    // Pinned live-observed suffixes (same values as instance provisioning).
    let chainargos =
        claude_keychain_scope(&home.join(".claude-chainargos"), home, cwd).expect("custom scope");
    assert_eq!(chainargos.service, "Claude Code-credentials-93aecf3d");
    assert!(!chainargos.is_default);

    let work = claude_keychain_scope(&home.join(".claude-work"), home, cwd).expect("custom scope");
    assert_eq!(work.service, "Claude Code-credentials-3342f2c7");

    // Relative and lexical-dot forms normalize to the same absolute service.
    let relative = claude_keychain_scope(
        Path::new("../donbeave/./.claude-work"),
        home,
        Path::new("/Users/other"),
    )
    .expect("relative scope");
    assert_eq!(relative.service, "Claude Code-credentials-3342f2c7");
    assert_eq!(
        relative.normalized_config_dir,
        Path::new("/Users/donbeave/.claude-work")
    );
}

#[test]
fn claude_keychain_scope_rejects_non_default_home_dot_paths() {
    let home = Path::new("/Users/donbeave");
    // A lexical `..` chain resolving back to the default dir is still default.
    let scope = claude_keychain_scope(
        Path::new("/Users/donbeave/x/../.claude"),
        home,
        Path::new("/"),
    )
    .expect("scope");
    assert!(scope.is_default);
    assert_eq!(scope.service, "Claude Code-credentials");
}
