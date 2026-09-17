// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn ignores_irrelevant_variables() {
    let parsed = parse_zshrc_source("PATH=/usr/bin\nFOO=bar\nexport EDITOR=vim\n");
    assert!(parsed.values.is_empty());
    assert!(parsed.unresolved.is_empty());
}

#[test]
fn parses_plain_and_export_assignments() {
    let parsed =
        parse_zshrc_source("ANTHROPIC_API_KEY=sk-ant-1\nexport OPENAI_API_KEY=sk-openai-2\n");
    assert_eq!(
        parsed.values.get("ANTHROPIC_API_KEY").map(String::as_str),
        Some("sk-ant-1")
    );
    assert_eq!(
        parsed.values.get("OPENAI_API_KEY").map(String::as_str),
        Some("sk-openai-2")
    );
    assert!(parsed.unresolved.is_empty());
}

#[test]
fn unquotes_simple_single_and_double_quotes() {
    let parsed =
        parse_zshrc_source("OPENAI_API_KEY=\"a\\\"b\\\\c\"\nANTHROPIC_API_KEY='lit $HOME `x`'\n");
    assert_eq!(
        parsed.values.get("OPENAI_API_KEY").map(String::as_str),
        Some("a\"b\\c")
    );
    assert_eq!(
        parsed.values.get("ANTHROPIC_API_KEY").map(String::as_str),
        Some("lit $HOME `x`")
    );
}

#[test]
fn dollar_expansions_are_unresolved() {
    let parsed = parse_zshrc_source(
        "CLAUDE_CONFIG_DIR=$HOME/.claude\nCODEX_HOME=${XDG_DATA_HOME}/codex\nOPENAI_API_KEY=a$Bc\n",
    );
    assert!(parsed.values.is_empty());
    assert_eq!(parsed.unresolved.len(), 3);
    assert!(
        parsed
            .unresolved
            .iter()
            .all(|e| e.kind == UnresolvedKind::UnresolvableExpansion)
    );
    assert_eq!(parsed.unresolved[0].detail, "$HOME");
    assert_eq!(parsed.unresolved[1].detail, "${XDG_DATA_HOME}");
    assert_eq!(parsed.unresolved[2].detail, "$Bc");
}

#[test]
fn op_read_is_typed() {
    let parsed = parse_zshrc_source("KIMI_API_KEY=$(op read op://vault/item/field)\n");
    assert!(parsed.values.is_empty());
    assert_eq!(parsed.unresolved.len(), 1);
    assert_eq!(parsed.unresolved[0].kind, UnresolvedKind::OpRead);
    assert_eq!(
        parsed.unresolved[0].detail,
        "$(op read op://vault/item/field)"
    );
}

#[test]
fn defined_function_calls_are_typed() {
    let parsed = parse_zshrc_source(
        "get_key() { echo hi; }\nfunction fetch_key { echo hi; }\nXAI_API_KEY=$(get_key xai)\nZHIPU_API_KEY=`fetch_key zai`\n",
    );
    assert!(parsed.values.is_empty());
    assert_eq!(parsed.unresolved.len(), 2);
    assert!(
        parsed
            .unresolved
            .iter()
            .all(|e| e.kind == UnresolvedKind::FunctionCall)
    );
}

#[test]
fn unknown_commands_are_command_substitutions() {
    let parsed =
        parse_zshrc_source("ZHIPU_API_KEY=$(cat ~/.key)\nOPENAI_API_KEY=`brew --prefix`/bin\n");
    assert!(parsed.values.is_empty());
    assert_eq!(parsed.unresolved.len(), 2);
    assert!(
        parsed
            .unresolved
            .iter()
            .all(|e| e.kind == UnresolvedKind::CommandSubstitution)
    );
}

#[test]
fn semicolons_inside_substitutions_do_not_split() {
    let parsed = parse_zshrc_source("OPENAI_API_KEY=$(echo a; echo b)\n");
    assert!(parsed.values.is_empty());
    assert_eq!(parsed.unresolved.len(), 1);
    assert_eq!(
        parsed.unresolved[0].kind,
        UnresolvedKind::CommandSubstitution
    );
    assert_eq!(parsed.unresolved[0].detail, "$(echo a; echo b)");
}

