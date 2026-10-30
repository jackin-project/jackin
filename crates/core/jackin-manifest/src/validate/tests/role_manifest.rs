// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_core::ClaudeConfig;
use std::collections::BTreeMap;

fn manifest_with(env: BTreeMap<String, EnvVarDecl>) -> RoleManifest {
    RoleManifest {
        version: jackin_core::current_manifest_version(),
        dockerfile: "Dockerfile".to_owned(),
        published_image: None,
        identity: None,
        agents: None,
        claude: Some(ClaudeConfig::default()),
        codex: None,
        amp: None,
        kimi: None,
        opencode: None,
        grok: None,
        hooks: None,
        env,
        docker: None,
    }
}

fn simple_decl() -> EnvVarDecl {
    EnvVarDecl {
        default_value: Some("value".to_owned()),
        interactive: false,
        skippable: false,
        prompt: None,
        options: Vec::new(),
        depends_on: Vec::new(),
    }
}

#[test]
fn rejects_invalid_env_var_name() {
    let mut env = BTreeMap::new();
    env.insert("1BAD".to_owned(), simple_decl());
    let error = validate_role_manifest(&manifest_with(env)).unwrap_err();
    assert!(
        error.to_string().contains("1BAD"),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_non_interactive_variable_without_default() {
    let mut env = BTreeMap::new();
    env.insert(
        "TOKEN".to_owned(),
        EnvVarDecl {
            default_value: None,
            interactive: false,
            skippable: false,
            prompt: None,
            options: Vec::new(),
            depends_on: Vec::new(),
        },
    );
    let error = validate_role_manifest(&manifest_with(env)).unwrap_err();
    assert!(
        error.to_string().contains("default value"),
        "unexpected error: {error}"
    );
}

#[test]
fn accepts_minimal_valid_manifest() {
    let mut env = BTreeMap::new();
    env.insert("TOKEN".to_owned(), simple_decl());
    let warnings = validate_role_manifest(&manifest_with(env)).unwrap();
    assert!(warnings.is_empty());
}
