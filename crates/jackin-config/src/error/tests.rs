// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::ConfigError;

#[test]
fn message_formats_display_and_debug_values() {
    let name = "project";
    let path = "/workspace/project";
    assert_eq!(
        ConfigError::msg(format_args!("workspace {name:?} at {path}")).to_string(),
        "workspace \"project\" at /workspace/project"
    );
}

#[test]
fn message_preserves_literal_braces_and_format_looking_data() {
    let text = r#"env = { NAME = "{value:?}" }"#;
    assert_eq!(ConfigError::msg(format_args!("{text}")).to_string(), text);
    assert_eq!(
        ConfigError::msg(format_args!("env = {{ NAME = \"{{value:?}}\" }}")).to_string(),
        text
    );
}