#[test]
fn arithmetic_tilde_and_globs_are_unresolved() {
    let parsed = parse_zshrc_source(
        "OPENAI_API_KEY=$((1 + 2))\nCODEX_HOME=~/.codex\nANTHROPIC_API_KEY=sk-*\n",
    );
    assert!(parsed.values.is_empty());
    assert_eq!(parsed.unresolved.len(), 3);
    assert!(
        parsed
            .unresolved
            .iter()
            .all(|e| e.kind == UnresolvedKind::UnresolvableExpansion)
    );
    assert_eq!(parsed.unresolved[1].detail, "~");
}

#[test]
fn handles_comments_semicolons_and_continuations() {
    let parsed = parse_zshrc_source(
        "# leading comment\nOPENAI_API_KEY=sk # trailing\nANTHROPIC_API_KEY=\"a#b\";export KIMI_API_KEY=sk-\\\nabc\n",
    );
    assert_eq!(
        parsed.values.get("OPENAI_API_KEY").map(String::as_str),
        Some("sk")
    );
    assert_eq!(
        parsed.values.get("ANTHROPIC_API_KEY").map(String::as_str),
        Some("a#b")
    );
    assert_eq!(
        parsed.values.get("KIMI_API_KEY").map(String::as_str),
        Some("sk-abc")
    );
    assert!(parsed.unresolved.is_empty());
}

#[test]
fn handles_multiple_and_bare_exports() {
    let parsed = parse_zshrc_source(
        "export OPENAI_API_KEY=a ANTHROPIC_API_KEY=b\nexport FOO\ntypeset -x KIMI_API_KEY=c\n",
    );
    assert_eq!(parsed.values.len(), 3);
    assert!(parsed.unresolved.is_empty());
}

#[test]
fn dynamic_irrelevant_variables_are_ignored() {
    let parsed = parse_zshrc_source("PATH=$(brew --prefix)/bin\nFOO=$HOME/x\n");
    assert!(parsed.values.is_empty());
    assert!(parsed.unresolved.is_empty());
}

#[test]
fn later_assignment_wins() {
    let parsed = parse_zshrc_source("OPENAI_API_KEY=first\nOPENAI_API_KEY=second\n");
    assert_eq!(
        parsed.values.get("OPENAI_API_KEY").map(String::as_str),
        Some("second")
    );
}

#[test]
fn append_and_array_assignments_are_unresolved() {
    let parsed = parse_zshrc_source("ANTHROPIC_API_KEY+=extra\nOPENAI_API_KEY=(a b)\n");
    assert!(parsed.values.is_empty());
    assert_eq!(parsed.unresolved.len(), 2);
    assert!(
        parsed
            .unresolved
            .iter()
            .all(|e| e.kind == UnresolvedKind::UnresolvableExpansion)
    );
}

#[test]
fn unescapes_backslashes() {
    let parsed = parse_zshrc_source("OPENAI_API_KEY=a\\ b\n");
    assert_eq!(
        parsed.values.get("OPENAI_API_KEY").map(String::as_str),
        Some("a b")
    );
}

#[test]
fn relevance_matrix() {
    for name in [
        "CLAUDE_CONFIG_DIR",
        "CODEX_HOME",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "KIMI_API_KEY",
        "ANTHROPIC_BASE_URL",
        "OPENAI_BASE_URL",
        "KIMI_BASE_URL",
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "AWS_PROFILE",
        "CLAUDE_CODE_OAUTH_TOKEN",
    ] {
        assert!(is_account_relevant(name), "{name} should be relevant");
    }
    for name in ["PATH", "EDITOR", "FOO", "MY_API_KEYS", "BASE_URLS"] {
        assert!(!is_account_relevant(name), "{name} should be ignored");
    }
}

#[test]
fn reports_line_numbers() {
    let parsed = parse_zshrc_source("FOO=1\nOPENAI_API_KEY=$X\n");
    assert_eq!(parsed.unresolved.len(), 1);
    assert_eq!(parsed.unresolved[0].line, 2);
    assert_eq!(parsed.unresolved[0].name, "OPENAI_API_KEY");
}
