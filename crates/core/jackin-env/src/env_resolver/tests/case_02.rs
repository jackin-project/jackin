// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn resolve_env_error_prompt_required_message_parity() {
    let err = ResolveEnvError::PromptRequired { name: "FOO".into() };
    assert_eq!(
        err.to_string(),
        "env var FOO: required prompt cannot be skipped"
    );
}
